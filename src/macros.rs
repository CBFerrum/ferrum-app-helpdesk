//! Saved replies with merge fields, and macros: several changes to a ticket in one click.
//!
//! A saved reply is a record the workspace owns; its `{{ … }}` fields are filled in from the ticket by one
//! function (`merge`) wherever a reply or a macro's text goes out. A macro is a record of steps run in order
//! on a ticket as the person who applies it, in one save: either every step applies or none does (a
//! refusal of any step, or of the ticket's own rules for the result, leaves the ticket as it was).

use crate::{linked, outgoing};
use ferrum_domain::util::{id, rows, s, sys};
use ferrum_kernel::{Doc, Error, Filter, HookCtx, Result};
use serde_json::{Value, json};
use uuid::Uuid;

const STATUSES: [&str; 5] = ["Open", "Replied", "On Hold", "Resolved", "Closed"];
const PRIORITIES: [&str; 4] = ["Low", "Medium", "High", "Urgent"];

/// What a merge field can name.
pub fn fields(ticket: &Doc, customer: Option<&Doc>, agent: &str, link: Option<&str>) -> Value {
    let customer_name = customer.and_then(|c| s(c, "customer_name")).or(s(ticket, "contact_name")).unwrap_or_default();
    let first = customer_name.split_whitespace().next().unwrap_or_default();
    let agent_first = agent.split_whitespace().next().unwrap_or(agent);
    json!({
        "customer": {"name": customer_name, "first_name": first, "company": customer.and_then(|c| s(c, "customer_name")).unwrap_or_default()},
        "ticket": {
            "number": s(ticket, "number").unwrap_or_default(), "subject": s(ticket, "subject").unwrap_or_default(), "priority": s(ticket, "priority").unwrap_or_default(),
            "status": s(ticket, "status").unwrap_or_default(), "link": link.unwrap_or_default(),
        },
        "agent": {"name": agent, "first_name": agent_first},
    })
}

/// `text` with its `{{ field }}` placeholders filled in; a field that is not there is empty, and text that is not
/// a template (a stray `{{`) is left as it was.
pub fn merge(text: &str, context: &Value) -> String {
    let env = minijinja::Environment::new();
    env.render_str(text, context).unwrap_or_else(|_| text.to_string())
}

/// The ticket's fields for a person's reply: its customer and the ticket's address in the app.
pub async fn context(hc: &mut HookCtx<'_>, ticket: &Doc) -> Result<Value> {
    let customer = match id(ticket, "customer") {
        Some(c) => linked(hc.kernel, hc.ctx, hc.tx, "Customer", c).await?,
        None => None,
    };
    let link = match id(ticket, "id") {
        Some(t) => outgoing::link_base(hc.kernel, hc.ctx.tenant).await.map(|b| format!("{b}/open/Ticket/{t}")),
        None => None,
    };
    Ok(fields(ticket, customer.as_ref(), &hc.ctx.full_name, link.as_deref()))
}

/// The saved reply `doc`, filled in for the ticket named in `params`.
pub async fn render_reply(hc: &mut HookCtx<'_>, reply: &Doc, params: &Value) -> Result<Value> {
    let ticket: Uuid = params.get("ticket").and_then(Value::as_str).and_then(|t| t.parse().ok()).ok_or_else(|| Error::field("ticket", "name the ticket"))?;
    let t = hc.kernel.get_tx(hc.ctx, hc.tx, "Ticket", ticket, false).await?;
    let ctx = context(hc, &t).await?;
    Ok(json!({"body": merge(s(reply, "body").unwrap_or_default(), &ctx)}))
}

fn bad(n: usize, why: impl std::fmt::Display) -> Error {
    Error::field("macro", format!("step {n}: {why}"))
}

