//! Time on tickets (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): entries are lines of the ticket written through its
//! ordinary update, the total is the platform's rollup, an agent logs and changes only their own (a Support Manager may
//! change anyone's), the portal shows none, and a report totals the time. One scenario: the suites share one database.

mod common;
use chrono::{Duration, Utc};
use common::*;
use ferrum_kernel::capability::Invocation;
use ferrum_kernel::{Error, Result};
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn time_on_tickets() {
    run(scenario());
}

fn on<T: std::fmt::Debug>(r: &Result<T>, field: &str) -> bool {
    matches!(r, Err(Error::Validation(f)) if f.iter().any(|x| x.field == field))
}

fn lines(t: &ferrum_kernel::Doc) -> Vec<Value> {
    t["time"].as_array().cloned().unwrap_or_default()
}

/// What a form sends to keep the lines it has and add one.
fn keep_and(t: &ferrum_kernel::Doc, add: Option<Value>) -> Value {
    let mut v: Vec<Value> = lines(t).iter().map(|r| json!({"id": r["id"]})).collect();
    v.extend(add);
    Value::Array(v)
}

async fn scenario() {
    let e = boot("time", false, vec![]).await;
    let k = e.k.clone();
    let support = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [{"agent": "sam@acme.test"}, {"agent": "kim@acme.test"}]})).await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bo@acme.test"}]})).await);
    let sam = e.user("sam@acme.test", &["Support Agent"]).await;
    let kim = e.user("kim@acme.test", &["Support Agent"]).await;
    let mia = e.user("mia@acme.test", &["Support Manager"]).await;
    let ticket =
        uid(&e.insert("Ticket", json!({"subject": "Printer", "team": support.to_string(), "contact_email": "cy@cust.test", "contact_name": "Cy"})).await);
    let get = |id: Uuid| {
        let e = &e;
        async move { e.get("Ticket", id).await }
    };
    let save = |who: &ferrum_kernel::Ctx, id: Uuid, time: Value| {
        let (k, who) = (k.clone(), who.clone());
        async move { k.update(&who, "Ticket", id, obj(json!({"time": time})), None).await }
    };

    // ── an_agent_logs_their_own_time ──
    let t = save(&sam, ticket, keep_and(&get(ticket).await, Some(json!({"minutes": 30})))).await.expect("sam logs");
    let first = &lines(&t)[0];
    assert_eq!((first["agent"].clone(), first["billable"].clone()), (json!("sam@acme.test"), json!(false)));
    assert_eq!(first["spent_on"], json!(Utc::now().date_naive().to_string()));

    // ── the_total_is_the_sum ──
    let t = save(&kim, ticket, keep_and(&t, Some(json!({"minutes": 45, "billable": true, "note": "Called the vendor"})))).await.expect("kim logs");
    assert_eq!(t["minutes_spent"], json!(75));
    let t = e.update("Ticket", ticket, json!({"minutes_spent": 5})).await;
    assert_eq!(t["minutes_spent"], json!(75), "the platform's total, not what is sent");

    // ── an_agent_cannot_log_for_someone_else ──
    let r = save(&sam, ticket, keep_and(&t, Some(json!({"minutes": 10, "agent": "kim@acme.test"})))).await;
    assert!(on(&r, "agent"), "{r:?}");
    let t = save(&mia, ticket, keep_and(&t, Some(json!({"minutes": 10, "agent": "kim@acme.test"})))).await.expect("a manager logs for kim");
    assert_eq!(t["minutes_spent"], json!(85));

    // ── only_the_author_or_a_manager_changes_an_entry ──
    let sams = lines(&t).into_iter().find(|r| r["agent"] == json!("sam@acme.test")).unwrap();
    let without_sams: Value = lines(&t).iter().filter(|r| r["id"] != sams["id"]).map(|r| json!({"id": r["id"]})).collect();
    assert!(on(&save(&kim, ticket, without_sams.clone()).await, "time"), "kim removes sam's");
    let edited: Value = lines(&t).iter().map(|r| if r["id"] == sams["id"] { json!({"id": r["id"], "minutes": 5}) } else { json!({"id": r["id"]}) }).collect();
    assert!(on(&save(&kim, ticket, edited.clone()).await, "time"), "kim edits sam's");
    let t = save(&sam, ticket, edited).await.expect("sam edits his own");
    assert_eq!(t["minutes_spent"], json!(60));
    let t = save(&mia, ticket, without_sams).await.expect("a manager removes it");
    assert_eq!(t["minutes_spent"], json!(55));

    // ── a_stale_write_is_refused_not_merged ──
    let stale = get(ticket).await;
    save(&kim, ticket, keep_and(&stale, Some(json!({"minutes": 15})))).await.unwrap();
    let r = k.update(&sam, "Ticket", ticket, obj(json!({"time": keep_and(&stale, Some(json!({"minutes": 20})))})), stale["version"].as_i64()).await;
    assert!(matches!(r, Err(Error::Conflict(_))), "{r:?}");
    assert_eq!(get(ticket).await["minutes_spent"], json!(70), "kim's entry is still there");

    // ── logging_repeats_under_one_key ──
    let now = get(ticket).await;
    let again =
        || Invocation::new("helpdesk.ticket.update", json!({"id": ticket, "values": {"time": keep_and(&now, Some(json!({"minutes": 1})))}})).key("log-1");
    let one = k.invoke(&sam, again()).await.expect("logged");
    let two = k.invoke(&sam, again()).await.expect("repeated");
    assert!(!one.repeated && two.repeated);
    assert_eq!(get(ticket).await["minutes_spent"], json!(71), "one entry");

    // ── minutes_are_bounded ──
    for bad in [0, 1441] {
        assert!(save(&sam, ticket, keep_and(&get(ticket).await, Some(json!({"minutes": bad})))).await.is_err(), "{bad}");
    }

    // ── a_closed_ticket_takes_no_time ──
    let done = uid(&e.insert("Ticket", json!({"subject": "Done", "team": support.to_string(), "contact_email": "cy@cust.test"})).await);
    e.update("Ticket", done, json!({"status": "Resolved", "resolution_details": "Fixed"})).await;
    e.update("Ticket", done, json!({"status": "Closed"})).await;
    assert!(save(&sam, done, json!([{"minutes": 5}])).await.is_err(), "a closed ticket is final");

    // ── the_portal_shows_no_time ──
    let cy = e.portal_user("cy@cust.test").await;
    let seen = call(&k, Some(&cy), e.tenant, "GET", &format!("portal/tickets/{ticket}"), Value::Null).await.expect("the customer's ticket");
    assert!(seen.get("time").is_none() && seen.get("minutes_spent").is_none(), "{seen}");

    // ── the_report_totals_by_agent_and_scope ──
    let other = uid(&e.insert("Ticket", json!({"subject": "Invoice", "team": billing.to_string(), "contact_email": "dee@cust.test"})).await);
    save(&mia, other, json!([{"minutes": 90, "agent": "bo@acme.test", "billable": true}])).await.unwrap();
    let today = Utc::now().date_naive();
    let params = |g: &str| -> HashMap<String, String> {
        [("from_date", (today - Duration::days(30)).to_string()), ("to_date", today.to_string()), ("group_by", g.to_string())]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b))
            .collect()
    };
    let r = ferrum_domain::reports::run(&k, &e.admin, "support_time_spent", &params("Agent")).await.expect("the report");
    let row = |r: &Value, g: &str| r["rows"].as_array().unwrap().iter().find(|x| x["group"] == json!(g)).cloned().unwrap_or_else(|| panic!("{g} in {r}"));
    let bo = row(&r, "bo@acme.test");
    assert_eq!((bo["entries"].clone(), bo["minutes"].clone(), bo["billable_minutes"].clone()), (json!(1), json!(90), json!(90)));
    assert_eq!(r["totals"]["minutes"], json!(161));
    assert!(r["totals"]["hours"].is_string() || r["totals"]["hours"].is_number(), "{r}");
    k.set_user_scopes(&e.admin, sam.user, "SupportTeam", &[support], None).await.unwrap();
    let scoped = k.load_ctx(e.tenant, sam.user, None, "it".into()).await.unwrap();
    let r = ferrum_domain::reports::run(&k, &scoped, "support_time_spent", &params("Agent")).await.expect("the report, scoped");
    assert!(r["rows"].as_array().unwrap().iter().all(|x| x["group"] != json!("bo@acme.test")), "only his team's tickets: {r}");
    let by_month = ferrum_domain::reports::run(&k, &e.admin, "support_time_spent", &params("Month")).await.unwrap();
    assert_eq!(row(&by_month, &today.format("%Y-%m").to_string())["minutes"], json!(161));
}
