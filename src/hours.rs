//! The working calendars the tickets' targets count on: an agreement's own (`ServiceLevelAgreement.calendar`, the name
//! of one of the platform's calendars), else the workspace's `helpdesk_hours`. That one is made when a ticket needs it
//! and the workspace has none of that name, every hour of every day in UTC, and is the administrator's to change from
//! then on (Admin, Calendars): an existing calendar of that name is never touched.

use ferrum_domain::util::sys;
use ferrum_kernel::{Calendar, Ctx, Kernel, Result, Tx};
use std::collections::{BTreeMap, BTreeSet};

/// The calendar a ticket's targets name when its agreement names none.
pub const CALENDAR: &str = "helpdesk_hours";

/// Makes `helpdesk_hours` when the workspace has none of that name.
pub async fn ensure(k: &Kernel, ctx: &Ctx, tx: &mut Tx) -> Result<()> {
    let round_the_clock = Calendar {
        name: CALENDAR.to_string(),
        zone: "UTC".into(),
        weekends: vec![],
        holidays: BTreeSet::new(),
        revision: 0,
        hours: None,
        exceptions: BTreeMap::new(),
    };
    k.ensure_calendar_tx(&sys(ctx), tx, &round_the_clock).await?;
    Ok(())
}

/// For the upkeep: the calendar, when the workspace has agreements and none yet.
pub async fn ensure_for_tenant(k: &Kernel, ctx: &Ctx) -> Result<()> {
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    let any: bool =
        sqlx::query_scalar("select exists (select 1 from e_service_level_agreement where tenant_id = $1)").bind(ctx.tenant).fetch_one(&mut *tx).await?;
    if any {
        ensure(k, ctx, &mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The names of the workspace's calendars.
pub async fn names(tx: &mut Tx, tenant: uuid::Uuid) -> Result<BTreeSet<String>> {
    Ok(sqlx::query_scalar::<_, String>("select name from logic_calendar where tenant_id = $1").bind(tenant).fetch_all(&mut **tx).await?.into_iter().collect())
}
