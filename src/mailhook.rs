//! Where Helpdesk meets the Mail app. Mail receives and stores every message as a `Communication`; this hook,
//! registered on that entity for workspaces that have Helpdesk, hands a received email to [`crate::inbound`] in
//! the same transaction and puts the Communication on the ticket it became. Mail's tables are never read: the
//! hook sees the record Mail is saving, as any hook does.
//!
//! A failure here is the mail's own, not Mail's: the work is undone to a savepoint, an entry in the Inbound
//! Mail Log says what went wrong, and Mail goes on storing the message.

use crate::inbound::{self, Outcome};
use async_trait::async_trait;
use ferrum_domain::util::{id, s, sys};
use ferrum_kernel::{Ctx, Doc, DocHook, HookCtx, Kernel, Result, Tx};
use serde_json::json;
use uuid::Uuid;

pub struct CommunicationHook;

/// Puts a Mail message on a ticket.
pub(crate) async fn link(k: &Kernel, sctx: &Ctx, tx: &mut Tx, communication: Uuid, ticket: Uuid) -> Result<()> {
    let mut v = Doc::new();
    v.insert("reference_entity".into(), json!("Ticket"));
    v.insert("reference_id".into(), json!(ticket.to_string()));
    k.set_values_tx(sctx, tx, "Communication", communication, v).await?;
    Ok(())
}

#[async_trait]
impl DocHook for CommunicationHook {
    async fn after_save(&self, hc: &mut HookCtx<'_>, doc: &Doc, old: Option<&Doc>) -> Result<()> {
        if old.is_some() || hc.ctx.schema.entity("SupportAddress").is_none() {
            return Ok(());
        }
        let Some(mail) = inbound::from_communication(doc) else { return Ok(()) };
        // Mail filed onto some other record is that record's.
        if s(doc, "reference_entity").is_some_and(|e| e != "Ticket") {
            return Ok(());
        }
        sqlx::query("savepoint helpdesk_inbound").execute(&mut **hc.tx).await?;
        match inbound::receive(hc.kernel, hc.ctx, hc.tx, &mail).await {
            Ok(Outcome::Created(t) | Outcome::Added(t)) => {
                sqlx::query("release savepoint helpdesk_inbound").execute(&mut **hc.tx).await?;
                if let Some(c) = id(doc, "id").filter(|_| id(doc, "reference_id") != Some(t)) {
                    link(hc.kernel, &sys(hc.ctx), hc.tx, c, t).await?;
                }
            }
            Ok(_) => {
                sqlx::query("release savepoint helpdesk_inbound").execute(&mut **hc.tx).await?;
            }
            Err(e) => {
                sqlx::query("rollback to savepoint helpdesk_inbound").execute(&mut **hc.tx).await?;
                tracing::warn!("inbound mail {:?} was not taken in: {e}", mail.message_id);
                inbound::log_failure(hc.kernel, hc.ctx, hc.tx, &mail, &e.to_string()).await?;
            }
        }
        Ok(())
    }
}
