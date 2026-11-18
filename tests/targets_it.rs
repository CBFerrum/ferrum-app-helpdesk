//! Tickets' targets on the platform's clock (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): policies by
//! priority, customer, group and team as records; the working calendar the workspace owns (made round the clock,
//! then the administrator's), with daylight saving; waiting for the customer stops the clock; and the breach is
//! recorded once and raises an event.

mod common;
use chrono::{DateTime, Utc};
use common::*;
use ferrum_kernel::Calendar;
use serde_json::{Value, json};

fn at(s: &str) -> DateTime<Utc> {
    s.parse().unwrap()
}

fn when(v: &Value) -> DateTime<Utc> {
    at(v.as_str().unwrap_or_else(|| panic!("not a moment: {v}")))
}

fn targets(list: &[(&str, &str, &str)]) -> Value {
    list.iter().map(|(p, r, z)| json!({"priority": p, "response_hours": r, "resolution_hours": z})).collect()
}

#[test]
fn targets_on_the_platforms_clock() {
    run(scenario());
}

async fn scenario() {
    let e = boot("targets", false, vec![]).await;
    let k = e.k.clone();

    // ── the workspace's calendar: made every hour of every day in UTC, then the administrator's ──
    let standard =
        uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "priorities": targets(&[("Medium", "4", "16"), ("Urgent", "1", "4")])})).await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bea@acme.test"}]})).await);
    let billing_sla = uid(&e
        .insert("ServiceLevelAgreement", json!({"sla_name": "Billing", "team": billing.to_string(), "priorities": targets(&[("Medium", "2", "6")])}))
        .await);
    let calendars = || async { e.sql::<String>("select zone from logic_calendar where tenant_id = $1 and name = 'helpdesk_hours'", &[]).await };
    assert_eq!(calendars().await, None, "nothing is made until a ticket needs it");
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    assert_eq!(calendars().await.as_deref(), Some("UTC"), "the upkeep makes it when the workspace has agreements");
    let open: Option<bool> = e.sql("select hours is null and weekends = '{}' from logic_calendar where tenant_id = $1 and name = 'helpdesk_hours'", &[]).await;
    assert_eq!(open, Some(true), "round the clock");
    // The workspace's own: an administrator sets its hours, and nothing puts it back.
    let week = json!({"mon": [["09:00", "17:00"]], "tue": [["09:00", "17:00"]], "wed": [["09:00", "17:00"]], "thu": [["09:00", "17:00"]], "fri": [["09:00", "17:00"]]});
    let mine: Calendar = serde_json::from_value(
        json!({"name": "helpdesk_hours", "zone": "America/New_York", "weekends": ["sat", "sun"], "holidays": [], "revision": 0, "hours": week}),
    )
    .unwrap();
    k.set_logic_calendar(&e.admin, &mine).await.expect("the administrator's hours");
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    assert_eq!(calendars().await.as_deref(), Some("America/New_York"));

    // ── policies as records: by priority, then team ──
    // Friday 30 October 2026, 16:00 in New York (EDT, UTC-4): daylight saving ends on Sunday 1 November.
    let friday = at("2026-10-30T20:00:00Z");
    k.pin_clock(Some(friday));
    let medium = e.insert("Ticket", json!({"subject": "Slow reports", "priority": "Medium", "opened_at": "2026-10-30T20:00:00Z"})).await;
    let urgent = e.insert("Ticket", json!({"subject": "Down", "priority": "Urgent", "opened_at": "2026-10-30T20:00:00Z"})).await;
    let team_ticket =
        e.insert("Ticket", json!({"subject": "Invoice", "priority": "Medium", "team": billing.to_string(), "opened_at": "2026-10-30T20:00:00Z"})).await;
    k.pin_clock(None);
    assert_eq!(medium["service_level_agreement"], json!(standard.to_string()));
    assert_eq!(team_ticket["service_level_agreement"], json!(billing_sla.to_string()), "the team's own agreement");
    // 4 hours from Friday 16:00: one before 17:00, three on Monday 2 November from 09:00, which is EST (UTC-5): noon is 17:00Z.
    assert_eq!(when(&medium["response_by"]), at("2026-11-02T17:00:00Z"), "daylight saving counted: Monday's noon is five hours behind UTC, Friday's was four");
    assert_eq!(when(&urgent["response_by"]), at("2026-10-30T21:00:00Z"), "an hour: 16:00 to 17:00");
    assert_eq!(when(&team_ticket["response_by"]), at("2026-11-02T15:00:00Z"), "2 hours: one on Friday, one on Monday (10:00 EST)");
    assert_eq!(when(&medium["resolution_by"]), at("2026-11-03T21:00:00Z"), "16 hours: 1 on Friday, 8 on Monday, 7 on Tuesday: 16:00 EST");

    // ── waiting for the customer stops the clock ──
    let id = uid(&medium);
    k.pin_clock(Some(at("2026-11-02T14:00:00Z"))); // Monday 09:00 EST
    let waiting = e.update("Ticket", id, json!({"status": "Replied"})).await;
    assert_eq!((waiting["response_by"].clone(), waiting["response_state"].clone()), (Value::Null, json!("paused")), "no due moment while stopped");
    k.pin_clock(Some(at("2026-11-02T19:00:00Z"))); // five hours later, 14:00 EST
    let back = e.update("Ticket", id, json!({"status": "Open"})).await;
    assert_eq!(back["response_state"], json!("running"));
    // 1 hour was used on Friday and nothing since Monday morning; the customer was away 5 hours of it: 3 hours are left from 14:00 EST.
    assert_eq!(when(&back["response_by"]), at("2026-11-02T22:00:00Z"), "the remaining three hours run from the resume: 17:00 EST");
    k.pin_clock(None);
    let answered_at = at("2026-11-02T20:00:00Z");
    k.pin_clock(Some(answered_at));
    e.update("Ticket", id, json!({"conversation": [{"author_kind": "Agent", "author": "admin@targets.test", "body": "On it"}]})).await;
    k.pin_clock(None);
    let met = e.get("Ticket", id).await;
    assert_eq!(met["response_state"], json!("met"), "answered in time");
    assert!(met["first_response_hours"].is_string(), "the hours it took are kept for the reports: {met:?}");

    // ── the breach is recorded once, raises an event, and the inbox's fields follow ──
    let marked = k.sweep_targets_at(Utc::now()).await.unwrap();
    assert_eq!(
        marked, 4,
        "the urgent ticket's two targets and the team ticket's two; the first ticket waits for its customer, so its resolution is stopped, not late"
    );
    assert_eq!(k.sweep_targets_at(Utc::now()).await.unwrap(), 0, "once");
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    let late = e.get("Ticket", uid(&urgent)).await;
    assert_eq!(
        (late["response_state"].clone(), late["sla_status"].clone(), late["response_breached"].clone()),
        (json!("breached"), json!("Breached"), json!(true))
    );
    let mut tx = k.db.tenant_tx(e.tenant).await.unwrap();
    let events: i64 = sqlx::query_scalar("select count(*) from outbox where tenant_id = $1 and record_id = $2 and topic = 'Ticket.updated' and payload->'doc'->>'response_state' = 'breached' and payload->'was' ? 'response_state'")
        .bind(e.tenant)
        .bind(uid(&urgent))
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(events, 1, "the ticket's updated event says so, once");

    // ── a priority raised: the new seconds count from the save ──
    let raised = e.update("Ticket", uid(&team_ticket), json!({"priority": "Urgent"})).await;
    assert_eq!(raised["service_level_agreement"], json!(standard.to_string()), "Billing sets no Urgent target: the default's");
}
