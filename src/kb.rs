//! Knowledge base articles: the publication date, the versions that were published, translations (an article in another
//! language linked to its original, one level), restoring a published version, and readers' helpful / not helpful votes.

use async_trait::async_trait;
use chrono::{SecondsFormat, Utc};
use ferrum_domain::util::{id, rows, s, set, sys};
use ferrum_kernel::{Doc, DocHook, Error, FieldError, HookCtx, Result, store};
use serde_json::{Value, json};
use uuid::Uuid;

/// How many published versions an article keeps.
const VERSIONS_KEPT: usize = 50;

/// The highest version number among an article's versions (0 when none).
fn newest(doc: &Doc) -> i64 {
    rows(doc, "versions").iter().filter_map(|v| v.get("version_no").and_then(Value::as_i64)).max().unwrap_or(0)
}

pub struct ArticleHook;

#[async_trait]
impl DocHook for ArticleHook {
    async fn before_validate(&self, hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
        // A language tag is kept trimmed and in lower case; empty is none.
        let language = s(doc, "language").map(|l| l.trim().to_lowercase()).filter(|l| !l.is_empty());
        set(doc, "language", language.map_or(Value::Null, Value::String));
        // An article's address is made from its title once, and stays.
        if !crate::help::has_slug(doc) {
            let me = id(doc, "id");
            let stored: Option<String> = match me {
                Some(m) => sqlx::query_scalar("select slug from e_kb_article where tenant_id = $1 and id = $2")
                    .bind(hc.ctx.tenant)
                    .bind(m)
                    .fetch_optional(&mut **hc.tx)
                    .await?
                    .flatten(),
                None => None,
            };
            match stored {
                Some(existing) => crate::help::address(doc, existing),
                None => {
                    let base = crate::help::slug_of(s(doc, "title").unwrap_or_default());
                    let free = crate::help::free_slug(hc.tx, hc.ctx.tenant, me, &base).await?;
                    crate::help::address(doc, free);
                }
            }
        }
        if s(doc, "status") != Some("Published") {
            return Ok(());
        }
        let stored = match id(doc, "id") {
            Some(i) => store::load(hc.tx, &hc.ctx.schema, hc.entity, hc.ctx.tenant, i, false).await?,
            None => None,
        };
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let was_published = stored.as_ref().is_some_and(|o| s(o, "status") == Some("Published"));
        if !was_published {
            set(doc, "published_at", now.clone());
        }
        // What customers can read is kept as a version each time it is published or changed while published. Only this
        // hook writes `versions` (clients cannot), under the article's row lock, so numbers only grow.
        let changed = |f: &str| stored.as_ref().is_none_or(|o| o.get(f).filter(|v| !v.is_null()) != doc.get(f).filter(|v| !v.is_null()));
        if !was_published || ["title", "summary", "content"].into_iter().any(changed) {
            // Oldest first (stored lines need not come back in that order), so the oldest are the ones let go.
            let mut versions = rows(doc, "versions");
            versions.sort_by_key(|v| v.get("version_no").and_then(Value::as_i64).unwrap_or(0));
            versions.push(
                json!({"id": Uuid::now_v7().to_string(), "version_no": newest(doc) + 1, "published_at": now, "published_by": hc.ctx.email, "title": doc.get("title"), "summary": doc.get("summary"), "content": doc.get("content")})
                    .as_object()
                    .cloned()
                    .unwrap_or_default(),
            );
            let drop = versions.len().saturating_sub(VERSIONS_KEPT);
            set(doc, "versions", Value::Array(versions.into_iter().skip(drop).map(Value::Object).collect()));
        }
        Ok(())
    }

    async fn validate(&self, hc: &mut HookCtx<'_>, doc: &Doc) -> Result<()> {
        let Some(original) = id(doc, "translation_of") else { return Ok(()) };
        let mut errors = Vec::new();
        let me = id(doc, "id");
        if me == Some(original) {
            errors.push(FieldError::new("translation_of", "an article is not its own translation"));
        } else {
            match store::load(hc.tx, &hc.ctx.schema, hc.entity, hc.ctx.tenant, original, false).await? {
                None => errors.push(FieldError::new("translation_of", "there is no such article")),
                Some(o) => {
                    if id(&o, "translation_of").is_some() {
                        errors.push(FieldError::new("translation_of", "is a translation itself: translate the original"));
                    }
                    if s(&o, "language").is_some() && s(&o, "language") == s(doc, "language") {
                        errors.push(FieldError::new("language", "is the original's language"));
                    }
                }
            }
            if let Some(me) = me {
                let has: bool = sqlx::query_scalar("select exists (select 1 from e_kb_article where tenant_id = $1 and translation_of = $2)")
                    .bind(hc.ctx.tenant)
                    .bind(me)
                    .fetch_one(&mut **hc.tx)
                    .await?;
                if has {
                    errors.push(FieldError::new("translation_of", "has translations of its own: it cannot become one"));
                }
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(Error::Validation(errors)) }
    }

    async fn action(&self, hc: &mut HookCtx<'_>, name: &str, doc: &Doc, params: &Value) -> Result<Option<Value>> {
        if name == "restore_version" {
            return restore(hc, doc, params).await.map(Some);
        }
        let field = match name {
            "mark_helpful" => "helpful_count",
            "mark_not_helpful" => "not_helpful_count",
            _ => return Ok(None),
        };
        // The kernel loaded the article locked for this action, so concurrent votes queue up.
        let article = id(doc, "id").ok_or_else(|| Error::internal("article without id"))?;
        let mut v = Doc::new();
        v.insert(field.into(), json!(doc.get(field).and_then(Value::as_i64).unwrap_or(0) + 1));
        hc.kernel.set_values_tx(&sys(hc.ctx), hc.tx, "KbArticle", article, v.clone()).await?;
        Ok(Some(Value::Object(v)))
    }
}

/// `restore_version`: the article's title, summary and text become that version's, saved as the caller (an ordinary edit, so
/// the article's rules run and, if it is published, the restored text is its newest version). Nothing is removed.
async fn restore(hc: &mut HookCtx<'_>, doc: &Doc, params: &Value) -> Result<Value> {
    let article = id(doc, "id").ok_or_else(|| Error::internal("article without id"))?;
    let wanted = params.get("version").and_then(Value::as_i64).ok_or_else(|| Error::field("version", "name a version by its number"))?;
    let version = rows(doc, "versions")
        .into_iter()
        .find(|v| v.get("version_no").and_then(Value::as_i64) == Some(wanted))
        .ok_or_else(|| Error::field("version", "there is no such version"))?;
    let mut patch = Doc::new();
    for f in ["title", "summary", "content"] {
        patch.insert(f.into(), version.get(f).cloned().unwrap_or(Value::Null));
    }
    let saved = hc.kernel.update_tx(hc.ctx, hc.tx, "KbArticle", article, patch, None).await?;
    let latest = newest(&saved);
    Ok(json!({"restored": wanted, "version_no": if latest > 0 { json!(latest) } else { Value::Null }}))
}
