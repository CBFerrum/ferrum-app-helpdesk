//! Tickets' targets on the platform's clock (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): policies by
//! priority, customer, group and team as records; the working calendar the workspace owns, with daylight saving;
//! waiting for the customer stops the clock; the breach is recorded once and raises an event; and a workspace
//! upgraded from a release that counted its own hours keeps its agreements and the clocks of its open tickets,
//! with no administrator action.

mod common;
use chrono::{DateTime, Duration, Utc};
use common::*;
use ferrum_kernel::Ctx;
use serde_json::{Value, json};
use uuid::Uuid;

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
    let sys = Ctx::system(e.tenant, k.registry.schema(e.tenant).await.unwrap());

    // ── the calendar is made from the default agreement the workspace had, once ──
    let hours = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"].map(|d| json!({"weekday": d, "start_time": "09:00", "end_time": "17:00"}));
    let standard = uid(&e
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Standard", "time_zone_offset": 330, "working_hours": hours, "holidays": [{"holiday_date": "2026-11-09", "description": "Local holiday"}],
                   "priorities": targets(&[("Medium", "4", "16"), ("Urgent", "1", "4")])}),
        )
        .await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bea@acme.test"}]})).await);
    let billing_sla = uid(&e
        .insert("ServiceLevelAgreement", json!({"sla_name": "Billing", "team": billing.to_string(), "priorities": targets(&[("Medium", "2", "6")])}))
        .await);
    let calendars = || async { e.sql::<String>("select zone from logic_calendar where tenant_id = $1 and name = 'helpdesk_hours'", &[]).await };
    assert_eq!(calendars().await, None, "nothing is made until a ticket needs it");
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(calendars().await.as_deref(), Some("Asia/Kolkata"), "the upkeep makes it from the default agreement, offset and all");
    let hours_row: Option<Value> = e.sql("select hours from logic_calendar where tenant_id = $1 and name = 'helpdesk_hours'", &[]).await;
    assert_eq!(hours_row.unwrap()["mon"], json!([["09:00", "17:00"]]));
    // The workspace's own: an administrator changes it, and nothing puts it back.
    e.exec(
        "update logic_calendar set zone = 'America/New_York', holidays = '{}', revision = revision + 1 where tenant_id = $1 and name = 'helpdesk_hours'",
        &[],
    )
    .await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
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
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
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
    let _ = (sys, Duration::zero());
    upgrade(&e).await;
}

