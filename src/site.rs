//! Helpdesk's endpoints for signed-in people (`/api/app/helpdesk/…`): the portal's "Your tickets"
//! (Ferrum docs/plan/apps-ui.md §9.1) for whoever raised them, and the open tickets assigned to an
//! agent for Home's My work. Portal people hold no roles: their tickets are the ones raised from
//! the address they signed in with, and what they change runs as the system after these checks.

use crate::ticket::OPEN;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ferrum_domain::util::{b, id, rows, s, set, sys};
use ferrum_kernel::{Ctx, Doc, Error, Filter, Kernel, ListQuery, Op, Result, Site, SiteRequest, SiteResponse, Tx, with_retry};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use uuid::Uuid;

const MAX_BODY: usize = 64 * 1024;
/// The longest message or description a customer may send.
const MAX_TEXT: usize = 20_000;

pub struct HelpdeskSite;

#[async_trait]
impl Site for HelpdeskSite {
    async fn handle(&self, k: &Kernel, req: SiteRequest) -> Result<SiteResponse> {
        let user = req.user.as_ref().ok_or_else(|| Error::Unauthorized("sign in first".into()))?;
        let segs: Vec<&str> = req.path.split('/').filter(|p| !p.is_empty()).collect();
        let (status, out) = match (req.method.as_str(), segs.as_slice()) {
            ("GET", ["work"]) => (200, work(k, user).await?),
            ("GET", ["portal", "tickets"]) => (200, mine(k, user, query(&req, "cursor")).await?),
            ("POST", ["portal", "tickets"]) => (201, raise(k, user, &body(&req)?).await?),
            ("GET", ["portal", "tickets", t]) => (200, one(k, user, ticket_id(t)?).await?),
            ("POST", ["portal", "tickets", t, "replies"]) => (200, reply(k, user, ticket_id(t)?, &body(&req)?).await?),
            ("POST", ["portal", "tickets", t, "rating"]) => (200, rate(k, user, ticket_id(t)?, &body(&req)?).await?),
            _ => return Err(Error::NotFound(format!("helpdesk endpoint {} {}", req.method, req.path))),
        };
        let mut r = SiteResponse::json(status, &out);
        r.headers.push(("cache-control".into(), "no-store".into()));
        Ok(r)
    }

    fn max_body(&self) -> usize {
        MAX_BODY
    }
}

fn query<'a>(req: &'a SiteRequest, name: &str) -> Option<&'a str> {
    req.query.get(name).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

fn body(req: &SiteRequest) -> Result<Map<String, Value>> {
    if req.body.iter().all(u8::is_ascii_whitespace) {
        return Ok(Map::new());
    }
    match serde_json::from_slice::<Value>(&req.body) {
        Ok(Value::Object(m)) => Ok(m),
        _ => Err(Error::invalid("the request body must be a JSON object")),
    }
}

fn ticket_id(raw: &str) -> Result<Uuid> {
    raw.parse().map_err(|_| Error::NotFound("ticket".into()))
}

/// A required piece of text, trimmed, at most `max` characters.
fn text(body: &Map<String, Value>, key: &str, max: usize) -> Result<String> {
    let v = body.get(key).and_then(Value::as_str).map(str::trim).unwrap_or_default();
    if v.is_empty() {
        return Err(Error::field(key, "is required"));
    }
    if v.chars().count() > max {
        return Err(Error::field(key, format!("at most {max} characters")));
    }
    Ok(v.to_string())
}

/// What a customer reads for a status.
fn label(status: &str) -> &'static str {
    match status {
        "Replied" => "Waiting for your reply",
        "On Hold" => "On hold",
        "Resolved" => "Resolved",
        "Closed" => "Closed",
        _ => "With the team",
    }
}

fn summary(t: &Doc) -> Value {
    let status = s(t, "status").unwrap_or("Open");
    json!({
        "id": s(t, "id"),
        "number": s(t, "number"),
        "subject": s(t, "subject"),
        "status": status,
        "label": label(status),
        "priority": s(t, "priority"),
        "opened_at": s(t, "opened_at"),
        "updated_at": s(t, "updated_at"),
    })
}