/// `apply_macro` on `ticket`: runs the macro's steps in one save, as the person who applies it.
pub async fn apply(hc: &mut HookCtx<'_>, ticket: &Doc, params: &Value) -> Result<Value> {
    let ticket_id = id(ticket, "id").ok_or_else(|| Error::internal("a ticket without an id"))?;
    let macro_id: Uuid = params.get("macro").and_then(Value::as_str).and_then(|m| m.parse().ok()).ok_or_else(|| Error::field("macro", "choose a macro"))?;
    // Read as the person: a personal macro of someone else's is not there for them.
    let m = hc.kernel.get_tx(hc.ctx, hc.tx, "SupportMacro", macro_id, true).await.map_err(|e| match e {
        Error::NotFound(_) | Error::Forbidden(_) => Error::field("macro", "there is no such macro for you"),
        other => other,
    })?;
    let sctx = sys(hc.ctx);
    let merge_ctx = context(hc, ticket).await?;
    let mut patch = Doc::new();
    let mut conversation: Vec<Value> = rows(ticket, "conversation").into_iter().map(Value::Object).collect();
    let before = conversation.len();
    let (mut add, mut remove) = (Vec::<String>::new(), Vec::<String>::new());
    let steps = rows(&m, "steps");
    for (i, step) in steps.iter().enumerate() {
        let n = i + 1;
        let value = s(step, "value").map(str::trim).unwrap_or_default();
        match s(step, "step").unwrap_or_default() {
            "Set Status" if STATUSES.contains(&value) => drop(patch.insert("status".into(), json!(value))),
            "Set Status" => return Err(bad(n, format!("{value:?} is not a status"))),
            "Set Priority" if PRIORITIES.contains(&value) => drop(patch.insert("priority".into(), json!(value))),
            "Set Priority" => return Err(bad(n, format!("{value:?} is not a priority"))),
            "Set Team" => {
                let team = hc.kernel.find_tx(&sctx, hc.tx, "SupportTeam", &[Filter::eq("team_name", value)], None, 1).await?;
                let team = team.first().and_then(|t| id(t, "id")).ok_or_else(|| bad(n, format!("there is no team {value:?}")))?;
                patch.insert("team".into(), json!(team.to_string()));
            }
            "Assign To" => drop(patch.insert("assigned_to".into(), if value.is_empty() { Value::Null } else { json!(value.to_lowercase()) })),
            "Set Type" => {
                let kind = hc.kernel.find_tx(&sctx, hc.tx, "TicketType", &[Filter::eq("type_name", value)], None, 1).await?;
                let kind = kind.first().and_then(|t| id(t, "id")).ok_or_else(|| bad(n, format!("there is no ticket type {value:?}")))?;
                patch.insert("ticket_type".into(), json!(kind.to_string()));
            }
            "Set Language" => drop(patch.insert("language".into(), if value.is_empty() { Value::Null } else { json!(value) })),
            "Set Resolution" => drop(patch.insert("resolution_details".into(), json!(merge(value, &merge_ctx)))),
            "Add Tag" | "Remove Tag" => {
                let tag = crate::tags::clean(value).ok_or_else(|| bad(n, format!("{value:?} is not a tag")))?;
                if s(step, "step") == Some("Add Tag") { add.push(tag) } else { remove.push(tag) }
            }
            kind @ ("Public Reply" | "Private Note") => {
                if value.is_empty() {
                    return Err(bad(n, "a reply or note needs its text"));
                }
                conversation
                    .push(json!({"author_kind": "Agent", "author": hc.ctx.email, "body": merge(value, &merge_ctx), "is_internal": kind == "Private Note"}));
            }
            other => return Err(bad(n, format!("{other:?} is not a step"))),
        }
    }
    if conversation.len() > before {
        patch.insert("conversation".into(), Value::Array(conversation));
    }
    if !patch.is_empty() {
        hc.kernel.update_tx(hc.ctx, hc.tx, "Ticket", ticket_id, patch, None).await?;
    }
    // As the person who pressed it, through the platform's call: whoever may change the ticket, tags kept lowercase, at most twenty.
    if !add.is_empty() || !remove.is_empty() {
        hc.kernel.tag_tx(hc.ctx, hc.tx, "Ticket", ticket_id, &add, &remove).await?;
    }
    let mut counted = Doc::new();
    counted.insert("uses".into(), json!(crate::int(&m, "uses") + 1));
    hc.kernel.set_values_tx(&sctx, hc.tx, "SupportMacro", macro_id, counted).await?;
    Ok(json!({"applied": steps.len(), "macro": s(&m, "macro_name")}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_fields_come_from_the_ticket_and_unknown_ones_are_empty() {
        let ticket = json!({"number": "TKT-1", "subject": "Jam", "priority": "High", "status": "Open", "contact_name": "Ada Lovelace"});
        let customer = json!({"customer_name": "Ada Ltd"});
        let ctx = fields(ticket.as_object().unwrap(), customer.as_object(), "Sam Jones", Some("https://x.test/t/1"));
        assert_eq!(
            merge("Hi {{customer.first_name}} ({{ customer.company }}), {{ticket.number}}: {{ticket.subject}}.\n{{agent.first_name}}", &ctx),
            "Hi Ada (Ada Ltd), TKT-1: Jam.\nSam"
        );
        assert_eq!(merge("Hi {{customer.nickname}}!", &ctx), "Hi !");
        assert_eq!(merge("Look: {{ unclosed", &ctx), "Look: {{ unclosed", "text that is no template stays as written");
        let bare = fields(json!({"contact_name": "Zed"}).as_object().unwrap(), None, "Sam", None);
        assert_eq!(merge("{{customer.first_name}} {{customer.company}}|{{ticket.link}}", &bare), "Zed |");
    }
}
