//! Support: tickets with SLAs on business hours, teams and assignment, knowledge base, warranty claims and maintenance.
//!
//! Hooks are registered per entity, so a tenant without the `support` package never runs them.

mod help;
mod hours;
pub mod inbound;
mod kb;
mod macros;
mod mailhook;
mod merge;
mod outgoing;
mod replies;
mod reports;
mod routing;
mod site;
mod survey;
mod tags;
mod ticket;
mod visits;
mod warranty;

use ferrum_domain::reports::ReportSource;
use ferrum_kernel::{Ctx, Doc, Error, HookRegistry, Kernel, Result, Tx};
use serde_json::Value;
use std::sync::Arc;
use uuid::Uuid;

/// Registers this package's document hooks.
pub fn register(hooks: &mut HookRegistry) {
    hooks
        .register("Ticket", Arc::new(ticket::TicketHook))
        .register("ServiceLevelAgreement", Arc::new(ticket::SlaHook))
        .register("SupportTeam", Arc::new(ticket::TeamHook))
        .register("TicketAssignmentRule", Arc::new(ticket::RuleHook))
        .register("KbArticle", Arc::new(kb::ArticleHook))
        .register("SavedReply", Arc::new(replies::ReplyHook))
        .register("WarrantyClaim", Arc::new(warranty::ClaimHook))
        .register("MaintenanceSchedule", Arc::new(visits::ScheduleHook))
        .register("MaintenanceVisit", Arc::new(visits::VisitHook))
        .register("TicketTag", Arc::new(tags::TagHook))
        .register("TagRule", Arc::new(tags::RuleHook))
        .register("SupportAddress", Arc::new(inbound::AddressHook))
        .register("InboundLog", Arc::new(inbound::LogHook));
    // Mail's records are Mail's: this runs only where Helpdesk is installed, and only reads the message Mail is saving.
    hooks.scoped(&["helpdesk".to_string()], |h| {
        h.register("Communication", Arc::new(mailhook::CommunicationHook));
    });
    hooks.add_site("helpdesk", Arc::new(site::HelpdeskSite));
}

/// This package's reports.
pub fn reports() -> Vec<Arc<dyn ReportSource>> {
    vec![Arc::new(reports::SupportReports)]
}

