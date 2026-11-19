//! The help centre: knowledge base articles for customers.
//!
//! An article has a visibility: Internal (agents), Customers Only (signed-in customers, in the portal) or Public (the
//! help centre's page, to anyone). Published and Public ones are the platform's published list `help-centre`
//! (a record page for each article by its `slug`, search and paging, a sitemap), made once for a workspace by the
//! upkeep and the workspace's own to change or take down. Customers in the portal can search articles, read them
//! (a view is counted), be offered the ones that fit what they are about to ask, and say whether one helped.
//!
//! The published list counts a visitor's views and "was this helpful" answers itself (`count_views`, `votes`: the
//! platform's own, once a visitor, from the page's own origin), into the same fields the portal counts into, so the
//! article's `views`, `helpful_count` and `not_helpful_count` are the sum of both ways in.

use ferrum_domain::util::{b, id, s, set, sys};
use ferrum_kernel::ctx::RowScope;
use ferrum_kernel::{Action, Ctx, Doc, Error, HookCtx, Kernel, Result, Tx, row_scope};
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

/// The address an article gets: its title in lowercase ASCII words joined by dashes, at most 80 characters.
pub fn slug_of(title: &str) -> String {
    let mut out = String::new();
    for c in title.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let t: String = out.trim_end_matches('-').chars().take(80).collect();
    let t = t.trim_end_matches('-').to_string();
    if t.is_empty() { "article".to_string() } else { t }
}

/// A slug no other article has: `base`, else `base-2`, `base-3`, ….
pub async fn free_slug(tx: &mut Tx, tenant: Uuid, article: Option<Uuid>, base: &str) -> Result<String> {
    let taken: Vec<String> = sqlx::query_scalar("select slug from e_kb_article where tenant_id = $1 and slug like $2 and ($3::uuid is null or id <> $3)")
        .bind(tenant)
        .bind(format!("{base}%"))
        .bind(article)
        .fetch_all(&mut **tx)
        .await?;
    if !taken.iter().any(|t| t == base) {
        return Ok(base.to_string());
    }
    (2..).map(|n| format!("{base}-{n}")).find(|c| !taken.contains(c)).ok_or_else(|| Error::internal("no free address"))
}

/// The list of Public, Published articles the help centre shows.
fn list() -> Value {
    json!({"op": "publish_list", "list": {
        "name": "help-centre", "label": "Help centre", "description": "Answers to common questions",
        "fields": ["title", "summary", "category", "published_at"], "detail": ["content"], "slug": "slug", "summary": "summary",
        "count_views": "views", "votes": {"up": "helpful_count", "down": "not_helpful_count"},
        "filter": [["status", "=", "Published"], ["visibility", "=", "Public"]], "sort": ["published_at", "desc"], "limit": 100}})
}

