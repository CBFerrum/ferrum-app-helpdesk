//! Where Helpdesk meets the Mail app. Mail receives and stores every message; Helpdesk registers each support
//! address with it (`ferrum_app_comms::routes`) and Mail hands a message for one to [`Takes`], once, in the transaction
//! that stores it, behind a savepoint of its own. Nothing of Mail's is read: the message arrives as [`Received`], with
//! the headers Mail kept (`Communication.headers`), and goes through [`crate::inbound`].
//!
//! A registration follows its address: made when an address is added or enabled, moved when it is changed, taken
//! back when it is disabled or deleted, and, for every address a workspace already had, by the upkeep. The address
//! itself is registered, which also takes `support+<anything>@`: the tag is the ticket's secret in the Reply-To of
//! what Helpdesk sends, so an answer finds its ticket by the address it was written to.
//!
//! A failure in the pipeline is the mail's own, not Mail's: the work is undone to a savepoint and an entry in the
//! Inbound Mail Log says what went wrong, and Mail goes on storing the message.

use crate::inbound::{self, Inbound, Outcome};
use async_trait::async_trait;
use ferrum_app_comms::routes::{self, Handler, Received, Registration};
use ferrum_domain::util::{b, id, s, sys};
use ferrum_kernel::{Ctx, Doc, Kernel, Result, Tx};
use serde_json::json;
use std::collections::BTreeMap;
use uuid::Uuid;

/// What Mail calls it: the app id its handler and registrations go by.
pub const APP: &str = "helpdesk";

/// The handler Mail hands support mail to.
pub struct Takes;

/// A message Mail hands over, as [`inbound`] reads one.
pub fn from_received(m: &Received) -> Inbound {
    Inbound {
        message_id: m.message_id.clone(),
        in_reply_to: m.in_reply_to.clone(),
        references: m.references.clone(),
        from_name: m.from_name.clone(),
        from: m.from.clone(),
        // Mail hands over the address headers as they were written (`"Ada" <ada@x.test>`): bare, lower case addresses here.
        to: m.to.iter().flat_map(|a| inbound::addresses(Some(a))).collect(),
        cc: m.cc.iter().flat_map(|a| inbound::addresses(Some(a))).collect(),
        subject: m.subject.clone(),
        text: m.text.clone(),
        attachments: m.attachments.clone(),
        headers: m.headers.iter().map(|(k, v)| (k.to_lowercase(), v.clone())).collect::<BTreeMap<_, _>>(),
        communication: Some(m.communication),
        reference: m.reference.as_ref().filter(|(entity, _)| entity == "Ticket").map(|(_, t)| *t),
        recipient: Some(m.recipient.clone()),
        token: m.token.clone(),
    }
}

/// Puts a Mail message on a ticket.
pub(crate) async fn link(k: &Kernel, sctx: &Ctx, tx: &mut Tx, communication: Uuid, ticket: Uuid) -> Result<()> {
    let mut v = Doc::new();
    v.insert("reference_entity".into(), json!("Ticket"));
    v.insert("reference_id".into(), json!(ticket.to_string()));
    k.set_values_tx(sctx, tx, "Communication", communication, v).await?;
    Ok(())
}

#[async_trait]
impl Handler for Takes {
    async fn receive(&self, k: &Kernel, ctx: &Ctx, tx: &mut Tx, received: &Received) -> Result<()> {
        // Mail filed onto some other record is that record's.
        if received.reference.as_ref().is_some_and(|(entity, _)| entity != "Ticket") || ctx.schema.entity("SupportAddress").is_none() {
            return Ok(());
        }
        let mail = from_received(received);
        let sctx = sys(ctx);
        sqlx::query("savepoint helpdesk_inbound").execute(&mut **tx).await?;
        match inbound::receive(k, ctx, tx, &mail).await {
            Ok(Outcome::Created(t) | Outcome::Added(t)) => {
                sqlx::query("release savepoint helpdesk_inbound").execute(&mut **tx).await?;
                link(k, &sctx, tx, received.communication, t).await?;
            }
            Ok(_) => {
                sqlx::query("release savepoint helpdesk_inbound").execute(&mut **tx).await?;
            }
            Err(e) => {
                sqlx::query("rollback to savepoint helpdesk_inbound").execute(&mut **tx).await?;
                tracing::warn!("inbound mail {:?} was not taken in: {e}", mail.message_id);
                inbound::log_failure(k, ctx, tx, &mail, &e.to_string()).await?;
            }
        }
        Ok(())
    }
}

/// Registers the handler with Mail, once at startup.
pub fn add_handler() {
    routes::add_handler(APP, std::sync::Arc::new(Takes));
}

/// The address a customer's answer to a ticket is written to: the support address with the ticket's secret in it
/// (`support+<secret>@acme.test`; for a registered pattern, the secret where its `*` is).
pub fn tagged(address: &str, token: &str) -> Option<String> {
    let (local, domain) = address.trim().split_once('@')?;
    if token.is_empty() || !token.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(if local.contains('*') { format!("{}@{domain}", local.replacen('*', token, 1)) } else { format!("{local}+{token}@{domain}") })
}

/// Whether Mail's registrations are here for this workspace: Mail installed, and a Mail new enough to have them.
async fn available(ctx: &Ctx, tx: &mut Tx) -> Result<bool> {
    if ctx.schema.entity("Communication").is_none() {
        return Ok(false);
    }
    let table: Option<String> = sqlx::query_scalar("select to_regclass('comms_mail_route')::text").fetch_one(&mut **tx).await?;
    Ok(table.is_some())
}

