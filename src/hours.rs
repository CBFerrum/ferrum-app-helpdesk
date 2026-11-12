//! The working calendars the tickets' targets count on.
//!
//! Before 0.10 each agreement carried its own hours, holidays and a fixed UTC offset. 0.10 moved the targets onto
//! the platform's business calendars and one calendar for everyone, `helpdesk_hours`; 0.10.2 gives each agreement
//! its own again (`ServiceLevelAgreement.calendar`, the name of a calendar; empty: `helpdesk_hours`).
//!
//! * `helpdesk_hours` is made, once and when absent, from the default agreement's hours and holidays as it had
//!   them (the offset as the nearest zone that never changes), else every hour of every day in UTC. An existing
//!   calendar of that name is never touched: an administrator edits it from then on.
//! * When a workspace is updated, the upkeep makes one calendar for every other agreement that had its own hours,
//!   holidays or time zone, or that existed before 0.10 (which then counted round the clock), from what the
//!   agreement still holds: `time_zone_offset`, `working_hours` and `holidays` were kept as hidden fields by 0.10.
//!   It does this once for a workspace (`upkeep_done`), so an agreement made later without a calendar stays on
//!   `helpdesk_hours`, and an administrator's changes are never put back. What cannot be recovered is said in the
//!   server's log (a fixed offset no zone without daylight saving has).

use chrono::{DateTime, NaiveDate, Utc};
use ferrum_domain::util::{id, rows, s, sys};
use ferrum_kernel::{Calendar, Ctx, Doc, Error, Filter, Kernel, Result, Tx};
use std::collections::{BTreeMap, BTreeSet};

/// The calendar a ticket's targets name when its agreement names none.
pub const CALENDAR: &str = "helpdesk_hours";

const DAYS: [(&str, &str); 7] =
    [("Monday", "mon"), ("Tuesday", "tue"), ("Wednesday", "wed"), ("Thursday", "thu"), ("Friday", "fri"), ("Saturday", "sat"), ("Sunday", "sun")];

/// The zone that has had the fixed offset `minutes` east of UTC at all times (no daylight saving), when there is one.
pub fn zone_exact(minutes: i64) -> Option<String> {
    Some(match minutes {
        0 => "UTC".into(),
        m if m % 60 == 0 && (-720..=840).contains(&m) => {
            let h = m / 60;
            format!("Etc/GMT{}{}", if h > 0 { "-" } else { "+" }, h.abs())
        }
        -570 => "Pacific/Marquesas".into(),
        210 => "Asia/Tehran".into(),
        270 => "Asia/Kabul".into(),
        330 => "Asia/Kolkata".into(),
        345 => "Asia/Kathmandu".into(),
        390 => "Asia/Yangon".into(),
        525 => "Australia/Eucla".into(),
        570 => "Australia/Darwin".into(),
        _ => return None,
    })
}

/// [`zone_exact`], else UTC.
#[cfg(test)]
pub fn zone_of(minutes: i64) -> String {
    zone_exact(minutes).unwrap_or_else(|| "UTC".into())
}

/// What an agreement held of its own calendar before 0.10.
#[derive(Debug, PartialEq)]
pub struct Recovered {
    pub zone: String,
    /// Whether the zone is exactly what the offset was (false: UTC stands in for an offset no such zone has).
    pub exact: bool,
    pub weekends: Vec<String>,
    pub holidays: Vec<NaiveDate>,
    pub hours: Option<BTreeMap<String, Vec<(String, String)>>>,
    /// Whether the agreement held anything at all: hours, holidays or an offset.
    pub any: bool,
}

/// "9:30" as "09:30".
fn pad(t: &str) -> String {
    match t.trim().split_once(':') {
        Some((h, m)) if h.len() == 1 => format!("0{h}:{m}"),
        _ => t.trim().to_string(),
    }
}

