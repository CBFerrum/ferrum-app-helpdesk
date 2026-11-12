//! Hours per agreement (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): each service agreement names its own working
//! calendar and its tickets count on it (two agreements with different hours and a holiday, daylight saving, a
//! ticket opened in the past, a reopening); and a workspace updated from 0.10.0, or from 0.9.1 straight, gets one
//! calendar for each agreement that had its own hours, keeps the clocks of its open tickets, and is told by nothing
//! it has to do.

mod common;
use chrono::{DateTime, Duration, Utc};
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

fn weekdays(from: &str, to: &str) -> Vec<Value> {
    ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"].iter().map(|d| json!({"weekday": d, "start_time": from, "end_time": to})).collect()
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

/// A running round: (worked seconds, paused, calendar it was worked out on, anchored).
async fn round(e: &Env, ticket: Uuid, target: &str) -> (i64, bool, Option<String>, bool) {
    let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
    let r: (i64, Option<DateTime<Utc>>, Option<String>, bool) = sqlx::query_as(
        "select worked_secs, paused_at, calendar, anchored from record_target where tenant_id = $1 and entity = 'Ticket' and record_id = $2 and target = $3",
    )
    .bind(e.tenant)
    .bind(ticket)
    .bind(target)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (r.0, r.1.is_some(), r.2, r.3)
}

#[test]
fn hours_per_agreement() {
    run(async {
        by_agreement().await;
        from_0_10_0().await;
        from_0_9_1().await;
    });
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

/// A workspace as 0.10.0 left it: agreements that kept their old hours (hidden), no `calendar`, one workspace calendar, tickets running on it.
async fn from_0_10_0() {
    let e = boot("cals10", false, vec![]).await;
    let k = e.k.clone();
    let (billing, night, plain_team) = (team(&e, "Billing").await, team(&e, "Night").await, team(&e, "Plain").await);
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "working_hours": weekdays("09:00", "17:00"), "priorities": targets(("4", "16"))})).await;
    e.insert(
        "ServiceLevelAgreement",
        json!({"sla_name": "Premium", "team": billing.to_string(), "time_zone_offset": 330, "working_hours": weekdays("10:00", "18:00"),
               "holidays": [{"holiday_date": "2026-11-04", "description": "Local"}], "priorities": targets(("4", "16"))}),
    )
    .await;
    let support24 =
        uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Support 24", "team": night.to_string(), "priorities": targets(("4", "16"))})).await);
    let plain = uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Plain", "team": plain_team.to_string(), "priorities": targets(("4", "16"))})).await);
    // Support 24 existed before 0.10, when an agreement with no hours counted round the clock; Plain was made since.
    e.exec("update e_service_level_agreement set created_at = '2020-01-01T00:00:00Z' where tenant_id = $1 and id = $2", &[support24]).await;
    // Tickets of the update's day: the workspace's calendar is made as one needs it.
    k.pin_clock(Some(at("2026-11-02T09:00:00Z")));
    let running =
        uid(&e.insert("Ticket", json!({"subject": "Running", "priority": "Medium", "team": billing.to_string(), "opened_at": "2026-11-02T09:00:00Z"})).await);
    let waiting =
        uid(&e.insert("Ticket", json!({"subject": "Waiting", "priority": "Medium", "team": billing.to_string(), "opened_at": "2026-11-02T09:00:00Z"})).await);
    let on_plain =
        uid(&e.insert("Ticket", json!({"subject": "Plain", "priority": "Medium", "team": plain_team.to_string(), "opened_at": "2026-11-02T09:00:00Z"})).await);
    assert_eq!(calendars_named(&e).await, ["helpdesk_hours"], "as 0.10.0 had it: one calendar, from the default agreement");
    k.pin_clock(Some(at("2026-11-02T10:00:00Z")));
    e.update("Ticket", waiting, json!({"status": "Replied"})).await;
    let before_wait = round(&e, waiting, "respond").await;
    assert_eq!(
        (before_wait.0, before_wait.1, before_wait.2.as_deref(), before_wait.3),
        (3600, true, Some("helpdesk_hours"), true),
        "an hour used on the workspace calendar, then stopped"
    );
    let due_before = e.get("Ticket", on_plain).await["response_by"].clone();

    // The update, and the upkeep that follows it: two hours into the running ticket's first response.
    k.pin_clock(Some(at("2026-11-02T11:00:00Z")));
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(
        calendars_named(&e).await,
        ["helpdesk_hours", "sla_premium", "sla_support_24"],
        "one for the agreement with its own hours, one for the old round-the-clock one; the default's are helpdesk_hours, Plain has none"
    );
    let premium = e.list("ServiceLevelAgreement", json!({"sla_name": "Premium"})).await.remove(0);
    assert_eq!(premium["calendar"], json!("sla_premium"));
    assert_eq!(e.get("ServiceLevelAgreement", plain).await["calendar"], Value::Null, "an agreement made since 0.10 stays on the workspace's calendar");
    assert_eq!(e.get("ServiceLevelAgreement", support24).await["calendar"], json!("sla_support_24"));
    let cal: (String, Value, Vec<chrono::NaiveDate>) = {
        let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
        let r = sqlx::query_as("select zone, hours, holidays from logic_calendar where tenant_id = $1 and name = 'sla_premium'")
            .bind(e.tenant)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        r
    };
    assert_eq!(
        (cal.0.as_str(), cal.1["mon"].clone(), cal.2),
        ("Asia/Kolkata", json!([["10:00", "18:00"]]), vec!["2026-11-04".parse().unwrap()]),
        "the hours, holiday and zone it had"
    );
    let t = e.get("Ticket", running).await;
    assert_eq!(t["sla_calendar"], json!("sla_premium"));
    // Two hours were used on the workspace's calendar (09:00-11:00 UTC); the other two count on Premium's, which is open
    // from 04:30 to 12:30 UTC: an hour and a half more today, half an hour tomorrow.
    assert_eq!(when(&t["response_by"]), at("2026-11-03T05:00:00Z"), "the time used is kept as it was counted, the rest counts on the agreement's hours");
    let r = round(&e, running, "respond").await;
    assert_eq!((r.0, r.1, r.2.as_deref()), (7200, false, Some("sla_premium")));
    // The one waiting for its customer is still stopped with the hour it used.
    let w = e.get("Ticket", waiting).await;
    assert_eq!((w["response_state"].clone(), w["response_by"].clone(), w["sla_calendar"].clone()), (json!("paused"), Value::Null, json!("sla_premium")));
    assert_eq!(round(&e, waiting, "respond").await.0, 3600);
    // It resumes on Premium's hours: three of the four hours are left.
    k.pin_clock(Some(at("2026-11-03T05:00:00Z")));
    let resumed = e.update("Ticket", waiting, json!({"status": "Open"})).await;
    assert_eq!(when(&resumed["response_by"]), at("2026-11-03T08:00:00Z"));
    // The ticket on an agreement without a calendar of its own is as it was.
    assert_eq!(e.get("Ticket", on_plain).await["response_by"], due_before);
    assert_eq!(e.get("Ticket", on_plain).await["sla_calendar"], Value::Null);
    // Done once: an administrator's calendar is not made again over, and a later agreement with old-style data is left alone.
    e.exec("update logic_calendar set zone = 'Asia/Dubai' where tenant_id = $1 and name = 'sla_premium'", &[]).await;
    let later = uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Later", "team": team(&e, "Later").await.to_string(), "working_hours": weekdays("01:00", "02:00"), "priorities": targets(("4", "16"))})).await);
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.get("ServiceLevelAgreement", later).await["calendar"], Value::Null);
    assert_eq!(calendars_named(&e).await.len(), 3);
    assert_eq!(e.sql::<String>("select zone from logic_calendar where tenant_id = $1 and name = 'sla_premium'", &[]).await.as_deref(), Some("Asia/Dubai"));
    assert!(e.list("SupportSettings", json!({})).await[0]["upkeep_done"].as_str().unwrap().contains("calendars"));
    k.pin_clock(None);
}

