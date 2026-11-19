//! Two more clocks on the platform's service targets (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): the next reply
//! (each later message from the customer answered in time) and the customer update (an open or held ticket not left
//! silent too long). The workspace's calendar is round the clock in UTC, so a due moment is its start plus the hours.
//! One scenario: the suites share one database and reset it.

mod common;
use chrono::{DateTime, Duration, Utc};
use common::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn next_reply_and_customer_update() {
    run(scenario());
}

fn when(v: &Value) -> DateTime<Utc> {
    v.as_str().unwrap_or_else(|| panic!("not a moment: {v}")).parse().unwrap()
}

fn near(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    (a - b).num_milliseconds().abs() < 1000
}

/// The `sent_at` of the ticket's latest message.
fn last_sent(t: &Value) -> DateTime<Utc> {
    when(&t["conversation"].as_array().unwrap().last().unwrap()["sent_at"])
}

async fn scenario() {
    let e = boot("clocks", false, vec![]).await;
    let k = e.k.clone();
    let both = json!({"priority": "Medium", "response_hours": "4", "resolution_hours": "16", "next_reply_hours": "2", "update_hours": "8"});
    let standard = uid(&e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "priorities": [both]})).await);
    let plain_team = uid(&e.insert("SupportTeam", json!({"team_name": "Plain", "members": [{"agent": "pat@acme.test"}]})).await);
    let plain = uid(&e
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Plain", "team": plain_team.to_string(), "priorities": [{"priority": "Medium", "response_hours": "4", "resolution_hours": "16"}]}),
        )
        .await);
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.expect("the workspace's calendar");

    // One message more on a ticket, as the ticket's form sends it: what is there, and the new one.
    let say = |ticket: Uuid, kind: &'static str, body: &'static str, internal: bool| {
        let e = &e;
        async move {
            let mut conv = e.get("Ticket", ticket).await["conversation"].as_array().cloned().unwrap_or_default();
            conv.push(json!({"author_kind": kind, "body": body, "is_internal": internal}));
            Value::Object(e.update("Ticket", ticket, json!({"conversation": conv})).await)
        }
    };
    let new_ticket = |subject: &'static str, team: Option<Uuid>| {
        let e = &e;
        async move {
            let mut v = json!({"subject": subject, "priority": "Medium", "contact_email": "cy@cust.test", "contact_name": "Cy",
                               "conversation": [{"author_kind": "Customer", "body": "Help"}]});
            if let Some(t) = team {
                v["team"] = json!(t.to_string());
            }
            uid(&e.insert("Ticket", v).await)
        }
    };
    let round = |ticket: Uuid| {
        let e = &e;
        async move { e.sql::<i32>("select round from record_target where tenant_id = $1 and record_id = $2 and target = 'next_reply'", &[ticket]).await }
    };

    // ── no_new_clock_before_the_first_response ──
    let a = new_ticket("Slow reports", None).await;
    let t = e.get("Ticket", a).await;
    assert_eq!((t["reply_owed_since"].clone(), t["next_reply_state"].clone()), (Value::Null, Value::Null), "the first response has its own clock");
    assert_eq!(t["next_reply_secs"], json!(7200), "the agreement's two hours are on the ticket");

    // ── next_reply_runs_from_the_oldest_unanswered_message ──
    say(a, "Agent", "Looking", false).await;
    let t = say(a, "Customer", "Still slow", false).await;
    let owed = last_sent(&t);
    assert_eq!((t["status"].clone(), t["next_reply_state"].clone()), (json!("Open"), json!("running")));
    assert!(near(when(&t["reply_owed_since"]), owed), "{t:?}");
    assert!(near(when(&t["next_reply_by"]), owed + Duration::hours(2)), "{}", t["next_reply_by"]);

    // ── a_second_customer_message_does_not_move_the_clock ──
    let t2 = say(a, "Customer", "Hello?", false).await;
    assert_eq!((t2["reply_owed_since"].clone(), t2["next_reply_by"].clone()), (t["reply_owed_since"].clone(), t["next_reply_by"].clone()));

    // ── a_reply_meets_it_and_the_next_message_starts_another ──
    let t = say(a, "Agent", "Fixed the index", false).await;
    assert_eq!((t["next_reply_state"].clone(), t["reply_owed_since"].clone()), (json!("met"), Value::Null));
    let t = say(a, "Customer", "Thanks, one more thing", false).await;
    assert_eq!(t["next_reply_state"], json!("running"));
    assert!(near(when(&t["next_reply_by"]), last_sent(&t) + Duration::hours(2)));
    assert_eq!(round(a).await, Some(2), "a second round");

    // ── the_sweep_breaches_a_late_next_reply_once ──
    e.exec("update record_target set due_at = now() - interval '1 hour' where tenant_id = $1 and record_id = $2 and target = 'next_reply'", &[a]).await;
    assert_eq!(k.sweep_targets_at(Utc::now()).await.unwrap(), 1);
    assert_eq!(k.sweep_targets_at(Utc::now()).await.unwrap(), 0, "once");
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    let t = e.get("Ticket", a).await;
    assert_eq!((t["next_reply_state"].clone(), t["sla_status"].clone(), t["next_reply_breached"].clone()), (json!("breached"), json!("Breached"), json!(true)));
    let events: Option<i64> = e
        .sql(
            "select count(*) from outbox where tenant_id = $1 and record_id = $2 and topic = 'Ticket.updated' and payload->'doc'->>'next_reply_state' = 'breached' and payload->'was' ? 'next_reply_state'",
            &[a],
        )
        .await;
    assert_eq!(events, Some(1), "the ticket's updated event says so, once");

    // ── a_late_next_reply_stays_marked ──
    let t = say(a, "Agent", "Sorry for the wait", false).await;
    assert_eq!(t["next_reply_state"], json!("met_late"));
    let t = say(a, "Customer", "And another", false).await;
    assert_eq!(t["next_reply_state"], json!("running"));
    assert_eq!(e.get("Ticket", a).await["next_reply_breached"], json!(true), "a late reply stays counted");

    // ── the_update_clock_follows_the_last_public_message ──
    let b = new_ticket("Waiting on a part", None).await;
    say(b, "Agent", "Ordering it", false).await;
    let t = Value::Object(e.update("Ticket", b, json!({"status": "On Hold"})).await);
    assert_eq!(t["update_state"], json!("running"), "{t:?}");
    assert!(near(when(&t["update_by"]), when(&t["update_from"]) + Duration::hours(8)), "{t:?}");
    let t = say(b, "Agent", "The part ships Monday", false).await;
    assert_eq!(t["status"], json!("On Hold"), "a reply keeps it held");
    assert!(near(when(&t["update_by"]), last_sent(&t) + Duration::hours(8)), "{}", t["update_by"]);
    let noted = say(b, "Agent", "Supplier is slow", true).await;
    assert_eq!(noted["update_by"], t["update_by"], "a note does not update the customer");

    // ── waiting_for_the_customer_meets_the_update_clock ──
    let t = Value::Object(e.update("Ticket", b, json!({"status": "Replied"})).await);
    assert_eq!(t["update_state"], json!("met"));
    let t = say(b, "Customer", "Any news?", false).await;
    assert_eq!((t["status"].clone(), t["update_state"].clone()), (json!("Open"), json!("running")));
    assert!(near(when(&t["update_by"]), last_sent(&t) + Duration::hours(8)), "a new round from their message");

    // ── an_agreement_without_the_new_hours_gives_no_new_clock ──
    let c = new_ticket("Plain one", Some(plain_team)).await;
    say(c, "Agent", "Hi", false).await;
    let t = say(c, "Customer", "Hi back", false).await;
    assert_eq!(t["service_level_agreement"], json!(plain.to_string()));
    for f in ["next_reply_state", "next_reply_by", "update_state", "update_by"] {
        assert_eq!(t[f], Value::Null, "{f}");
    }

    // ── new_hours_reach_open_tickets_at_the_upkeep ──
    e.update(
        "ServiceLevelAgreement",
        plain,
        json!({"priorities": [{"priority": "Medium", "response_hours": "4", "resolution_hours": "16", "next_reply_hours": "2"}]}),
    )
    .await;
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    assert_eq!(e.get("Ticket", c).await["next_reply_state"], json!("running"));

    // ── compliance_counts_the_new_misses ──
    let today = Utc::now().date_naive();
    let p: HashMap<String, String> = [("from_date", (today - Duration::days(30)).to_string()), ("to_date", today.to_string()), ("group_by", "SLA".into())]
        .into_iter()
        .map(|(a, b)| (a.to_string(), b))
        .collect();
    let r = ferrum_domain::reports::run(&k, &e.admin, "support_sla_compliance", &p).await.expect("the report");
    let row = r["rows"].as_array().unwrap().iter().find(|x| x["group"] == json!("Standard")).cloned().unwrap_or_else(|| panic!("{r}"));
    assert_eq!((row["next_reply_late"].clone(), row["update_missed"].clone()), (json!(1), json!(0)), "{r}");
    assert_eq!(r["totals"]["next_reply_late"], json!(1));
    let _ = standard;
}
