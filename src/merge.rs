//! Merging duplicate tickets into one: the conversation, files, comments, watchers and tags of each
//! duplicate move onto the surviving ticket (the conversation in time order, each message marked with
//! the ticket it came from), and the duplicate closes as merged, linking to the survivor, which is
//! where a customer's reply to it lands (`site.rs`). Who merged and when is a note in both
//! conversations and the duplicate's `merged_*` fields, besides the audit entry of the action.
//!
//! The platform's record merge (`mergeable`, `ferrum_kernel::merge`) moves the same comments, files,
//! tags and followers (the statements below are its own), but it makes the duplicate an alias that
//! lists no longer show and does not move a record's own lines, so it is not used: a merged ticket
//! stays here, closed. A merge is not undone: a closed ticket is final and the messages are the
//! survivor's now.

use chrono::Utc;
use ferrum_domain::util::{id, rows, s, sys};
use ferrum_kernel::{Action, Doc, Error, HookCtx, Result, row_scope};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, QueryBuilder};
use uuid::Uuid;

/// What hangs off a ticket and moves with it: comments, assignments and files, once each.
const MOVED: [&str; 3] = ["record_comment", "record_assignment", "file_object"];
/// What both tickets may have, kept once: tags and watchers, by the column that names them.
const SHARED: [(&str, &str); 2] = [("record_tag", "tag"), ("record_follower", "user_id")];

fn obj(v: Value) -> Doc {
    v.as_object().cloned().unwrap_or_default()
}

/// `merge_tickets` on the ticket that stays: `numbers` names the duplicates.
pub(crate) async fn merge(hc: &mut HookCtx<'_>, survivor: &Doc, params: &Value) -> Result<Value> {
    let keep = id(survivor, "id").ok_or_else(|| Error::internal("a ticket without an id"))?;
    if s(survivor, "status") == Some("Closed") {
        return Err(Error::invalid("a closed ticket is final: merge into one that is open"));
    }
    let mut numbers: Vec<String> = params
        .get("numbers")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|n| !n.is_empty())
        .map(str::to_string)
        .collect();
    numbers.dedup();
    if numbers.is_empty() {
        return Err(Error::field("numbers", "name the tickets to merge into this one"));
    }
    let ent = hc.ctx.schema.entity("Ticket").ok_or_else(|| Error::NotFound("Ticket".into()))?;
    let (tenant, system, who, now) = (hc.ctx.tenant, sys(hc.ctx), hc.ctx.email.to_lowercase(), Utc::now());
    let mine = s(survivor, "number").unwrap_or("the surviving ticket").to_string();
    let mut moved = 0u64;
    for number in &numbers {
        let mut find = QueryBuilder::new("select a.id from e_ticket a where a.tenant_id = ");
        find.push_bind(tenant);
        row_scope(hc.ctx, ent, Action::Write)?.push_sql(&mut find, "a");
        find.push(" and a.number = ").push_bind(number.clone());
        let dup: Uuid = find
            .build_query_scalar()
            .fetch_optional(&mut **hc.tx)
            .await?
            .ok_or_else(|| Error::field("numbers", format!("{number} is not a ticket you may merge")))?;
        let d = hc.kernel.get_tx(&system, hc.tx, "Ticket", dup, true).await?;
        let bad = |m: &str| Error::field("numbers", format!("{number} {m}"));
        if dup == keep {
            return Err(bad("is this ticket: a ticket is not merged into itself"));
        }
        if s(&d, "status") == Some("Closed") {
            return Err(bad("is closed, and a closed ticket is final"));
        }
        let same = match (id(&d, "customer"), id(survivor, "customer")) {
            (Some(a), Some(b)) => a == b,
            (None, None) => s(&d, "contact_email").zip(s(survivor, "contact_email")).is_some_and(|(a, b)| a.eq_ignore_ascii_case(b)),
            _ => false,
        };
        if !same {
            return Err(bad("is another customer's: only tickets of one customer are merged"));
        }
        let was = s(&d, "number").unwrap_or(number).to_string();
        moved +=
            sqlx::query("update e_ticket__conversation set parent_id = $1, merged_from = coalesce(merged_from, $2) where tenant_id = $3 and parent_id = $4")
                .bind(keep)
                .bind(&was)
                .bind(tenant)
                .bind(dup)
                .execute(&mut **hc.tx)
                .await?
                .rows_affected();
        for table in MOVED {
            sqlx::query(AssertSqlSafe(format!("update {table} set record_id = $1 where tenant_id = $2 and entity = 'Ticket' and record_id = $3")))
                .bind(keep)
                .bind(tenant)
                .bind(dup)
                .execute(&mut **hc.tx)
                .await?;
        }
        for (table, column) in SHARED {
            sqlx::query(AssertSqlSafe(format!(
                "delete from {table} d where d.tenant_id = $2 and d.entity = 'Ticket' and d.record_id = $3 \
                 and exists (select 1 from {table} k where k.tenant_id = d.tenant_id and k.entity = d.entity and k.record_id = $1 and k.{column} = d.{column})"
            )))
            .bind(keep)
            .bind(tenant)
            .bind(dup)
            .execute(&mut **hc.tx)
            .await?;
            sqlx::query(AssertSqlSafe(format!("update {table} set record_id = $1 where tenant_id = $2 and entity = 'Ticket' and record_id = $3")))
                .bind(keep)
                .bind(tenant)
                .bind(dup)
                .execute(&mut **hc.tx)
                .await?;
        }
        // Who merged, and when: a note on each ticket.
        let note = |text: String, parent: Uuid, from: Option<&str>| {
            sqlx::query(
                "insert into e_ticket__conversation (tenant_id, id, parent_id, idx, author_kind, author, body, is_internal, sent_at, attachments, emailed, merged_from) \
                 values ($1, $2, $3, 1000000, 'Agent', $4, $5, true, $6, '[]', false, $7)",
            )
            .bind(tenant)
            .bind(Uuid::now_v7())
            .bind(parent)
            .bind(who.clone())
            .bind(text)
            .bind(now)
            .bind(from.map(str::to_string))
        };
        note(format!("Merged into {mine} by {who}."), dup, None).execute(&mut **hc.tx).await?;
        note(format!("{was} ({}) was merged into this ticket by {who}.", s(&d, "subject").unwrap_or_default()), keep, Some(&was)).execute(&mut **hc.tx).await?;
        let values = json!({"status": "Closed", "merged_into": keep, "merged_at": now.to_rfc3339(), "merged_by": who, "resolution_details": format!("Merged into {mine}")});
        hc.kernel.update_tx(&system, hc.tx, "Ticket", dup, obj(values), None).await?;
    }
    // The conversation in time order, and what the inbox shows of the ticket worked out again from it.
    sqlx::query(
        "update e_ticket__conversation c set idx = r.n from (select id, row_number() over (order by sent_at, idx, id) - 1 as n from e_ticket__conversation \
         where tenant_id = $1 and parent_id = $2) r where c.tenant_id = $1 and c.id = r.id",
    )
    .bind(tenant)
    .bind(keep)
    .execute(&mut **hc.tx)
    .await?;
    let now_doc = hc.kernel.get_tx(&system, hc.tx, "Ticket", keep, true).await?;
    let conversation: Vec<Value> = rows(&now_doc, "conversation").into_iter().map(Value::Object).collect();
    hc.kernel.update_tx(&system, hc.tx, "Ticket", keep, obj(json!({"conversation": conversation})), None).await?;
    Ok(json!({"merged": numbers, "messages_moved": moved, "message": format!("Merged {} into {mine}", numbers.join(", "))}))
}