/// Once for a workspace: the help centre's published list, and the flag in the settings that says it was made (so one
/// an administrator takes down is not made again).
pub async fn ensure_defaults(k: &Kernel, ctx: &Ctx) -> Result<()> {
    let sctx = sys(ctx);
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    let settings = k.find_tx(&sctx, &mut tx, "SupportSettings", &[], None, 1).await?;
    tx.commit().await?;
    if settings.first().is_some_and(|d| b(d, "defaults_made")) {
        return Ok(());
    }
    let op = serde_json::from_value(list()).map_err(|e| Error::internal(format!("the help centre's list: {e}")))?;
    if ctx.schema.entity("KbArticle").is_some()
        && let Err(e) = k.add_customization(&sctx, "KbArticle", op, Some("The help centre: Public, Published articles".into())).await
    {
        tracing::warn!(tenant = %ctx.tenant, "the help centre's list could not be made: {e}");
        return Ok(());
    }
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    let mut flag = Doc::new();
    flag.insert("defaults_made".into(), json!(true));
    match settings.first().and_then(|d| id(d, "id")) {
        Some(row) => k.set_values_tx(&sctx, &mut tx, "SupportSettings", row, flag).await?,
        None => {
            k.insert_tx(&sctx, &mut tx, "SupportSettings", flag).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Words of a request worth looking up: letters and digits, at least three characters, at most eight.
fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric()).map(str::to_lowercase).filter(|w| w.chars().count() >= 3) {
        if !out.contains(&w) && out.len() < 8 {
            out.push(w);
        }
    }
    out
}

/// Published articles customers may read, best fit first: those holding the words of `text` (a title counts most). With no
/// words, the newest. Read as the workspace: only Published articles that are Public or Customers Only are ever here.
pub async fn find(tx: &mut Tx, tenant: Uuid, text: Option<&str>, limit: i64, language: Option<&str>) -> Result<Vec<Value>> {
    search(tx, tenant, text, limit, &["Public", "Customers Only"], None, language).await
}

/// The one search over articles: Published ones of the given visibilities (and, for staff, within their record scope on
/// articles), those holding the words of `text` first (title 3, summary 2, text 1), else the newest. One article per family
/// (an original and its translations): the one in `language` (or its base or a variant of it), else one in no language,
/// else the original; with no language asked, the original.
pub async fn search(
    tx: &mut Tx,
    tenant: Uuid,
    text: Option<&str>,
    limit: i64,
    visibilities: &[&str],
    scope: Option<&RowScope>,
    language: Option<&str>,
) -> Result<Vec<Value>> {
    let language = language.map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty());
    let w = words(text.unwrap_or_default());
    let visibilities: Vec<String> = visibilities.iter().map(|v| v.to_string()).collect();
    type Row = (Uuid, String, Option<String>, Option<String>, Option<String>, Option<String>);
    let mut qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "select id, title, summary, slug, visibility, category_name from (select distinct on (coalesce(a.translation_of, a.id)) \
         a.id, a.title, a.summary, a.slug, a.visibility, c.category_name, a.published_at, ",
    );
    push_score(&mut qb, &w);
    qb.push(
        " as score from e_kb_article a \
         left join e_kb_category c on c.tenant_id = a.tenant_id and c.id = a.category where a.tenant_id = ",
    );
    qb.push_bind(tenant).push(" and a.status = 'Published' and a.visibility = any(").push_bind(visibilities).push(")");
    qb.push(" and (cardinality(").push_bind(w.clone()).push("::text[]) = 0 or exists (select 1 from unnest(").push_bind(w.clone());
    qb.push("::text[]) w where a.title ilike '%' || w || '%' or coalesce(a.summary, '') ilike '%' || w || '%' or a.content ilike '%' || w || '%'))");
    if let Some(scope) = scope {
        scope.push_sql(&mut qb, "a");
    }
    qb.push(" order by coalesce(a.translation_of, a.id), case when ")
        .push_bind(language.clone())
        .push("::text is null or a.language = ")
        .push_bind(language.clone());
    qb.push(" or ")
        .push_bind(language.clone())
        .push(" like a.language || '-%' or a.language like ")
        .push_bind(language)
        .push(" || '-%' then 0 when a.language is null then 1 else 2 end, ");
    qb.push("(a.translation_of is not null), score desc) x order by score desc, published_at desc nulls last, id limit ").push_bind(limit);
    let found: Vec<Row> = qb.build_query_as().fetch_all(&mut **tx).await?;
    Ok(found.into_iter().map(|(i, title, summary, slug, visibility, category)| json!({"id": i, "title": title, "summary": summary, "slug": slug, "visibility": visibility, "category": category})).collect())
}

/// How well article `a` holds the words `w` (title 3, summary 2, text 1 each).
fn push_score(qb: &mut QueryBuilder<Postgres>, w: &[String]) {
    qb.push(
        "(select coalesce(sum((case when a.title ilike '%' || w || '%' then 3 else 0 end) + (case when coalesce(a.summary, '') ilike '%' || w || '%' then 2 else 0 end) \
          + (case when a.content ilike '%' || w || '%' then 1 else 0 end)), 0) from unnest(",
    );
    qb.push_bind(w.to_vec()).push("::text[]) w)");
}

