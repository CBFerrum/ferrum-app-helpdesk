//! The help centre: knowledge base articles for customers.
//!
//! An article has a visibility: Internal (agents), Customers Only (signed-in customers, in the portal) or Public (the
//! help centre's page, to anyone). Published and Public ones are the platform's published list `help-centre`
//! (a record page for each article by its `slug`, search and paging, a sitemap), made once for a workspace by the
//! upkeep and the workspace's own to change or take down. Customers in the portal can search articles, read them
//! (a view is counted), be offered the ones that fit what they are about to ask, and say whether one helped.
//!
//! What the platform's public pages do not offer is a way to count a view or a vote from a visitor who is not signed
//! in: counts here are the portal's.

use ferrum_domain::util::{b, id, s, set, sys};
use ferrum_kernel::{Ctx, Doc, Error, Kernel, Result, Tx};
use serde_json::{Value, json};
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
pub async fn find(tx: &mut Tx, tenant: Uuid, text: Option<&str>, limit: i64) -> Result<Vec<Value>> {
    let w = words(text.unwrap_or_default());
    type Row = (Uuid, String, Option<String>, Option<String>, Option<String>, Option<String>);
    let found: Vec<Row> = sqlx::query_as(
        "select a.id, a.title, a.summary, a.slug, a.visibility, c.category_name from e_kb_article a left join e_kb_category c on c.tenant_id = a.tenant_id and c.id = a.category \
         where a.tenant_id = $1 and a.status = 'Published' and a.visibility in ('Public', 'Customers Only') \
           and (cardinality($2::text[]) = 0 or exists (select 1 from unnest($2::text[]) w where a.title ilike '%' || w || '%' or coalesce(a.summary, '') ilike '%' || w || '%' or a.content ilike '%' || w || '%')) \
         order by (select coalesce(sum((case when a.title ilike '%' || w || '%' then 3 else 0 end) + (case when coalesce(a.summary, '') ilike '%' || w || '%' then 2 else 0 end) \
                                    + (case when a.content ilike '%' || w || '%' then 1 else 0 end)), 0) from unnest($2::text[]) w) desc, a.published_at desc nulls last, a.id limit $3",
    )
    .bind(tenant)
    .bind(&w)
    .bind(limit)
    .fetch_all(&mut **tx)
    .await?;
    Ok(found.into_iter().map(|(i, title, summary, slug, visibility, category)| json!({"id": i, "title": title, "summary": summary, "slug": slug, "visibility": visibility, "category": category})).collect())
}

/// One article for a customer, counting the view.
pub async fn read(tx: &mut Tx, tenant: Uuid, article: Uuid) -> Result<Value> {
    type Row = (Uuid, String, Option<String>, String, Option<String>, i64);
    let row: Option<Row> = sqlx::query_as(
        "update e_kb_article a set views = coalesce(views, 0) + 1 where a.tenant_id = $1 and a.id = $2 and a.status = 'Published' and a.visibility in ('Public', 'Customers Only') \
         returning a.id, a.title, a.summary, a.content, (select category_name from e_kb_category c where c.tenant_id = a.tenant_id and c.id = a.category), coalesce(a.views, 0)::bigint",
    )
    .bind(tenant)
    .bind(article)
    .fetch_optional(&mut **tx)
    .await?;
    let (i, title, summary, content, category, views) = row.ok_or_else(|| Error::NotFound("article".into()))?;
    Ok(json!({"id": i, "title": title, "summary": summary, "content": content, "category": category, "views": views}))
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