/// Windows sorted, those that touch or overlap joined (a calendar refuses overlapping ones; the old counting
/// treated them as one stretch).
fn joined(mut windows: Vec<(String, String)>) -> Vec<(String, String)> {
    windows.sort();
    let mut out: Vec<(String, String)> = Vec::new();
    for (a, z) in windows {
        match out.last_mut() {
            Some(last) if a <= last.1 => {
                if z > last.1 {
                    last.1 = z;
                }
            }
            _ => out.push((a, z)),
        }
    }
    out
}

/// A calendar's parts as an agreement described its own.
pub fn from_agreement(sla: &Doc) -> Recovered {
    let offset = crate::int(sla, "time_zone_offset");
    let holidays: BTreeSet<NaiveDate> = rows(sla, "holidays").iter().filter_map(|r| s(r, "holiday_date").and_then(|d| d.get(..10)?.parse().ok())).collect();
    let mut hours = BTreeMap::new();
    for (name, key) in DAYS {
        let windows: Vec<(String, String)> = rows(sla, "working_hours")
            .iter()
            .filter(|r| s(r, "weekday") == Some(name))
            .filter_map(|r| Some((pad(s(r, "start_time")?), pad(s(r, "end_time")?))))
            .collect();
        if !windows.is_empty() {
            hours.insert(key.to_string(), joined(windows));
        }
    }
    let any = offset != 0 || !holidays.is_empty() || !hours.is_empty();
    let (zone, exact) = match zone_exact(offset) {
        Some(z) => (z, true),
        None => ("UTC".to_string(), false),
    };
    let holidays = holidays.into_iter().collect();
    if hours.is_empty() {
        return Recovered { zone, exact, weekends: vec![], holidays, hours: None, any };
    }
    let weekends = DAYS.iter().filter(|(_, k)| !hours.contains_key(*k)).map(|(_, k)| k.to_string()).collect();
    Recovered { zone, exact, weekends, holidays, hours: Some(hours), any }
}

fn calendar(name: &str, r: Recovered) -> Calendar {
    Calendar {
        name: name.to_string(),
        zone: r.zone,
        weekends: r.weekends,
        holidays: r.holidays.into_iter().collect(),
        revision: 0,
        hours: r.hours,
        exceptions: BTreeMap::new(),
    }
}

/// A calendar name for an agreement: `sla_` and its name's words, at most 64 characters, not one of `taken`.
pub fn name_for(sla_name: &str, taken: &BTreeSet<String>) -> String {
    let mut slug = String::new();
    for c in sla_name.chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('_') {
            slug.push('_');
        }
    }
    let slug: String = slug.trim_matches('_').chars().take(50).collect();
    let base = format!("sla_{}", if slug.is_empty() { "agreement" } else { slug.trim_end_matches('_') });
    if !taken.contains(&base) {
        return base;
    }
    (2..).map(|n| format!("{base}_{n}")).find(|c| !taken.contains(c)).unwrap_or(base)
}

/// The agreement `helpdesk_hours` is made from: the enabled one without criteria, else the first by name.
fn source(agreements: &[Doc]) -> Option<&Doc> {
    let plain = |a: &Doc| ["customer", "customer_group", "team", "ticket_type"].iter().all(|f| a.get(*f).is_none_or(serde_json::Value::is_null));
    agreements.iter().find(|a| plain(a)).or_else(|| agreements.first())
}

async fn enabled(k: &Kernel, ctx: &Ctx, tx: &mut Tx) -> Result<Vec<Doc>> {
    k.find_tx(&sys(ctx), tx, "ServiceLevelAgreement", &[Filter::eq("enabled", true)], Some("sla_name asc"), 1000).await
}

async fn existing(tx: &mut Tx, tenant: uuid::Uuid) -> Result<BTreeSet<String>> {
    Ok(sqlx::query_scalar::<_, String>("select name from logic_calendar where tenant_id = $1").bind(tenant).fetch_all(&mut **tx).await?.into_iter().collect())
}