/// Registers `address`; the reason when Mail refuses it (another app has it, it is a person's, or its domain is not one
/// of the workspace's mail domains).
async fn register(tx: &mut Tx, tenant: Uuid, address: &str) -> std::result::Result<(), String> {
    sqlx::query("savepoint helpdesk_register").execute(&mut **tx).await.map_err(|e| e.to_string())?;
    match routes::register(tx, tenant, &Registration { app: APP, address, keep_in_mailbox: true }).await {
        Ok(_) => {
            sqlx::query("release savepoint helpdesk_register").execute(&mut **tx).await.map_err(|e| e.to_string())?;
            Ok(())
        }
        Err(e) => {
            sqlx::query("rollback to savepoint helpdesk_register").execute(&mut **tx).await.map_err(|e| e.to_string())?;
            Err(e.to_string().chars().take(500).collect())
        }
    }
}

async fn note(k: &Kernel, sctx: &Ctx, tx: &mut Tx, address: Uuid, current: &Doc, registered: bool, why: Option<String>) -> Result<()> {
    let mut v = Doc::new();
    if b(current, "mail_registered") != registered {
        v.insert("mail_registered".into(), json!(registered));
    }
    if s(current, "registration_note").map(str::to_string) != why {
        v.insert("registration_note".into(), json!(why));
    }
    if !v.is_empty() {
        k.set_values_tx(sctx, tx, "SupportAddress", address, v).await?;
    }
    Ok(())
}

/// Keeps Mail's registration in step with a support address that was saved (`new`, with what it was in `old`) or deleted
/// (`new` none): the old address taken back when it moved, was disabled or went, the new one registered when it is enabled.
pub async fn sync(k: &Kernel, ctx: &Ctx, tx: &mut Tx, old: Option<&Doc>, new: Option<&Doc>) -> Result<()> {
    if !available(ctx, tx).await? {
        return Ok(());
    }
    let sctx = sys(ctx);
    let wanted = new.filter(|d| b(d, "enabled")).and_then(|d| s(d, "address")).map(inbound::norm);
    if let Some(was) = old.and_then(|d| s(d, "address")).map(inbound::norm).filter(|a| Some(a) != wanted.as_ref()) {
        routes::unregister(tx, ctx.tenant, APP, &was).await.or_else(|e| {
            tracing::warn!(address = %was, "a support address could not be taken back from Mail: {e}");
            Ok::<_, ferrum_kernel::Error>(false)
        })?;
    }
    let (Some(doc), Some(row)) = (new, new.and_then(|d| id(d, "id"))) else { return Ok(()) };
    match wanted {
        Some(address) => match register(tx, ctx.tenant, &address).await {
            Ok(()) => note(k, &sctx, tx, row, doc, true, None).await,
            Err(why) => note(k, &sctx, tx, row, doc, false, Some(why)).await,
        },
        None => note(k, &sctx, tx, row, doc, false, None).await,
    }
}

/// For the upkeep: every enabled address registered, every registration of Helpdesk's that no enabled address stands
/// behind taken back. Changes only what differs, so it is safe on every replica at once. Returns how many it changed.
pub async fn sync_all(k: &Kernel, ctx: &Ctx) -> Result<usize> {
    if ctx.schema.entity("SupportAddress").is_none() {
        return Ok(0);
    }
    let sctx = sys(ctx);
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    if !available(ctx, &mut tx).await? {
        tx.commit().await?;
        return Ok(0);
    }
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))").bind(format!("helpdesk-upkeep|mail|{}", ctx.tenant)).execute(&mut *tx).await?;
    let addresses = k.find_tx(&sctx, &mut tx, "SupportAddress", &[], Some("address asc"), 1000).await?;
    let mut have: Vec<String> = routes::registered(&mut tx, ctx.tenant, Some(APP)).await?.into_iter().map(|r| r.address).collect();
    let mut changed = 0;
    for a in &addresses {
        let (Some(row), Some(address)) = (id(a, "id"), s(a, "address").map(inbound::norm)) else { continue };
        if !b(a, "enabled") {
            if have.contains(&address) {
                routes::unregister(&mut tx, ctx.tenant, APP, &address).await?;
                changed += 1;
            }
            note(k, &sctx, &mut tx, row, a, false, None).await?;
            continue;
        }
        if have.contains(&address) {
            note(k, &sctx, &mut tx, row, a, true, None).await?;
            continue;
        }
        match register(&mut tx, ctx.tenant, &address).await {
            Ok(()) => {
                changed += 1;
                note(k, &sctx, &mut tx, row, a, true, None).await?;
            }
            Err(why) => note(k, &sctx, &mut tx, row, a, false, Some(why)).await?,
        }
        have.push(address);
    }
    let enabled: Vec<String> = addresses.iter().filter(|a| b(a, "enabled")).filter_map(|a| s(a, "address").map(inbound::norm)).collect();
    for stale in have.iter().filter(|h| !enabled.contains(h)) {
        if routes::unregister(&mut tx, ctx.tenant, APP, stale).await? {
            changed += 1;
        }
    }
    tx.commit().await?;
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tag_is_the_tickets_secret_in_the_address_it_is_answered_at() {
        assert_eq!(tagged("support@acme.test", "k3j9x2m1q7aa8888").as_deref(), Some("support+k3j9x2m1q7aa8888@acme.test"));
        assert_eq!(tagged("ticket-*@acme.test", "k3j9x2").as_deref(), Some("ticket-k3j9x2@acme.test"), "a pattern takes it where its star is");
        assert_eq!(tagged("support@acme.test", ""), None);
        assert_eq!(tagged("support@acme.test", "a b"), None, "never anything but letters and digits");
        assert_eq!(tagged("nonsense", "abc"), None);
    }
}