/// The caller's ticket, as the system sees it; another person's reads as missing.
async fn own(k: &Kernel, sctx: &Ctx, tx: &mut Tx, user: &Ctx, ticket: Uuid, lock: bool) -> Result<Doc> {
    let t = match k.get_tx(sctx, tx, "Ticket", ticket, lock).await {
        Ok(t) => t,
        Err(Error::NotFound(_)) => return Err(Error::NotFound("ticket".into())),
        Err(e) => return Err(e),
    };
    if s(&t, "contact_email") != Some(user.email.to_lowercase().as_str()) || user.email.is_empty() {
        return Err(Error::NotFound("ticket".into()));
    }
    Ok(t)
}

/// `GET portal/tickets`: the caller's tickets, newest first, twenty at a time.
async fn mine(k: &Kernel, user: &Ctx, cursor: Option<&str>) -> Result<Value> {
    if user.email.is_empty() {
        return Ok(json!({"tickets": [], "next_cursor": null}));
    }
    let q = ListQuery {
        filters: vec![Filter::eq("contact_email", user.email.to_lowercase())],
        sort: Some("opened_at desc".into()),
        limit: Some(20),
        cursor: cursor.map(str::to_string),
        ..Default::default()
    };
    let page = k.list(&sys(user), "Ticket", &q).await?;
    Ok(json!({"tickets": page.rows.iter().map(summary).collect::<Vec<_>>(), "next_cursor": page.next_cursor}))
}

/// `GET portal/tickets/{id}`: one ticket and its public conversation (agents' notes stay out).
async fn one(k: &Kernel, user: &Ctx, ticket: Uuid) -> Result<Value> {
    let sctx = sys(user);
    let mut tx = k.db.tenant_tx(user.tenant).await?;
    let t = own(k, &sctx, &mut tx, user, ticket, false).await?;
    let view = detail(&mut tx, user, &t).await?;
    tx.commit().await?;
    Ok(view)
}

async fn detail(tx: &mut Tx, user: &Ctx, t: &Doc) -> Result<Value> {
    let messages: Vec<Doc> = rows(t, "conversation").into_iter().filter(|m| !b(m, "is_internal")).collect();
    // Agents by name, not by address.
    let agents: Vec<String> = messages.iter().filter(|m| s(m, "author_kind") != Some("Customer")).filter_map(|m| s(m, "author").map(str::to_string)).collect();
    let names: HashMap<String, String> = if agents.is_empty() {
        HashMap::new()
    } else {
        sqlx::query_as::<_, (String, String)>("select email::text, full_name from app_user where tenant_id = $1 and email = any($2) and full_name <> ''")
            .bind(user.tenant)
            .bind(&agents)
            .fetch_all(&mut **tx)
            .await?
            .into_iter()
            .collect()
    };
    let status = s(t, "status").unwrap_or("Open");
    let closed = status == "Closed";
    let done = matches!(status, "Resolved" | "Closed");
    let mut view = summary(t);
    let extra = json!({
        "description": s(t, "description"),
        "messages": messages.iter().map(|m| {
            let mine = s(m, "author_kind") == Some("Customer");
            let who = if mine { "You".to_string() } else { s(m, "author").and_then(|a| names.get(a)).cloned().unwrap_or_else(|| "Support team".into()) };
            json!({"id": s(m, "id"), "mine": mine, "author": who, "body": s(m, "body"), "sent_at": s(m, "sent_at")})
        }).collect::<Vec<_>>(),
        "resolution": if done { s(t, "resolution_details") } else { None },
        "rating": t.get("satisfaction_rating").cloned().unwrap_or(Value::Null),
        "feedback": s(t, "feedback"),
        "can_reply": !closed,
        "can_rate": done && t.get("satisfaction_rating").is_none_or(Value::is_null),
    });
    if let (Value::Object(v), Value::Object(e)) = (&mut view, extra) {
        v.extend(e);
    }
    Ok(view)
}

