//! Splitting a customer's message off into a ticket of its own. The message stays where it is (a conversation is
//! append-only) and is marked with the new ticket's number; the new ticket is the person's, made with their rights, for
//! the same customer and contact, and links to the ticket it came from, which keeps the message's files.

use crate::child_table;
use ferrum_domain::flow::created;
use ferrum_domain::util::{b, id, rows, s};
use ferrum_kernel::{Doc, Error, HookCtx, Result};
use serde_json::{Value, json};
use sqlx::AssertSqlSafe;
use uuid::Uuid;

/// What the new ticket takes from the one it is split from.
const COPIED: [&str; 9] = ["priority", "ticket_type", "channel", "customer", "contact_name", "contact_email", "cc", "language", "team"];

/// `split_message` on `ticket`: `message` names a public message from the customer, `subject` the new ticket's.
pub(crate) async fn split(hc: &mut HookCtx<'_>, ticket: &Doc, params: &Value) -> Result<Value> {
    let source = id(ticket, "id").ok_or_else(|| Error::internal("a ticket without an id"))?;
    let wanted = params.get("message").and_then(Value::as_str).unwrap_or_default();
    let not_one = || Error::field("message", "name a public message from the customer in this ticket");
    let message = rows(ticket, "conversation")
        .into_iter()
        .find(|m| s(m, "id") == Some(wanted))
        .filter(|m| !b(m, "is_internal") && s(m, "author_kind") == Some("Customer"))
        .ok_or_else(not_one)?;
    let message_id: Uuid = wanted.parse().map_err(|_| not_one())?;
    let already = |n: &str| Error::invalid(format!("this message was split off already, into {n}"));
    if let Some(n) = s(&message, "split_into") {
        return Err(already(n));
    }
    let subject = params.get("subject").and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty());
    if subject.is_some_and(|v| v.chars().count() > 300) {
        return Err(Error::field("subject", "at most 300 characters"));
    }
    let body = s(&message, "body").unwrap_or_default();
    let mut input = Doc::new();
    input.insert("subject".into(), json!(subject.map_or_else(|| crate::ticket::preview(body), str::to_string)));
    input.insert("description".into(), json!(body));
    for f in COPIED {
        if let Some(v) = ticket.get(f).filter(|v| !v.is_null()) {
            input.insert(f.into(), v.clone());
        }
    }
    input.insert("links".into(), json!([{"ticket": source.to_string()}]));
    // As the person: their right to create tickets and their teams apply.
    let new = hc.kernel.insert_tx(hc.ctx, hc.tx, "Ticket", input).await?;
    let number = s(&new, "number").unwrap_or_default().to_string();
    let table = child_table(hc.ctx, "Ticket", "conversation")?;
    let marked =
        sqlx::query(AssertSqlSafe(format!("update {table} set split_into = $1 where tenant_id = $2 and parent_id = $3 and id = $4 and split_into is null")))
            .bind(&number)
            .bind(hc.ctx.tenant)
            .bind(source)
            .bind(message_id)
            .execute(&mut **hc.tx)
            .await?
            .rows_affected();
    if marked == 0 {
        // Split meanwhile by someone else: the new ticket goes with this transaction.
        return Err(already("another ticket"));
    }
    Ok(created("Ticket", &new))
}
