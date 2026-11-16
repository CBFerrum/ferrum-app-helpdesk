//! Hours per agreement (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): each service agreement names its own working
//! calendar and its tickets count on it (two agreements with different hours and a holiday, daylight saving, a
//! ticket opened in the past, a reopening).

mod common;
use chrono::{DateTime, Utc};
use common::*;
use ferrum_kernel::Calendar;
use serde_json::{Value, json};
use uuid::Uuid;

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn when(v: &Value) -> DateTime<Utc> {
    at(v.as_str().unwrap_or_else(|| panic!("not a moment: {v}")))
}

fn week(from: &str, to: &str) -> Value {
    json!({"mon": [[from, to]], "tue": [[from, to]], "wed": [[from, to]], "thu": [[from, to]], "fri": [[from, to]]})
}

fn targets(medium: (&str, &str)) -> Value {
    json!([{"priority": "Medium", "response_hours": medium.0, "resolution_hours": medium.1}])
}

async fn make_calendar(e: &Env, name: &str, zone: &str, holidays: &[&str], hours: Value) {
    let cal: Calendar =
        serde_json::from_value(json!({"name": name, "zone": zone, "weekends": ["sat", "sun"], "holidays": holidays, "revision": 0, "hours": hours})).unwrap();
    e.k.set_logic_calendar(&e.admin, &cal).await.unwrap_or_else(|er| panic!("calendar {name}: {er:?}"));
}

async fn team(e: &Env, name: &str) -> Uuid {
    uid(&e.insert("SupportTeam", json!({"team_name": name, "members": [{"agent": format!("{}@acme.test", name.to_lowercase())}]})).await)
}

async fn calendars_named(e: &Env) -> Vec<String> {
    let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
    let names: Vec<String> =
        sqlx::query_scalar("select name from logic_calendar where tenant_id = $1 order by name").bind(e.tenant).fetch_all(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    names
}

#[test]
fn hours_per_agreement() {
    run(by_agreement());
}

async fn by_agreement() {
    let e = boot("cals", false, vec![]).await;
    let k = e.k.clone();
    // Monday to Friday 09:00-17:00 UTC with Monday 9 November a holiday, and 12:00-20:00 with none.
    make_calendar(&e, "sla_standard", "UTC", &["2026-11-09"], week("09:00", "17:00")).await;
    make_calendar(&e, "sla_late", "UTC", &[], week("12:00", "20:00")).await;
    make_calendar(&e, "sla_ny", "America/New_York", &[], week("09:00", "17:00")).await;
    let (billing, overseas) = (team(&e, "Billing").await, team(&e, "Overseas").await);
    let standard =
        uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "calendar": "sla_standard", "priorities": targets(("4", "16"))})).await);
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Billing", "team": billing.to_string(), "calendar": "sla_late", "priorities": targets(("2", "6"))}))
        .await;
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Overseas", "team": overseas.to_string(), "calendar": "sla_ny", "priorities": targets(("4", "16"))}))
        .await;
    // A calendar the workspace does not have would refuse every ticket's save: it is refused when the agreement is saved.
    let refused = k
        .insert(
            &e.admin,
            "ServiceLevelAgreement",
            common::obj(json!({"sla_name": "Typo", "team": team(&e, "Typos").await.to_string(), "calendar": "sla_nope", "priorities": targets(("1", "2"))})),
        )
        .await;
    assert!(refused.is_err_and(|er| er.to_string().contains("sla_nope")), "a calendar that is not there");

    // Friday 6 November, 16:00: the same ticket on two agreements.
    k.pin_clock(Some(at("2026-11-06T16:00:00Z")));
    let plain = e.insert("Ticket", json!({"subject": "Slow", "priority": "Medium", "opened_at": "2026-11-06T16:00:00Z"})).await;
    let late = e.insert("Ticket", json!({"subject": "Invoice", "priority": "Medium", "team": billing.to_string(), "opened_at": "2026-11-06T16:00:00Z"})).await;
    k.pin_clock(None);
    assert_eq!((plain["service_level_agreement"].clone(), plain["sla_calendar"].clone()), (json!(standard.to_string()), json!("sla_standard")));
    assert_eq!(late["sla_calendar"], json!("sla_late"));
    // 4 hours: one on Friday, none over the weekend or on the Monday holiday, three on Tuesday from 09:00.
    assert_eq!(when(&plain["response_by"]), at("2026-11-10T12:00:00Z"), "the holiday and the weekend are not counted");
    assert_eq!(when(&plain["resolution_by"]), at("2026-11-11T16:00:00Z"), "16 hours: one Friday, eight Tuesday, seven Wednesday");
    assert_eq!(when(&late["response_by"]), at("2026-11-06T18:00:00Z"), "its own hours: two hours from 16:00, a Friday evening");
    assert_eq!(when(&late["resolution_by"]), at("2026-11-09T14:00:00Z"), "6 hours: four on Friday, two on Monday from 12:00 (no holiday here)");
    // No agreement names the default: nothing is made for it by the fallback while every ticket has a calendar of its own.
    assert!(!calendars_named(&e).await.contains(&"helpdesk_hours".to_string()), "{:?}", calendars_named(&e).await);

    // Daylight saving, on an agreement's own calendar: Friday 30 October 16:00 EDT; the clocks go back on Sunday.
    k.pin_clock(Some(at("2026-10-30T20:00:00Z")));
    let ny = e.insert("Ticket", json!({"subject": "Overseas", "priority": "Medium", "team": overseas.to_string(), "opened_at": "2026-10-30T20:00:00Z"})).await;
    k.pin_clock(None);
    assert_eq!(when(&ny["response_by"]), at("2026-11-02T17:00:00Z"), "one hour on Friday, three on Monday from 09:00 EST (UTC-5)");

    // A ticket opened in the past (an import, a backdated one) is due by its own opening, and the breach is recorded once.
    k.pin_clock(Some(at("2026-11-06T16:00:00Z")));
    let old = e.insert("Ticket", json!({"subject": "Imported", "priority": "Medium", "opened_at": "2026-11-02T10:00:00Z"})).await;
    k.pin_clock(None);
    assert_eq!(when(&old["response_by"]), at("2026-11-02T14:00:00Z"), "four hours from Monday 10:00, not from the import");
    assert_eq!(old["response_state"], json!("running"));
    k.sweep_targets_at(at("2026-11-07T00:00:00Z")).await.unwrap();
    assert_eq!(e.get("Ticket", uid(&old)).await["response_state"], json!("breached"));

    // A reopening starts the resolution clock again from the reopening, not from the opening.
    k.pin_clock(Some(at("2026-11-02T10:00:00Z")));
    let reopen = uid(&e.insert("Ticket", json!({"subject": "Back", "priority": "Medium", "opened_at": "2026-11-02T10:00:00Z"})).await);
    k.pin_clock(Some(at("2026-11-02T12:00:00Z")));
    e.update("Ticket", reopen, json!({"status": "Resolved", "resolution_details": "Done"})).await;
    k.pin_clock(Some(at("2026-11-06T16:00:00Z")));
    let again = e.update("Ticket", reopen, json!({"status": "Open"})).await;
    k.pin_clock(None);
    assert_eq!(
        (again["resolution_state"].clone(), when(&again["resolution_by"])),
        (json!("running"), at("2026-11-11T16:00:00Z")),
        "16 hours from Friday 16:00"
    );
}