/// `POST portal/tickets` `{subject, description, priority?}`: a new ticket from the portal.
async fn raise(k: &Kernel, user: &Ctx, body: &Map<String, Value>) -> Result<Value> {
    if user.email.is_empty() {
        return Err(Error::Forbidden("this account has no email address to answer".into()));
    }
    let subject = text(body, "subject", 300)?;
    let description = text(body, "description", MAX_TEXT)?;
    let priority = match body.get("priority").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) {
        None => "Medium",
        Some(p @ ("Low" | "Medium" | "High")) => p,
        Some(_) => return Err(Error::field("priority", "choose Low, Medium or High")),
    };
    k.rate_limit(
        &format!("helpdesk|raise|{}|{}", user.tenant, user.user),
        std::time::Duration::from_secs(3600),
        20,
        "too many new tickets in an hour; try again later",
    )
    .await?;
    let sctx = sys(user);
    let email = user.email.to_lowercase();
    with_retry(|| async {
        let mut tx = k.db.tenant_tx(user.tenant).await?;
        let mut t = Doc::new();
        set(&mut t, "subject", subject.clone());
        set(&mut t, "description", description.clone());
        set(&mut t, "priority", priority);
        set(&mut t, "channel", "Portal");
        set(&mut t, "contact_email", email.clone());
        set(&mut t, "contact_name", if user.full_name.is_empty() { email.clone() } else { user.full_name.clone() });
        if let Some(c) = customer_of(k, &sctx, &mut tx, &email).await? {
            set(&mut t, "customer", c.to_string());
        }
        let saved = k.insert_tx(&sctx, &mut tx, "Ticket", t).await?;
        let view = detail(&mut tx, user, &saved).await?;
        tx.commit().await?;
        Ok(view)
    })
    .await
}

/// The customer an address belongs to: the customer's own email, or one of its contacts'.
async fn customer_of(k: &Kernel, sctx: &Ctx, tx: &mut Tx, email: &str) -> Result<Option<Uuid>> {
    let direct = k.find_tx(sctx, tx, "Customer", &[Filter::eq("email", email)], Some("created_at asc"), 5).await?;
    if let Some(c) = direct.iter().find(|c| !b(c, "disabled")).and_then(|c| id(c, "id")) {
        return Ok(Some(c));
    }
    if sctx.schema.entity("Contact").is_some_and(|c| c.field("emails").is_some() && c.field("customer").is_some()) {
        let contacts = k.find_tx(sctx, tx, "Contact", &[Filter::eq("emails.email", email)], Some("created_at asc"), 5).await?;
        return Ok(contacts.iter().find_map(|c| id(c, "customer")));
    }
    Ok(None)
}

/// `POST portal/tickets/{id}/replies` `{body}`: the customer's message; it reopens a ticket that
/// waited on them or was resolved (the ticket's own rules).
async fn reply(k: &Kernel, user: &Ctx, ticket: Uuid, body: &Map<String, Value>) -> Result<Value> {
    let message = text(body, "body", MAX_TEXT)?;
    k.rate_limit(
        &format!("helpdesk|reply|{}|{}", user.tenant, user.user),
        std::time::Duration::from_secs(3600),
        60,
        "too many messages in an hour; try again later",
    )
    .await?;
    let sctx = sys(user);
    with_retry(|| async {
        let mut tx = k.db.tenant_tx(user.tenant).await?;
        let t = own(k, &sctx, &mut tx, user, ticket, true).await?;
        if s(&t, "status") == Some("Closed") {
            return Err(Error::invalid("this ticket is closed; raise a new one"));
        }
        let mut conversation: Vec<Value> = rows(&t, "conversation").into_iter().map(Value::Object).collect();
        conversation.push(json!({"author_kind": "Customer", "body": message, "is_internal": false}));
        let mut patch = Doc::new();
        patch.insert("conversation".into(), Value::Array(conversation));
        let saved = k.update_tx(&sctx, &mut tx, "Ticket", ticket, patch, None).await?;
        let view = detail(&mut tx, user, &saved).await?;
        tx.commit().await?;
        Ok(view)
    })
    .await
}

