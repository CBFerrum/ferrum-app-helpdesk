//! The working calendar the tickets' targets count on.
//!
//! Before 0.10 each agreement carried its own hours, holidays and a fixed UTC offset. Targets now count
//! on the platform's business calendar `helpdesk_hours` (Admin, Calendars: hours, a time zone with daylight
//! saving, dated exceptions), because a service target names one calendar. A workspace that has none gets it
//! made the first time a ticket needs it (or by the upkeep): from the default agreement's hours and holidays as
//! it had them (the offset as the nearest zone that never changes), else every hour of every day in UTC. An
//! existing calendar of that name is never touched. An administrator edits it from then on.

use ferrum_domain::util::{rows, s, sys};
use ferrum_kernel::{Ctx, Doc, Error, Filter, Kernel, Result, Tx};
use serde_json::{Value, json};

/// The calendar both of a ticket's targets name.
pub const CALENDAR: &str = "helpdesk_hours";

const DAYS: [(&str, &str); 7] =
    [("Monday", "mon"), ("Tuesday", "tue"), ("Wednesday", "wed"), ("Thursday", "thu"), ("Friday", "fri"), ("Saturday", "sat"), ("Sunday", "sun")];

/// The zone that has had the fixed offset `minutes` east of UTC at all times (no daylight saving), else UTC.
pub fn zone_of(minutes: i64) -> String {
    match minutes {
        0 => "UTC".into(),
        m if m % 60 == 0 && (-720..=840).contains(&m) => {
            let h = m / 60;
            format!("Etc/GMT{}{}", if h > 0 { "-" } else { "+" }, h.abs())
        }
        270 => "Asia/Kabul".into(),
        330 => "Asia/Kolkata".into(),
        345 => "Asia/Kathmandu".into(),
        390 => "Asia/Yangon".into(),
        _ => "UTC".into(),
    }
}

/// A calendar's `(zone, weekends, holidays, hours)` as an agreement described its own.
fn from_agreement(sla: &Doc) -> (String, Vec<String>, Vec<String>, Option<Value>) {
    let zone = zone_of(crate::int(sla, "time_zone_offset"));
    let holidays: Vec<String> = rows(sla, "holidays").iter().filter_map(|r| s(r, "holiday_date").map(str::to_string)).collect();
    let mut hours = serde_json::Map::new();
    for (name, key) in DAYS {
        let windows: Vec<Value> = rows(sla, "working_hours")
            .iter()
            .filter(|r| s(r, "weekday") == Some(name))
            .filter_map(|r| Some(json!([s(r, "start_time")?, pad(s(r, "end_time")?)])))
            .map(|w| json!([pad(w[0].as_str().unwrap_or_default()), w[1]]))
            .collect();
        if !windows.is_empty() {
            hours.insert(key.into(), Value::Array(windows));
        }
    }
    if hours.is_empty() {
        return (zone, vec![], holidays, None);
    }
    let closed: Vec<String> = DAYS.iter().filter(|(_, k)| !hours.contains_key(*k)).map(|(_, k)| k.to_string()).collect();
    (zone, closed, holidays, Some(Value::Object(hours)))
}

/// "9:30" as "09:30".
fn pad(t: &str) -> String {
    match t.trim().split_once(':') {
        Some((h, m)) if h.len() == 1 => format!("0{h}:{m}"),
        _ => t.trim().to_string(),
    }
}

/// Makes the calendar when the workspace has none of that name.
pub async fn ensure(k: &Kernel, ctx: &Ctx, tx: &mut Tx) -> Result<()> {
    let there: bool = sqlx::query_scalar("select exists (select 1 from logic_calendar where tenant_id = $1 and name = $2)")
        .bind(ctx.tenant)
        .bind(CALENDAR)
        .fetch_one(&mut **tx)
        .await?;
    if there {
        return Ok(());
    }
    let sctx = sys(ctx);
    let agreements = k.find_tx(&sctx, tx, "ServiceLevelAgreement", &[Filter::eq("enabled", true)], Some("sla_name asc"), 1000).await?;
    let plain = |a: &Doc| ["customer", "customer_group", "team", "ticket_type"].iter().all(|f| a.get(*f).is_none_or(Value::is_null));
    let chosen = agreements.iter().find(|a| plain(a)).or_else(|| agreements.first());
    let (zone, weekends, holidays, hours) = match chosen {
        Some(a) => {
            let full = k
                .get_tx(&sctx, tx, "ServiceLevelAgreement", ferrum_domain::util::id(a, "id").unwrap_or_default(), false)
                .await
                .map_err(|e| Error::internal(e.to_string()))?;
            from_agreement(&full)
        }
        None => ("UTC".into(), vec![], vec![], None),
    };
    sqlx::query("insert into logic_calendar (tenant_id, name, zone, weekends, holidays, hours) values ($1, $2, $3, $4, $5::date[], $6) on conflict do nothing")
        .bind(ctx.tenant)
        .bind(CALENDAR)
        .bind(zone)
        .bind(weekends)
        .bind(holidays)
        .bind(hours)
        .execute(&mut **tx)
        .await?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fixed_offset_becomes_the_zone_that_never_changes() {
        assert_eq!(
            (zone_of(0).as_str(), zone_of(330).as_str(), zone_of(-300).as_str(), zone_of(540).as_str()),
            ("UTC", "Asia/Kolkata", "Etc/GMT+5", "Etc/GMT-9")
        );
        assert_eq!(zone_of(-720), "Etc/GMT+12");
        assert_eq!(zone_of(840), "Etc/GMT-14");
        assert_eq!(zone_of(333), "UTC");
    }

    #[test]
    fn an_agreements_hours_become_a_calendars() {
        let sla = json!({"time_zone_offset": 330, "holidays": [{"holiday_date": "2026-11-02"}],
            "working_hours": [{"weekday": "Monday", "start_time": "9:00", "end_time": "18:00"}, {"weekday": "Tuesday", "start_time": "09:00", "end_time": "13:00"},
                              {"weekday": "Tuesday", "start_time": "14:00", "end_time": "18:00"}]});
        let (zone, closed, holidays, hours) = from_agreement(sla.as_object().unwrap());
        assert_eq!((zone.as_str(), holidays), ("Asia/Kolkata", vec!["2026-11-02".to_string()]));
        assert_eq!(closed, ["wed", "thu", "fri", "sat", "sun"]);
        assert_eq!(hours.unwrap(), json!({"mon": [["09:00", "18:00"]], "tue": [["09:00", "13:00"], ["14:00", "18:00"]]}));
        let (zone, closed, _, hours) = from_agreement(json!({"time_zone_offset": 0}).as_object().unwrap());
        assert_eq!((zone.as_str(), closed.is_empty(), hours), ("UTC", true, None), "no hours: round the clock");
    }
}