/// Makes `helpdesk_hours` when the workspace has none of that name.
pub async fn ensure(k: &Kernel, ctx: &Ctx, tx: &mut Tx) -> Result<()> {
    let sctx = sys(ctx);
    if existing(tx, ctx.tenant).await?.contains(CALENDAR) {
        return Ok(());
    }
    let agreements = enabled(k, ctx, tx).await?;
    let recovered = match source(&agreements).and_then(|a| id(a, "id")) {
        Some(a) => from_agreement(&k.get_tx(&sctx, tx, "ServiceLevelAgreement", a, false).await.map_err(|e| Error::internal(e.to_string()))?),
        None => Recovered { zone: "UTC".into(), exact: true, weekends: vec![], holidays: vec![], hours: None, any: false },
    };
    k.ensure_calendar_tx(&sctx, tx, &calendar(CALENDAR, recovered)).await?;
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

/// Once for a workspace: a calendar for each agreement other than the one `helpdesk_hours` came from that had its own
/// hours, holidays or time zone, or existed before 0.10 (the release that moved the targets onto one calendar, whose
/// migration's moment is when its SQL was applied), made from what the agreement still holds, and named by it. Returns
/// how many were made. A workspace whose agreements have no `calendar` field yet (an older release) is left alone.
pub async fn recreate(k: &Kernel, ctx: &Ctx) -> Result<usize> {
    if ctx.schema.entity("ServiceLevelAgreement").is_none_or(|e| e.field("calendar").is_none()) {
        return Ok(0);
    }
    let sctx = sys(ctx);
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    sqlx::query("select pg_advisory_xact_lock(hashtext($1))").bind(format!("helpdesk-upkeep|calendars|{}", ctx.tenant)).execute(&mut *tx).await?;
    if crate::done(k, ctx, &mut tx, "calendars").await? {
        tx.commit().await?;
        return Ok(0);
    }
    let all = k.find_tx(&sctx, &mut tx, "ServiceLevelAgreement", &[], Some("sla_name asc"), 1000).await?;
    let from = source(&enabled(k, ctx, &mut tx).await?).and_then(|a| id(a, "id"));
    let migrated: Option<DateTime<Utc>> =
        sqlx::query_scalar("select applied_at from cluster_package_sql where package = 'helpdesk' and file = '001_targets.sql'")
            .fetch_optional(&mut *tx)
            .await?;
    let before: BTreeSet<uuid::Uuid> = match migrated {
        Some(at) => sqlx::query_scalar("select id from e_service_level_agreement where tenant_id = $1 and created_at < $2")
            .bind(ctx.tenant)
            .bind(at)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .collect(),
        None => BTreeSet::new(),
    };
    let mut taken = existing(&mut tx, ctx.tenant).await?;
    let mut made = 0;
    for head in all {
        let Some(sla) = id(&head, "id") else { continue };
        if Some(sla) == from || s(&head, "calendar").is_some_and(|c| !c.trim().is_empty()) {
            continue;
        }
        let full = k.get_tx(&sctx, &mut tx, "ServiceLevelAgreement", sla, false).await?;
        let recovered = from_agreement(&full);
        if !recovered.any && !before.contains(&sla) {
            continue;
        }
        if !recovered.exact {
            tracing::warn!(tenant = %ctx.tenant, agreement = s(&full, "sla_name").unwrap_or_default(), offset = crate::int(&full, "time_zone_offset"),
                "the agreement's fixed UTC offset has no zone without daylight saving: its calendar is in UTC, to be set by an administrator");
        }
        let name = name_for(s(&full, "sla_name").unwrap_or_default(), &taken);
        match k.ensure_calendar_tx(&sctx, &mut tx, &calendar(&name, recovered)).await {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(tenant = %ctx.tenant, %sla, "the agreement's calendar could not be made, it stays on {CALENDAR}: {e}");
                continue;
            }
        }
        taken.insert(name.clone());
        let mut v = Doc::new();
        v.insert("calendar".into(), serde_json::json!(name));
        k.set_values_tx(&sctx, &mut tx, "ServiceLevelAgreement", sla, v).await?;
        made += 1;
    }
    crate::mark(k, ctx, &mut tx, "calendars").await?;
    tx.commit().await?;
    Ok(made)
}