/// `suggest_articles` on a ticket: up to eight published articles for the words given, else the ticket's subject and the
/// customer's latest words, Internal ones included, as far as the caller may read articles. Changes nothing.
pub async fn suggest(hc: &mut HookCtx<'_>, ticket: &Doc, params: &Value) -> Result<Value> {
    let q = params.get("q").and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if q.chars().count() > 200 {
        return Err(Error::field("q", "at most 200 characters"));
    }
    let text = if q.is_empty() {
        format!("{} {}", s(ticket, "subject").unwrap_or_default(), s(ticket, "last_customer_message").unwrap_or_default())
    } else {
        q.to_string()
    };
    let Some(ent) = hc.ctx.schema.entity("KbArticle") else { return Ok(json!({"articles": []})) };
    let scope = match row_scope(hc.ctx, ent, Action::Read) {
        Ok(scope) => scope,
        Err(Error::Forbidden(_)) => return Ok(json!({"articles": []})),
        Err(e) => return Err(e),
    };
    let found = search(hc.tx, hc.ctx.tenant, Some(&text), 8, &["Internal", "Customers Only", "Public"], Some(&scope), s(ticket, "language")).await?;
    Ok(json!({"articles": found}))
}

/// One article for a customer, counting the view (a signed-in customer's: the public page counts its visitors itself).
pub async fn read(tx: &mut Tx, tenant: Uuid, article: Uuid) -> Result<Value> {
    type Row = (Uuid, String, Option<String>, String, Option<String>, i64, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "update e_kb_article a set views = coalesce(views, 0) + 1 where a.tenant_id = $1 and a.id = $2 and a.status = 'Published' and a.visibility in ('Public', 'Customers Only') \
         returning a.id, a.title, a.summary, a.content, (select category_name from e_kb_category c where c.tenant_id = a.tenant_id and c.id = a.category), coalesce(a.views, 0)::bigint, a.language",
    )
    .bind(tenant)
    .bind(article)
    .fetch_optional(&mut **tx)
    .await?;
    let (i, title, summary, content, category, views, language) = row.ok_or_else(|| Error::NotFound("article".into()))?;
    // The other members of its family a customer may read, by language.
    let others: Vec<(Uuid, Option<String>, String)> = sqlx::query_as(
        "select b.id, b.language, b.title from e_kb_article a join e_kb_article b on b.tenant_id = a.tenant_id \
           and coalesce(b.translation_of, b.id) = coalesce(a.translation_of, a.id) and b.id <> a.id \
         where a.tenant_id = $1 and a.id = $2 and b.status = 'Published' and b.visibility in ('Public', 'Customers Only') order by b.language nulls first, b.id",
    )
    .bind(tenant)
    .bind(article)
    .fetch_all(&mut **tx)
    .await?;
    let translations: Vec<Value> = others.into_iter().map(|(id, language, title)| json!({"id": id, "language": language, "title": title})).collect();
    Ok(
        json!({"id": i, "title": title, "summary": summary, "content": content, "category": category, "views": views, "language": language, "translations": translations}),
    )
}

/// "Was this helpful": counted on the article, atomically.
pub async fn vote(tx: &mut Tx, tenant: Uuid, article: Uuid, helpful: bool) -> Result<()> {
    let column = if helpful { "helpful_count" } else { "not_helpful_count" };
    let sql = format!(
        "update e_kb_article set {column} = coalesce({column}, 0) + 1 where tenant_id = $1 and id = $2 and status = 'Published' and visibility in ('Public', 'Customers Only')"
    );
    let n = sqlx::query(sqlx::AssertSqlSafe(sql)).bind(tenant).bind(article).execute(&mut **tx).await?.rows_affected();
    if n == 0 { Err(Error::NotFound("article".into())) } else { Ok(()) }
}

/// The article's address once it is published (and stays).
pub fn address(doc: &mut Doc, slug: String) {
    set(doc, "slug", slug);
}

pub fn has_slug(doc: &Doc) -> bool {
    s(doc, "slug").is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_are_ascii_words() {
        assert_eq!(slug_of("How do I reset my password?"), "how-do-i-reset-my-password");
        assert_eq!(slug_of("  --Wi-Fi: not working!! "), "wi-fi-not-working");
        assert_eq!(slug_of("日本語"), "article");
        assert!(slug_of(&"word ".repeat(40)).len() <= 80);
    }

    #[test]
    fn requests_are_looked_up_by_their_longer_words() {
        assert_eq!(words("My printer is jammed, again! Printer"), ["printer", "jammed", "again"]);
        assert!(words("a to I").is_empty());
    }
}