/// A workspace as 0.9.x left it: its agreements, and open tickets that carry their own deadlines and no target rows.
async fn upgrade(e: &Env) {
    let k = e.k.clone();
    let tenant = e.tenant;
    // A second workspace's worth of state in the same cluster: legacy tickets inserted as the old code stored them.
    let sla: Uuid = e.sql("select id from e_service_level_agreement where tenant_id = $1 and sla_name = 'Standard'", &[]).await.unwrap();
    let legacy = |status: &'static str| status;
    let (running, missed, answered, waiting, done) = (Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7());
    let now = Utc::now();
    let ago = |h: i64| now - Duration::hours(h);
    let mut tx = k.db.tenant_tx(tenant).await.unwrap();
    sqlx::query("select set_config('ferrum.system', 'on', true)").execute(&mut *tx).await.unwrap();
    // "Before": the targets' fields are empty and no round exists (they come with this release), the old ones are filled.
    for (id, subject, status, opened, rby, zby, rbreach, zbreach, responded, history) in [
        (
            running,
            "Legacy running",
            "Open",
            ago(2),
            Some(now + Duration::hours(2)),
            Some(now + Duration::hours(14)),
            false,
            false,
            None,
            json!([{"status": "Open", "at": ago(2)}]),
        ),
        (missed, "Legacy missed", "Open", ago(30), Some(ago(26)), Some(ago(14)), true, true, None, json!([{"status": "Open", "at": ago(30)}])),
        (
            answered,
            "Legacy answered",
            "Open",
            ago(3),
            Some(now + Duration::hours(1)),
            Some(now + Duration::hours(13)),
            false,
            false,
            Some(ago(2)),
            json!([{"status": "Open", "at": ago(3)}]),
        ),
        (
            waiting,
            "Legacy waiting",
            "Replied",
            ago(5),
            None,
            None,
            false,
            false,
            Some(ago(4)),
            json!([{"status": "Open", "at": ago(5)}, {"status": "Replied", "at": ago(4)}]),
        ),
        (
            done,
            "Legacy done",
            "Resolved",
            ago(50),
            Some(ago(46)),
            Some(ago(34)),
            false,
            false,
            Some(ago(49)),
            json!([{"status": "Open", "at": ago(50)}, {"status": "Resolved", "at": ago(40)}]),
        ),
    ] {
        sqlx::query(
            "insert into e_ticket (tenant_id, id, subject, status, priority, channel, opened_at, service_level_agreement, response_by, resolution_by, \
             response_breached, resolution_breached, first_responded_at, resolved_at, status_history, sla_status, paused_hours) \
             values ($1, $2, $3, $4, 'Medium', 'Portal', $5, $6, $7, $8, $9, $10, $11, $12, $13, 'On Track', 0)",
        )
        .bind(tenant)
        .bind(id)
        .bind(subject)
        .bind(legacy(status))
        .bind(opened)
        .bind(sla)
        .bind(rby)
        .bind(zby)
        .bind(rbreach)
        .bind(zbreach)
        .bind(responded)
        .bind((status == "Resolved").then(|| ago(40)))
        .bind(history)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    // The release's migration, as a server upgrade runs it.
    sqlx::raw_sql(include_str!("../packages/helpdesk/sql/001_targets.sql")).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let round = |id: Uuid, target: &'static str| {
        let (k, tenant) = (k.clone(), tenant);
        async move {
            let mut tx = k.db.tenant_tx(tenant).await.unwrap();
            let row: Option<(i64, i64, Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> = sqlx::query_as(
                "select within_secs, worked_secs, due_at, paused_at, met_at, breached_at, started_at from record_target where tenant_id = $1 and entity = 'Ticket' and record_id = $2 and target = $3",
            )
            .bind(tenant)
            .bind(id)
            .bind(target)
            .fetch_optional(&mut *tx)
            .await
            .unwrap();
            tx.commit().await.unwrap();
            row
        }
    };
    let state = |id: Uuid| {
        let e_k = k.clone();
        async move {
            let mut tx = e_k.db.tenant_tx(tenant).await.unwrap();
            let r: (Option<String>, Option<String>) = sqlx::query_as("select response_state, resolution_state from e_ticket where tenant_id = $1 and id = $2")
                .bind(tenant)
                .bind(id)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
            r
        }
    };
    // Running: started when opened, due when it was due, on the seconds its agreement gives.
    let r = round(running, "respond").await.expect("a round");
    assert_eq!((r.0, r.1, r.3.is_none(), r.4.is_none(), r.5.is_none()), (4 * 3600, 0, true, true, true));
    assert!(
        r.2.is_some_and(|d| (d - (now + Duration::hours(2))).num_seconds().abs() <= 1) && r.6.is_some_and(|s| (s - ago(2)).num_seconds().abs() <= 1),
        "{r:?}"
    );
    assert_eq!(state(running).await, (Some("running".into()), Some("running".into())));
    assert_eq!(round(running, "resolve").await.unwrap().0, 16 * 3600);
    // Missed: stays missed, on both targets.
    assert!(round(missed, "respond").await.unwrap().5.is_some() && round(missed, "resolve").await.unwrap().5.is_some());
    assert_eq!(state(missed).await, (Some("breached".into()), Some("breached".into())));
    // Answered: its response stays given; only the resolution runs.
    let a = round(answered, "respond").await.unwrap();
    assert!(a.4.is_some() && a.5.is_none(), "met, not late: {a:?}");
    assert_eq!(state(answered).await, (Some("met".into()), Some("running".into())));
    // Waiting for the customer: stopped, with the working time it had used (about an hour: opened 5 hours ago, replied at 4, none counted stopped before).
    let w = round(waiting, "resolve").await.unwrap();
    assert!(w.3.is_some() && w.2.is_none(), "stopped, no due moment: {w:?}");
    assert!((3000..=3700).contains(&w.1), "about an hour used: {}", w.1);
    assert_eq!(state(waiting).await.1, Some("paused".into()));
    // Resolved before the upgrade: left as it was.
    assert!(round(done, "resolve").await.is_none() && state(done).await == (None, None));

    // The upkeep brings the inbox's fields up to date, and nothing needs an administrator: a save keeps the clock.
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let doc = e.get("Ticket", running).await;
    let before = when(&doc["response_by"]);
    let kept = e.update("Ticket", running, json!({"conversation": [{"author_kind": "Customer", "author": "c@x.test", "body": "Hello?"}]})).await;
    assert_eq!(kept["response_state"], json!("running"));
    assert!(
        (when(&kept["response_by"]) - before).num_seconds().abs() <= 1,
        "the same due moment after the first save in the new release: {} vs {before}",
        kept["response_by"]
    );
    assert_eq!(e.get("Ticket", missed).await["sla_status"], json!("Breached"));
    // Their agreements are the records they were.
    let policy = e.get("ServiceLevelAgreement", sla).await;
    assert_eq!((policy["sla_name"].clone(), policy["priorities"][0]["response_hours"].clone()), (json!("Standard"), json!("4")));
}
