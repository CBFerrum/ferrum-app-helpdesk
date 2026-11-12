//! Customers writing to support: what becomes of a mail addressed to a support address.
//!
//! The Mail app receives the message and stores it as a `Communication` (`mailhook.rs` hands it here);
//! this module decides what it means. Nothing here reads Mail's tables: a mail comes in as [`Inbound`],
//! a plain description of the message, whatever brought it.
//!
//! * A mail for no support address is not ours and is not logged.
//! * Auto-replies, bounces, mailing-list mail, mail from a support address itself and mail over a sender's
//!   hourly limit are recognised by their headers and addresses, never turned into tickets or answered, and
//!   kept in the Inbound Mail Log for an administrator.
//! * A reply is matched to its ticket by the Message-ID it answers (In-Reply-To, References) or by the
//!   ticket's number and secret in the subject tag the app puts in outgoing mail. Never by subject text alone.
//! * Anything else is a new ticket of the address's team. The sender is matched to a customer by exact address
//!   (the customer's, or a contact's); an unknown sender is handled as Support Settings say.
//! * Mail on a closed ticket (final) opens a new ticket that says which it follows up.
//!
//! Everything runs in the caller's transaction as the system, under a lock on the sender, so the same mail
//! arriving twice at once is taken once and the hourly limit counts exactly.

use crate::site::customer_of;
use async_trait::async_trait;
use ferrum_domain::util::{b, id, rows, s, sys};
use ferrum_kernel::{Ctx, Doc, DocHook, Error, HookCtx, Kernel, Result, Tx};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use uuid::Uuid;

/// The longest text kept from a mail.
const MAX_TEXT: usize = 20_000;

/// A received mail, as this module reads it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Inbound {
    /// Without angle brackets.
    pub message_id: Option<String>,
    pub in_reply_to: Vec<String>,
    pub references: Vec<String>,
    pub from_name: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub text: String,
    /// Stored files: `[{id, file_name, content_type, size}]`.
    pub attachments: Vec<Value>,
    /// Header names (any case) to values: Auto-Submitted, Precedence, List-Id, List-Unsubscribe, X-Autoreply,
    /// X-Failed-Recipients, X-Ferrum-Helpdesk. Only what the mail had.
    pub headers: BTreeMap<String, String>,
    /// The Mail message this arrived as, to link it to the ticket.
    pub communication: Option<Uuid>,
    /// A ticket Mail already threaded it onto.
    pub reference: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Not addressed to a support address.
    NotForSupport,
    Created(Uuid),
    Added(Uuid),
    /// Waiting in the log for staff (the log row).
    Held(Uuid),
    /// Not a ticket or a message: why.
    Ignored(&'static str),
    /// Already taken.
    Duplicate,
}

/// The header Helpdesk puts on its own mail; mail carrying it came from this app and is never taken in.
pub const MARKER: &str = "x-ferrum-helpdesk";

pub fn norm(address: &str) -> String {
    address.trim().trim_matches(['<', '>']).trim().to_lowercase()
}

/// `support+tag@x.test` is `support@x.test`.
fn without_tag(address: &str) -> String {
    match address.split_once('@') {
        Some((local, domain)) => format!("{}@{domain}", local.split('+').next().unwrap_or(local)),
        None => address.to_string(),
    }
}