/// Periodic upkeep (every few minutes on every replica; must be idempotent and safe to run concurrently):
/// re-evaluates the SLA of open tickets and closes tickets resolved longer than the grace period.
/// A failing tenant is logged and does not stop the others; the first error is returned.
pub async fn maintenance(k: &Kernel) -> Result<()> {
    let mut tx = k.db.system_tx().await?;
    let tenants: Vec<Uuid> = sqlx::query_scalar("select id from tenant where status = 'active' order by id").fetch_all(&mut *tx).await?;
    tx.commit().await?;
    let mut first_error = None;
    for tenant in tenants {
        let res = async {
            let schema = k.registry.schema(tenant).await?;
            if schema.entity("Ticket").is_none() {
                return Ok(());
            }
            let ctx = Ctx::system(tenant, schema);
            hours::ensure_for_tenant(k, &ctx).await?;
            hours::recreate(k, &ctx).await?;
            help::ensure_defaults(k, &ctx).await?;
            let refreshed = ticket::refresh_slas(k, &ctx).await? + ticket::refresh_verdicts(k, &ctx).await?;
            survey::send_due(k, &ctx).await?;
            let escalated = ticket::escalate(k, &ctx).await? + routing::reassign_unanswered(k, &ctx).await?;
            let closed = ticket::auto_close(k, &ctx).await?;
            if refreshed + escalated + closed > 0 {
                tracing::info!(%tenant, refreshed, escalated, closed, "support maintenance");
            }
            Ok::<_, Error>(())
        }
        .await;
        if let Err(e) = res {
            tracing::warn!(%tenant, "support maintenance failed: {e}");
            first_error.get_or_insert(e);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Whether the upkeep already did `what` once for this workspace (`SupportSettings.upkeep_done`, a list of words).
pub(crate) async fn done(k: &Kernel, ctx: &Ctx, tx: &mut Tx, what: &str) -> Result<bool> {
    let settings = k.find_tx(&ferrum_domain::util::sys(ctx), tx, "SupportSettings", &[], None, 1).await?;
    Ok(settings.first().and_then(|d| d.get("upkeep_done")).and_then(Value::as_str).is_some_and(|l| l.split(',').any(|w| w.trim() == what)))
}

/// Records that the upkeep did `what`, in the transaction that did it.
pub(crate) async fn mark(k: &Kernel, ctx: &Ctx, tx: &mut Tx, what: &str) -> Result<()> {
    let sctx = ferrum_domain::util::sys(ctx);
    let settings = k.find_tx(&sctx, tx, "SupportSettings", &[], None, 1).await?;
    let list = settings.first().and_then(|d| d.get("upkeep_done")).and_then(Value::as_str).unwrap_or_default();
    let mut v = Doc::new();
    v.insert("upkeep_done".into(), Value::String(if list.is_empty() { what.to_string() } else { format!("{list},{what}") }));
    match settings.first().and_then(|d| ferrum_domain::util::id(d, "id")) {
        Some(row) => k.set_values_tx(&sctx, tx, "SupportSettings", row, v).await?,
        None => {
            k.insert_tx(&sctx, tx, "SupportSettings", v).await?;
        }
    }
    Ok(())
}

/// A whole-number field (missing or empty reads as 0).
fn int(doc: &Doc, key: &str) -> i64 {
    doc.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// Quoted physical table of a child table field (`e_parent__field`, shortened when long).
fn child_table(ctx: &Ctx, entity: &str, field: &str) -> Result<String> {
    ctx.schema
        .entity(entity)
        .and_then(|e| e.children.get(field))
        .map(|c| ferrum_kernel::ddl::q(&c.table))
        .ok_or_else(|| Error::internal(format!("{entity}.{field} is not a table")))
}

/// A linked record, or `None` when it does not exist (link validation reports that to the user).
async fn linked(k: &Kernel, ctx: &Ctx, tx: &mut Tx, entity: &str, id: Uuid) -> Result<Option<Doc>> {
    match ferrum_domain::util::get(k, ctx, tx, entity, id).await {
        Ok(d) => Ok(Some(d)),
        Err(Error::NotFound(_)) => Ok(None),
        Err(e) => Err(e),
    }
}

include!(concat!(env!("OUT_DIR"), "/packages.rs"));
include!(concat!(env!("OUT_DIR"), "/ui.rs"));

/// This app's manifest (`app.yaml`).
pub const MANIFEST: &str = include_str!("../app.yaml");

/// The manifest of the connector app with ERP (`connectors/erp/app.yaml`, package `helpdesk_erp`).
/// It ships with Helpdesk.
pub const CONNECTOR_MANIFEST: &str = include_str!("../connectors/erp/app.yaml");

/// This app's metadata packages, embedded at build time.
pub fn packages() -> std::result::Result<Vec<ferrum_meta::Package>, ferrum_meta::MetaError> {
    ferrum_meta::catalog::load_embedded(EMBEDDED)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_is_a_release_the_platform_takes() {
        let m = ferrum_apps::AppManifest::parse(MANIFEST).expect("app.yaml");
        assert_eq!(m.routes(), vec![("helpdesk".to_string(), vec![])]);
        let files = UI.iter().map(|(p, b)| (p.to_string(), b.to_vec())).collect();
        let ui = ferrum_apps::ui::prepare("helpdesk", files).expect("ui/dist is an interface release");
        assert!(UI.iter().any(|(p, _)| *p == "index.html") && !UI.iter().any(|(p, _)| p.starts_with('.')), "{}", ui.digest);
        ferrum_apps::AppManifest::parse(CONNECTOR_MANIFEST).expect("connectors/erp/app.yaml");
    }
}