/// The names of the workspace's calendars.
pub async fn names(tx: &mut Tx, tenant: uuid::Uuid) -> Result<BTreeSet<String>> {
    existing(tx, tenant).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn doc(v: serde_json::Value) -> Doc {
        v.as_object().cloned().unwrap()
    }

    #[test]
    fn a_fixed_offset_becomes_the_zone_that_never_changes() {
        assert_eq!(
            (zone_of(0).as_str(), zone_of(330).as_str(), zone_of(-300).as_str(), zone_of(540).as_str()),
            ("UTC", "Asia/Kolkata", "Etc/GMT+5", "Etc/GMT-9")
        );
        assert_eq!(zone_of(-720), "Etc/GMT+12");
        assert_eq!(zone_of(840), "Etc/GMT-14");
        assert_eq!(zone_of(333), "UTC");
        assert_eq!((zone_exact(210).as_deref(), zone_exact(570).as_deref(), zone_exact(-210)), (Some("Asia/Tehran"), Some("Australia/Darwin"), None));
    }

    #[test]
    fn an_agreements_hours_become_a_calendars() {
        let sla = doc(json!({"time_zone_offset": 330, "holidays": [{"holiday_date": "2026-11-02"}],
            "working_hours": [{"weekday": "Monday", "start_time": "9:00", "end_time": "18:00"}, {"weekday": "Tuesday", "start_time": "14:00", "end_time": "18:00"},
                              {"weekday": "Tuesday", "start_time": "09:00", "end_time": "13:00"}, {"weekday": "Wednesday", "start_time": "09:00", "end_time": "13:00"},
                              {"weekday": "Wednesday", "start_time": "12:00", "end_time": "15:00"}]}));
        let r = from_agreement(&sla);
        assert_eq!((r.zone.as_str(), r.exact, r.any), ("Asia/Kolkata", true, true));
        assert_eq!(r.holidays, vec!["2026-11-02".parse::<NaiveDate>().unwrap()]);
        assert_eq!(r.weekends, ["thu", "fri", "sat", "sun"]);
        let hours = r.hours.unwrap();
        assert_eq!(hours["mon"], [("09:00".to_string(), "18:00".to_string())]);
        assert_eq!(hours["tue"], [("09:00".to_string(), "13:00".to_string()), ("14:00".to_string(), "18:00".to_string())]);
        assert_eq!(hours["wed"], [("09:00".to_string(), "15:00".to_string())], "overlapping rows count as one stretch");
        let none = from_agreement(&doc(json!({"time_zone_offset": 0})));
        assert_eq!(
            (none.zone.as_str(), none.weekends.is_empty(), none.hours, none.any),
            ("UTC", true, None, false),
            "no hours: round the clock, and nothing held"
        );
        assert!(!from_agreement(&doc(json!({"time_zone_offset": -210}))).exact, "an offset without a zone is UTC, and says so");
        assert!(from_agreement(&doc(json!({"time_zone_offset": 60}))).any, "an offset alone is something");
    }

    #[test]
    fn calendars_are_named_by_their_agreements_and_never_twice() {
        let mut taken: BTreeSet<String> = ["helpdesk_hours".to_string()].into();
        assert_eq!(name_for("Premium 24x7 (EU)", &taken), "sla_premium_24x7_eu");
        taken.insert("sla_premium_24x7_eu".into());
        assert_eq!(name_for("premium 24x7 eu", &taken), "sla_premium_24x7_eu_2");
        assert_eq!(name_for("日本語", &taken), "sla_agreement");
        assert!(name_for(&"word ".repeat(30), &taken).len() <= 64);
    }
}