/// The addresses in a header's text (`"Rao, Priya" <p@x.test>, q@x.test`), lowercased.
pub fn addresses(list: Option<&str>) -> Vec<String> {
    let (mut out, mut cur, mut quoted) = (Vec::new(), String::new(), false);
    let push = |item: &str, out: &mut Vec<String>| {
        let inner = match (item.rfind('<'), item.rfind('>')) {
            (Some(a), Some(z)) if a < z => &item[a + 1..z],
            _ => item,
        };
        let a = norm(inner);
        if a.contains('@') && !a.contains(char::is_whitespace) && !out.contains(&a) {
            out.push(a);
        }
    };
    for c in list.unwrap_or_default().chars() {
        match c {
            '"' => {
                quoted = !quoted;
                cur.push(c);
            }
            ',' if !quoted => {
                push(&cur, &mut out);
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    push(&cur, &mut out);
    out
}

/// The tag outgoing mail carries in its subject, `[TKT-2026-000123/k3j9x2m1q7aa]`: the ticket's number and its secret.
pub fn tag(subject: &str) -> Option<(String, String)> {
    let mut rest = subject;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let close = after.find(']')?;
        if let Some((number, token)) = after[..close].rsplit_once('/')
            && !number.trim().is_empty()
            && number.len() <= 64
            && (8..=64).contains(&token.len())
            && token.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return Some((number.trim().to_string(), token.to_string()));
        }
        rest = &after[close + 1..];
    }
    None
}

/// A new secret for a ticket's subject tag.
pub fn new_token() -> String {
    Uuid::new_v4().simple().to_string()[..16].to_string()
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Why a mail is not for a person to answer, from its headers and sender: (reason, the header or address).
pub fn noise(mail: &Inbound, own: &HashSet<String>) -> Option<(&'static str, String)> {
    let h: BTreeMap<String, String> = mail.headers.iter().map(|(k, v)| (k.to_lowercase(), v.trim().to_lowercase())).collect();
    let from = norm(&mail.from);
    let local = from.split('@').next().unwrap_or_default();
    if own.contains(&without_tag(&from)) || h.contains_key(MARKER) {
        return Some(("Own address", if h.contains_key(MARKER) { MARKER.to_string() } else { from }));
    }
    if matches!(local, "mailer-daemon" | "postmaster") || h.contains_key("x-failed-recipients") {
        return Some(("Bounce", if h.contains_key("x-failed-recipients") { "x-failed-recipients".into() } else { from }));
    }
    if h.contains_key("list-id") || h.contains_key("list-unsubscribe") || h.get("precedence").is_some_and(|p| p == "list") {
        return Some(("Mailing list", h.keys().find(|k| k.starts_with("list-")).cloned().unwrap_or_else(|| "precedence: list".into())));
    }
    if let Some(v) = h.get("auto-submitted").filter(|v| v.as_str() != "no") {
        return Some(("Automatic reply", format!("auto-submitted: {v}")));
    }
    if h.contains_key("x-autoreply") || h.contains_key("x-autorespond") {
        return Some(("Automatic reply", "x-autoreply".into()));
    }
    if let Some(v) = h.get("precedence").filter(|v| matches!(v.as_str(), "bulk" | "junk" | "auto_reply")) {
        return Some(("Automatic reply", format!("precedence: {v}")));
    }
    if ["noreply", "no-reply", "donotreply", "do-not-reply"].contains(&local) {
        return Some(("Automatic reply", from));
    }
    None
}

fn cut(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

fn one_line(text: &str, max: usize) -> String {
    cut(&text.chars().filter(|c| !c.is_control()).collect::<String>(), max).trim().to_string()
}

struct Policy {
    unknown: String,
    per_hour: i64,
}

async fn policy(k: &Kernel, sctx: &Ctx, tx: &mut Tx) -> Result<Policy> {
    let settings = k.find_tx(sctx, tx, "SupportSettings", &[], None, 1).await?;
    let st = settings.first();
    Ok(Policy {
        unknown: st.and_then(|d| s(d, "unknown_senders")).unwrap_or("Open Ticket").to_string(),
        per_hour: st.and_then(|d| d.get("inbound_per_hour")).and_then(Value::as_i64).unwrap_or(20),
    })
}

/// The ticket a mail answers: by the number and secret in its subject, else by the Message-ID it answers,
/// else by a ticket Mail already put it on. A merged ticket leads to the one that stayed.
async fn matching_ticket(tx: &mut Tx, tenant: Uuid, mail: &Inbound) -> Result<Option<Uuid>> {
    let mut found = None;
    if let Some((number, token)) = tag(&mail.subject) {
        let row: Option<(Uuid, Option<String>)> = sqlx::query_as("select id, mail_token from e_ticket where tenant_id = $1 and number = $2")
            .bind(tenant)
            .bind(number)
            .fetch_optional(&mut **tx)
            .await?;
        found = row.filter(|(_, stored)| stored.as_deref().is_some_and(|t| same(t, &token))).map(|(i, _)| i);
    }
    if found.is_none() {
        let wanted: Vec<String> = mail.in_reply_to.iter().chain(mail.references.iter()).map(|m| m.trim().trim_matches(['<', '>']).to_string()).collect();
        if !wanted.is_empty() {
            found =
                sqlx::query_scalar("select parent_id from e_ticket__conversation where tenant_id = $1 and message_id = any($2) order by sent_at desc limit 1")
                    .bind(tenant)
                    .bind(&wanted)
                    .fetch_optional(&mut **tx)
                    .await?;
        }
    }
    found = found.or(mail.reference);
    for _ in 0..10 {
        let Some(t) = found else { break };
        match sqlx::query_scalar::<_, Option<Uuid>>("select merged_into from e_ticket where tenant_id = $1 and id = $2")
            .bind(tenant)
            .bind(t)
            .fetch_optional(&mut **tx)
            .await?
        {
            Some(Some(stays)) => found = Some(stays),
            Some(None) => break,
            None => return Ok(None),
        }
    }
    Ok(found)
}

#[allow(clippy::too_many_arguments)]
async fn log(
    k: &Kernel,
    sctx: &Ctx,
    tx: &mut Tx,
    mail: &Inbound,
    to: &str,
    outcome: &str,
    reason: Option<&str>,
    detail: Option<&str>,
    ticket: Option<Uuid>,
    held: Option<Uuid>,
) -> Result<Uuid> {
    let mut row = Doc::new();
    for (f, v) in [
        ("from_address", json!(cut(&norm(&mail.from), 320))),
        ("to_address", json!(cut(to, 320))),
        ("subject", json!(one_line(&mail.subject, 300))),
        ("message_id", json!(mail.message_id.as_deref().map(|m| cut(m, 500)))),
        ("outcome", json!(outcome)),
        ("reason", json!(reason)),
        ("detail", json!(detail.map(|d| one_line(d, 500)))),
        ("ticket", json!(ticket.map(|t| t.to_string()))),
        ("payload", if outcome == "Held" { serde_json::to_value(mail).unwrap_or(Value::Null) } else { Value::Null }),
    ] {
        row.insert(f.into(), v);
    }
    if let Some(log) = held {
        k.set_values_tx(sctx, tx, "InboundLog", log, row).await?;
        return Ok(log);
    }
    id(&k.insert_tx(sctx, tx, "InboundLog", row).await?, "id").ok_or_else(|| Error::internal("a log row without an id"))
}

/// A failure of the whole pipeline, kept for the administrator (after the work it undid was rolled back).
pub async fn log_failure(k: &Kernel, ctx: &Ctx, tx: &mut Tx, mail: &Inbound, error: &str) -> Result<()> {
    log(k, &sys(ctx), tx, mail, "", "Failed", Some("Error"), Some(error), None, None).await.map(|_| ())
}

/// What a mail means. See the module's notes. `held` is the log row of a held mail staff released.
pub async fn receive(k: &Kernel, ctx: &Ctx, tx: &mut Tx, mail: &Inbound) -> Result<Outcome> {
    run(k, ctx, tx, mail, None).await
}

async fn run(k: &Kernel, ctx: &Ctx, tx: &mut Tx, mail: &Inbound, held: Option<Uuid>) -> Result<Outcome> {
    let sctx = sys(ctx);
    let tenant = ctx.tenant;
    let addresses_all = k.find_tx(&sctx, tx, "SupportAddress", &[], Some("address asc"), 1000).await?;
    let own: HashSet<String> = addresses_all.iter().filter_map(|a| s(a, "address")).map(|a| without_tag(&norm(a))).collect();
    let recipients: HashSet<String> = mail.to.iter().chain(mail.cc.iter()).map(|a| without_tag(&norm(a))).collect();
    let Some(target) = addresses_all.iter().find(|a| b(a, "enabled") && s(a, "address").is_some_and(|x| recipients.contains(&without_tag(&norm(x))))) else {
        return Ok(Outcome::NotForSupport);
    };
    let to = s(target, "address").unwrap_or_default().to_string();
    let from = norm(&mail.from);
    // One mail from one sender at a time: duplicates and the hourly count are exact.
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))").bind(format!("helpdesk-inbound|{tenant}|{from}")).execute(&mut **tx).await?;
    let pol = policy(k, &sctx, tx).await?;
    if held.is_none() {
        if let Some(mid) = mail.message_id.as_deref() {
            let seen: bool = sqlx::query_scalar(
                "select exists (select 1 from e_inbound_log where tenant_id = $1 and message_id = $2 and outcome in ('Ticket Created', 'Added to Ticket', 'Held'))",
            )
            .bind(tenant)
            .bind(cut(mid, 500))
            .fetch_one(&mut **tx)
            .await?;
            if seen {
                return Ok(Outcome::Duplicate);
            }
        }
        if let Some((reason, detail)) = noise(mail, &own) {
            log(k, &sctx, tx, mail, &to, "Ignored", Some(reason), Some(&detail), None, None).await?;
            return Ok(Outcome::Ignored(reason));
        }
        if pol.per_hour > 0 {
            let taken: i64 = sqlx::query_scalar(
                "select count(*) from e_inbound_log where tenant_id = $1 and from_address = $2 and received_at > now() - interval '1 hour' \
                 and outcome in ('Ticket Created', 'Added to Ticket', 'Held')",
            )
            .bind(tenant)
            .bind(&from)
            .fetch_one(&mut **tx)
            .await?;
            if taken >= pol.per_hour {
                log(k, &sctx, tx, mail, &to, "Ignored", Some("Rate limit"), Some(&format!("{taken} in the last hour")), None, None).await?;
                return Ok(Outcome::Ignored("Rate limit"));
            }
        }
    }
    let name = mail.from_name.clone().filter(|n| !n.trim().is_empty()).unwrap_or_else(|| from.split('@').next().unwrap_or_default().to_string());
    let matched = matching_ticket(tx, tenant, mail).await?;
    let mut existing = None;
    if let Some(t) = matched {
        let status: Option<String> =
            sqlx::query_scalar("select status from e_ticket where tenant_id = $1 and id = $2").bind(tenant).bind(t).fetch_optional(&mut **tx).await?;
        existing = status.map(|st| (t, st == "Closed"));
    }
    let customer = match existing {
        Some((_, false)) => None,
        _ => customer_of(k, &sctx, tx, &from).await?,
    };
    if existing.is_none() && customer.is_none() && held.is_none() {
        match pol.unknown.as_str() {
            "Ignore" => {
                log(k, &sctx, tx, mail, &to, "Ignored", Some("Unknown sender"), None, None, None).await?;
                return Ok(Outcome::Ignored("Unknown sender"));
            }
            "Hold for Review" => {
                let row = log(k, &sctx, tx, mail, &to, "Held", Some("Unknown sender"), None, None, None).await?;
                return Ok(Outcome::Held(row));
            }
            _ => {}
        }
    }
    let copied: Vec<String> = mail.to.iter().chain(mail.cc.iter()).map(|a| norm(a)).filter(|a| a != &from && !own.contains(&without_tag(a))).collect();
    let (ticket, created) = match existing {
        Some((t, false)) => {
            append(k, &sctx, tx, t, mail, &from, &name, &copied).await?;
            (t, false)
        }
        _ => {
            let related = existing.map(|(t, _)| t);
            (create(k, &sctx, tx, mail, target, &from, &name, customer, &copied, related).await?, true)
        }
    };
    let outcome = if created { "Ticket Created" } else { "Added to Ticket" };
    log(k, &sctx, tx, mail, &to, outcome, None, None, Some(ticket), held).await?;
    Ok(if created { Outcome::Created(ticket) } else { Outcome::Added(ticket) })
}

fn files(mail: &Inbound) -> Vec<Value> {
    mail.attachments.iter().filter_map(|a| a.get("id").cloned()).map(|i| json!({"id": i})).collect()
}

fn message(mail: &Inbound, from: &str, body: &str, copied: &[String]) -> Value {
    json!({
        "author_kind": "Customer", "author": from, "body": body, "is_internal": false,
        "attachments": files(mail), "message_id": mail.message_id.as_deref().map(|m| cut(m, 500)),
        "communication": mail.communication.map(|c| c.to_string()), "cc": if copied.is_empty() { Value::Null } else { json!(copied.join(", ")) },
    })
}

fn body_of(mail: &Inbound) -> String {
    let text = cut(mail.text.trim(), MAX_TEXT);
    if text.is_empty() { one_line(&mail.subject, 300) } else { text }
}

/// Copied addresses a ticket does not have yet, besides its contact.
fn new_copies(ticket: &Doc, copied: &[String], from: &str) -> Option<String> {
    let contact = s(ticket, "contact_email").map(norm).unwrap_or_default();
    let mut have: Vec<String> = addresses(s(ticket, "cc"));
    let before = have.len();
    for a in copied.iter().map(String::as_str).chain([from]) {
        if a != contact && !a.is_empty() && !have.iter().any(|h| h == a) {
            have.push(a.to_string());
        }
    }
    (have.len() > before).then(|| have.join(", "))
}

/// A customer's message on an open ticket, with its copies and files, as the system.
#[allow(clippy::too_many_arguments)]
async fn append(k: &Kernel, sctx: &Ctx, tx: &mut Tx, ticket: Uuid, mail: &Inbound, from: &str, _name: &str, copied: &[String]) -> Result<()> {
    let t = k.get_tx(sctx, tx, "Ticket", ticket, true).await?;
    let mut conversation: Vec<Value> = rows(&t, "conversation").into_iter().map(Value::Object).collect();
    conversation.push(message(mail, from, &body_of(mail), copied));
    let mut patch = Doc::new();
    patch.insert("conversation".into(), Value::Array(conversation));
    if let Some(cc) = new_copies(&t, copied, from) {
        patch.insert("cc".into(), json!(cc));
    }
    k.update_tx(sctx, tx, "Ticket", ticket, patch, None).await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn create(
    k: &Kernel,
    sctx: &Ctx,
    tx: &mut Tx,
    mail: &Inbound,
    target: &Doc,
    from: &str,
    name: &str,
    customer: Option<Uuid>,
    copied: &[String],
    related: Option<Uuid>,
) -> Result<Uuid> {
    let body = body_of(mail);
    let mut t = Doc::new();
    for (f, v) in [
        ("subject", json!(if one_line(&mail.subject, 300).is_empty() { from.to_string() } else { one_line(&mail.subject, 300) })),
        ("description", json!(body)),
        ("priority", json!(s(target, "default_priority").unwrap_or("Medium"))),
        ("ticket_type", json!(target.get("ticket_type").cloned().unwrap_or(Value::Null))),
        ("team", json!(target.get("team").cloned().unwrap_or(Value::Null))),
        ("channel", json!("Email")),
        ("channel_address", json!(id(target, "id").map(|i| i.to_string()))),
        ("contact_email", json!(from)),
        ("contact_name", json!(name)),
        ("customer", json!(customer.map(|c| c.to_string()))),
        ("related_ticket", json!(related.map(|c| c.to_string()))),
        ("cc", if copied.is_empty() { Value::Null } else { json!(copied.join(", ")) }),
    ] {
        t.insert(f.into(), v);
    }
    let saved = k.insert_tx(sctx, tx, "Ticket", t).await?;
    let ticket = id(&saved, "id").ok_or_else(|| Error::internal("a ticket without an id"))?;
    // The mail's own words are the description; a message carries what the mail brought (files, its id).
    if !mail.attachments.is_empty() || mail.message_id.is_some() || mail.communication.is_some() {
        let mut patch = Doc::new();
        patch.insert("conversation".into(), json!([message(mail, from, &body, copied)]));
        k.update_tx(sctx, tx, "Ticket", ticket, patch, None).await?;
    }
    Ok(ticket)
}

// ───────────────────────────── the log's actions ─────────────────────────────

pub struct LogHook;

#[async_trait]
impl DocHook for LogHook {
    async fn action(&self, hc: &mut HookCtx<'_>, name: &str, doc: &Doc, _params: &Value) -> Result<Option<Value>> {
        if !matches!(name, "release" | "discard") {
            return Ok(None);
        }
        let log = id(doc, "id").ok_or_else(|| Error::internal("a log row without an id"))?;
        if s(doc, "outcome") != Some("Held") {
            return Err(Error::invalid("only mail held for review can be released or discarded"));
        }
        let sctx = sys(hc.ctx);
        let mail: Inbound = serde_json::from_value(doc.get("payload").cloned().unwrap_or(Value::Null))
            .map_err(|e| Error::invalid(format!("the held mail cannot be read: {e}")))?;
        if name == "discard" {
            let mut v = Doc::new();
            for (f, x) in [("outcome", json!("Ignored")), ("reason", json!("Discarded by staff")), ("payload", Value::Null)] {
                v.insert(f.into(), x);
            }
            hc.kernel.set_values_tx(&sctx, hc.tx, "InboundLog", log, v).await?;
            for f in mail.attachments.iter().filter_map(|a| a.get("id")?.as_str()?.parse::<Uuid>().ok()) {
                if let Err(e) = hc.kernel.remove_file(&sctx, f).await {
                    tracing::warn!(file = %f, "a discarded mail's file was not removed: {e}");
                }
            }
            return Ok(Some(json!({"outcome": "Ignored"})));
        }
        let outcome = run(hc.kernel, hc.ctx, hc.tx, &mail, Some(log)).await?;
        let ticket = match outcome {
            Outcome::Created(t) | Outcome::Added(t) => t,
            other => return Err(Error::invalid(format!("the mail is not for a support address any more ({other:?})"))),
        };
        if let Some(c) = mail.communication {
            crate::mailhook::link(hc.kernel, &sctx, hc.tx, c, ticket).await?;
        }
        Ok(Some(json!({"ticket": ticket, "outcome": "Ticket Created"})))
    }
}

/// An address is stored as written in lower case.
pub struct AddressHook;

#[async_trait]
impl DocHook for AddressHook {
    async fn before_validate(&self, _hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
        if let Some(a) = s(doc, "address").map(norm) {
            doc.insert("address".into(), json!(a));
        }
        Ok(())
    }
}

/// An `Inbound` from a Mail `Communication` as Mail stores received mail (docs of the comms package): `None` unless it is
/// received email. Header names Mail keeps in `headers` are read when it keeps them.
pub fn from_communication(doc: &Doc) -> Option<Inbound> {
    if s(doc, "channel") != Some("Email") || s(doc, "direction") != Some("Received") {
        return None;
    }
    let headers = doc
        .get("headers")
        .and_then(Value::as_object)
        .map(|h| h.iter().filter_map(|(k, v)| Some((k.to_lowercase(), v.as_str()?.to_string()))).collect())
        .unwrap_or_default();
    let list = |f: &str| {
        s(doc, f).map(|v| v.split_whitespace().map(|m| m.trim_matches(['<', '>']).to_string()).filter(|m| !m.is_empty()).collect()).unwrap_or_default()
    };
    Some(Inbound {
        message_id: s(doc, "message_id").map(str::to_string),
        in_reply_to: s(doc, "in_reply_to").map(|m| vec![m.to_string()]).unwrap_or_default(),
        references: list("mail_references"),
        from_name: s(doc, "sender_name").map(str::to_string),
        from: s(doc, "sender")?.to_string(),
        to: addresses(s(doc, "recipients")),
        cc: addresses(s(doc, "cc")),
        subject: s(doc, "subject").unwrap_or_default().to_string(),
        text: s(doc, "text_content").unwrap_or_default().to_string(),
        attachments: doc.get("attachments").and_then(Value::as_array).cloned().unwrap_or_default(),
        headers,
        communication: id(doc, "id"),
        reference: match (s(doc, "reference_entity"), id(doc, "reference_id")) {
            (Some("Ticket"), Some(t)) => Some(t),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mail(headers: &[(&str, &str)], from: &str) -> Inbound {
        Inbound { from: from.into(), headers: headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), ..Default::default() }
    }

    #[test]
    fn addresses_are_found_in_a_headers_text() {
        assert_eq!(addresses(Some("\"Rao, Priya\" <P@X.test>, q@x.test, <p@x.test>, nonsense")), ["p@x.test", "q@x.test"]);
        assert!(addresses(None).is_empty());
        assert_eq!(without_tag("support+billing@x.test"), "support@x.test");
    }

    #[test]
    fn the_subject_tag_is_the_number_and_the_secret_and_nothing_else_matches() {
        assert_eq!(tag("Re: [TKT-2026-000123/ab12cd34ef56gh78] Printer jam"), Some(("TKT-2026-000123".into(), "ab12cd34ef56gh78".into())));
        assert_eq!(tag("[ignore me] Re: [T-1/aaaaaaaa1111] x"), Some(("T-1".into(), "aaaaaaaa1111".into())));
        assert_eq!(tag("Re: [TKT-2026-000123] Printer jam"), None, "the number alone is not enough");
        assert_eq!(tag("Re: TKT-2026-000123/ab12cd34ef56gh78 Printer jam"), None);
        assert_eq!(tag("[T-1/short]"), None);
        assert_eq!(tag("[unclosed T-1/abcdefgh12"), None);
        assert!(same("abc", "abc") && !same("abc", "abd") && !same("abc", "ab"));
    }

    #[test]
    fn loops_and_noise_are_recognised_by_their_headers() {
        let own: HashSet<String> = ["support@acme.test".to_string()].into();
        let why = |m: &Inbound| noise(m, &own).map(|r| r.0);
        assert_eq!(why(&mail(&[], "ada@cust.test")), None);
        assert_eq!(why(&mail(&[], "Support+x@Acme.test")), Some("Own address"), "the support address itself, with a tag");
        assert_eq!(why(&mail(&[("X-Ferrum-Helpdesk", "TKT-1")], "ada@cust.test")), Some("Own address"));
        assert_eq!(why(&mail(&[], "MAILER-DAEMON@mx.test")), Some("Bounce"));
        assert_eq!(why(&mail(&[("X-Failed-Recipients", "x@y.test")], "ada@cust.test")), Some("Bounce"));
        assert_eq!(why(&mail(&[("Auto-Submitted", "auto-replied")], "ada@cust.test")), Some("Automatic reply"));
        assert_eq!(why(&mail(&[("Auto-Submitted", "no")], "ada@cust.test")), None, "a person's mail says no");
        assert_eq!(why(&mail(&[("X-Autoreply", "yes")], "ada@cust.test")), Some("Automatic reply"));
        assert_eq!(why(&mail(&[("Precedence", "bulk")], "ada@cust.test")), Some("Automatic reply"));
        assert_eq!(why(&mail(&[("List-Id", "<news.x.test>")], "ada@cust.test")), Some("Mailing list"));
        assert_eq!(why(&mail(&[("List-Unsubscribe", "<mailto:u@x.test>")], "ada@cust.test")), Some("Mailing list"));
        assert_eq!(why(&mail(&[], "no-reply@shop.test")), Some("Automatic reply"));
    }

    #[test]
    fn a_communication_becomes_a_mail() {
        let doc = json!({"id": Uuid::nil().to_string(), "channel": "Email", "direction": "Received", "sender": "ada@cust.test", "sender_name": "Ada",
            "recipients": "\"Support\" <support@acme.test>", "cc": "bob@cust.test", "subject": "Hi", "text_content": "Hello", "message_id": "m1@cust.test",
            "in_reply_to": "p0@acme.test", "mail_references": "<r1@x> <p0@acme.test>", "headers": {"Auto-Submitted": "no"},
            "reference_entity": "Ticket", "reference_id": Uuid::nil().to_string()});
        let m = from_communication(doc.as_object().unwrap()).unwrap();
        assert_eq!(
            (m.to, m.cc, m.references, m.reference.is_some()),
            (vec!["support@acme.test".into()], vec!["bob@cust.test".into()], vec!["r1@x".into(), "p0@acme.test".into()], true)
        );
        assert_eq!(m.headers.get("auto-submitted").map(String::as_str), Some("no"));
        assert!(from_communication(json!({"channel": "Email", "direction": "Sent"}).as_object().unwrap()).is_none());
        assert!(from_communication(json!({"channel": "SMS", "direction": "Received", "sender": "x"}).as_object().unwrap()).is_none());
    }

    #[test]
    fn copies_are_added_once_and_never_the_contact() {
        let t = json!({"contact_email": "ada@cust.test", "cc": "bob@cust.test"});
        let t = t.as_object().unwrap();
        assert_eq!(
            new_copies(t, &["ada@cust.test".into(), "bob@cust.test".into(), "cy@cust.test".into()], "dee@cust.test").as_deref(),
            Some("bob@cust.test, cy@cust.test, dee@cust.test")
        );
        assert_eq!(new_copies(t, &["bob@cust.test".into()], "ada@cust.test"), None);
    }
}
