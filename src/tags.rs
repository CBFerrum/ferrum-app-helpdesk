//! Tags on tickets: records with a colour (`TicketTag`), put on a ticket by a person (the platform's tags, which
//! lists filter by and merges move) or by a rule (`TagRule`: a condition on the ticket, checked when it is saved).
//! A rule only ever adds a tag; taking one off is a person's.

use async_trait::async_trait;
use ferrum_domain::util::s;
use ferrum_kernel::{Doc, DocHook, Error, HookCtx, Result};
use ferrum_meta::{Expr, Scope};
use serde_json::{Value, json};
use uuid::Uuid;

/// A tag is a lowercase word: letters, digits, dashes and underscores.
pub fn clean(name: &str) -> Option<String> {
    let t = name.trim().to_lowercase();
    (!t.is_empty() && t.chars().count() <= 40 && t.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')).then_some(t)
}

fn colour(v: &str) -> bool {
    v.len() == 7 && v.starts_with('#') && v[1..].chars().all(|c| c.is_ascii_hexdigit())
}

pub struct TagHook;

#[async_trait]
impl DocHook for TagHook {
    async fn before_validate(&self, _hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
        if let Some(name) = s(doc, "tag_name") {
            let cleaned = clean(name).ok_or_else(|| Error::field("tag_name", "lowercase letters, digits, dashes and underscores, at most 40"))?;
            doc.insert("tag_name".into(), json!(cleaned));
        }
        if let Some(c) = s(doc, "colour")
            && !colour(c)
        {
            return Err(Error::field("colour", "use #rrggbb, e.g. #2563eb"));
        }
        Ok(())
    }
}

pub struct RuleHook;

#[async_trait]
impl DocHook for RuleHook {
    async fn validate(&self, _hc: &mut HookCtx<'_>, doc: &Doc) -> Result<()> {
        if let Some(c) = s(doc, "condition") {
            Expr::compile(c).map_err(|m| Error::field("condition", m))?;
        }
        Ok(())
    }
}

/// Puts the tags of every enabled rule whose condition holds for the saved ticket on it, once each.
pub async fn apply(hc: &mut HookCtx<'_>, doc: &Doc, ticket: Uuid) -> Result<()> {
    let rules: Vec<(Uuid, String, String)> = sqlx::query_as(
        "select r.id, r.condition, t.tag_name from e_tag_rule r join e_ticket_tag t on t.tenant_id = r.tenant_id and t.id = r.tag \
         where r.tenant_id = $1 and r.enabled order by r.rule_name",
    )
    .bind(hc.ctx.tenant)
    .fetch_all(&mut **hc.tx)
    .await?;
    if rules.is_empty() {
        return Ok(());
    }
    let view = hc.entity.cel_view(&Value::Object(doc.clone()), &hc.ctx.schema);
    let user = hc.ctx.user_value();
    for (rule, condition, tag) in rules {
        match Expr::compile(&condition).and_then(|e| e.eval_bool(&Scope { doc: &view, old: None, parent: None, user: &user })) {
            Ok(true) => {
                sqlx::query("insert into record_tag (tenant_id, entity, record_id, tag) values ($1, 'Ticket', $2, $3) on conflict do nothing")
                    .bind(hc.ctx.tenant)
                    .bind(ticket)
                    .bind(tag)
                    .execute(&mut **hc.tx)
                    .await?;
            }
            Ok(false) => {}
            Err(e) => tracing::warn!(%rule, "a tag rule's condition failed: {e}"),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_lowercase_words() {
        assert_eq!(clean(" Billing-Q4 ").as_deref(), Some("billing-q4"));
        assert_eq!(clean("two words"), None);
        assert_eq!(clean(""), None);
        assert!(colour("#2563eb") && !colour("2563eb") && !colour("#25eb") && !colour("#zzzzzz"));
    }
}