/// A workspace as 0.9.1 left it, upgraded straight to this version: the agreement's own hours counted its tickets' clocks, and the
/// tickets still open carry the deadlines they were given.
async fn from_0_9_1() {
    let e = boot("cals091", false, vec![]).await;
    let k = e.k.clone();
    let tenant = e.tenant;
    let billing = team(&e, "Billing").await;
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "working_hours": weekdays("09:00", "17:00"), "priorities": targets(("4", "16"))})).await;
    let premium = uid(&e
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Premium", "team": billing.to_string(), "time_zone_offset": -300, "working_hours": weekdays("08:00", "16:00"), "priorities": targets(("4", "16"))}),
        )
        .await);
    let now = Utc::now();
    let (running, waiting) = (Uuid::now_v7(), Uuid::now_v7());
    let mut tx = k.db.tenant_tx(tenant).await.unwrap();
    sqlx::query("select set_config('ferrum.system', 'on', true)").execute(&mut *tx).await.unwrap();
    for (id, subject, status, opened, rby, zby, history) in [
        (
            running,
            "Legacy running",
            "Open",
            now - Duration::hours(2),
            Some(now + Duration::hours(2)),
            Some(now + Duration::hours(14)),
            json!([{"status": "Open", "at": now - Duration::hours(2)}]),
        ),
        (
            waiting,
            "Legacy waiting",
            "Replied",
            now - Duration::hours(5),
            None,
            None,
            json!([{"status": "Open", "at": now - Duration::hours(5)}, {"status": "Replied", "at": now - Duration::hours(4)}]),
        ),
    ] {
        sqlx::query(
            "insert into e_ticket (tenant_id, id, subject, status, priority, channel, opened_at, team, service_level_agreement, response_by, resolution_by, \
             response_breached, resolution_breached, first_responded_at, status_history, sla_status, paused_hours) \
             values ($1, $2, $3, $4, 'Medium', 'Portal', $5, $6, $7, $8, $9, false, false, $10, $11, 'On Track', 0)",
        )
        .bind(tenant)
        .bind(id)
        .bind(subject)
        .bind(status)
        .bind(opened)
        .bind(billing)
        .bind(premium)
        .bind(rby)
        .bind(zby)
        .bind((status == "Replied").then(|| now - Duration::hours(4)))
        .bind(history)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    // The migration the release before this one ran (a first-response and resolution round for each open ticket).
    sqlx::raw_sql(include_str!("../packages/helpdesk/sql/001_targets.sql")).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    // The legacy rows are older than the migration's record of itself: this is the workspace that was there before.
    e.exec("update e_service_level_agreement set created_at = '2020-01-01T00:00:00Z' where tenant_id = $1", &[]).await;
    let due = e.get("Ticket", running).await["response_by"].clone();
    assert_eq!(round(&e, running, "respond").await.2, None, "a round from before the platform recorded what it was counted on");

    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(calendars_named(&e).await, ["helpdesk_hours", "sla_premium"], "the default's hours are helpdesk_hours; Premium had its own offset and hours");
    let zone: Option<String> = e.sql("select zone from logic_calendar where tenant_id = $1 and name = 'sla_premium'", &[]).await;
    assert_eq!(zone.as_deref(), Some("Etc/GMT+5"), "its fixed offset (UTC-5), as a zone that never changes");
    let t = e.get("Ticket", running).await;
    assert_eq!((t["sla_calendar"].clone(), t["response_state"].clone()), (json!("sla_premium"), json!("running")));
    assert!((when(&t["response_by"]) - when(&due)).num_seconds().abs() <= 1, "the deadline it was given is the one it has: {} against {due}", t["response_by"]);
    assert_eq!(round(&e, running, "respond").await.2.as_deref(), Some("sla_premium"), "now counted on the agreement's calendar");
    let w = e.get("Ticket", waiting).await;
    assert_eq!(
        (w["response_state"].clone(), w["resolution_state"].clone(), w["resolution_by"].clone()),
        (json!("met"), json!("paused"), Value::Null),
        "answered, and still stopped"
    );
}