/// `POST portal/tickets/{id}/rating` `{rating, feedback?}`: how the customer found the help, once
/// the ticket is resolved.
async fn rate(k: &Kernel, user: &Ctx, ticket: Uuid, body: &Map<String, Value>) -> Result<Value> {
    let rating = body.get("rating").and_then(Value::as_i64).filter(|r| (1..=5).contains(r)).ok_or_else(|| Error::field("rating", "from 1 to 5"))?;
    let feedback = match body.get("feedback").and_then(Value::as_str).map(str::trim).filter(|f| !f.is_empty()) {
        Some(f) if f.chars().count() > 2000 => return Err(Error::field("feedback", "at most 2000 characters")),
        f => f.map(str::to_string),
    };
    let sctx = sys(user);
    with_retry(|| async {
        let mut tx = k.db.tenant_tx(user.tenant).await?;
        let t = own(k, &sctx, &mut tx, user, ticket, true).await?;
        if !matches!(s(&t, "status"), Some("Resolved" | "Closed")) {
            return Err(Error::invalid("a ticket is rated once it is resolved"));
        }
        let mut patch = Doc::new();
        patch.insert("satisfaction_rating".into(), json!(rating));
        patch.insert("feedback".into(), json!(feedback));
        let saved = k.update_tx(&sctx, &mut tx, "Ticket", ticket, patch, None).await?;
        let view = detail(&mut tx, user, &saved).await?;
        tx.commit().await?;
        Ok(view)
    })
    .await
}

/// `GET work`: the open tickets assigned to the caller, for Home's My work: due when their SLA
/// wants them resolved, High and Urgent ones marked.
async fn work(k: &Kernel, user: &Ctx) -> Result<Value> {
    if user.email.is_empty() {
        return Ok(json!({"items": []}));
    }
    let q = ListQuery {
        filters: vec![Filter::eq("assigned_to", user.email.to_lowercase()), Filter { field: "status".into(), op: Op::In, value: json!(OPEN) }],
        sort: Some("opened_at asc".into()),
        limit: Some(50),
        ..Default::default()
    };
    // As the person: an agent who may no longer read tickets sees none.
    let page = match k.list(user, "Ticket", &q).await {
        Ok(p) => p,
        Err(Error::Forbidden(_)) => return Ok(json!({"items": []})),
        Err(e) => return Err(e),
    };
    let offset = *user.now_local().offset();
    let items: Vec<Value> = page
        .rows
        .iter()
        .map(|t| {
            let who = s(t, "customer_name").or(s(t, "contact_name")).or(s(t, "contact_email"));
            let due = s(t, "resolution_by")
                .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
                .map(|d| d.with_timezone(&Utc).with_timezone(&offset).format("%Y-%m-%d").to_string());
            let status = s(t, "status").unwrap_or("Open");
            let subtitle = [s(t, "number"), who, (status != "Open").then_some(status)].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            json!({
                "title": s(t, "subject").unwrap_or("Ticket"),
                "subtitle": subtitle,
                "url": format!("/helpdesk/t/{}", s(t, "id").unwrap_or_default()),
                "due": due,
                "priority": matches!(s(t, "priority"), Some("High" | "Urgent")).then_some("High"),
                "updated_at": s(t, "updated_at"),
            })
        })
        .collect();
    Ok(json!({"items": items}))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn customers_read_plain_statuses() {
        assert_eq!(label("Replied"), "Waiting for your reply");
        assert_eq!(label("Open"), "With the team");
        assert_eq!(label("Resolved"), "Resolved");
        let mut b = Map::new();
        b.insert("subject".into(), json!("  Printer jam  "));
        assert_eq!(text(&b, "subject", 300).unwrap(), "Printer jam");
        assert!(text(&b, "body", 10).is_err());
        b.insert("long".into(), json!("x".repeat(11)));
        assert!(text(&b, "long", 10).is_err());
    }
}
