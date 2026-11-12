//! The reports a service desk lives by (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first), read from the tickets' own fields and
//! the platform's target states: first response and resolution against their targets, the backlog by age, volume by channel,
//! tag and customer, the reopen rate, each by agent and team, and within an agent's record scopes.

mod common;
use chrono::{Duration, Utc};
use common::*;
use serde_json::{Value, json};
use std::collections::HashMap;

#[test]
fn reports() {
    run(scenario());
}

fn row<'a>(r: &'a Value, group: &str) -> &'a Value {
    r["rows"].as_array().unwrap().iter().find(|x| x["group"] == json!(group)).unwrap_or_else(|| panic!("{group} in {r}"))
}

async fn scenario() {
    let e = boot("reports", false, vec![]).await;
    let k = e.k.clone();
    let today = Utc::now().date_naive();
    let params = |g: &str| -> HashMap<String, String> {
        [("from_date", (today - Duration::days(60)).to_string()), ("to_date", today.to_string()), ("group_by", g.to_string())]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    };
    let run = |name: &'static str, g: &'static str| {
        let (k, admin, p) = (k.clone(), e.admin.clone(), params(g));
        async move { ferrum_domain::reports::run(&k, &admin, name, &p).await.unwrap() }
    };
    // A workspace upgraded from before: no agreements, no tags, tickets with none of the new fields.
    let sup = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [{"agent": "ann@acme.test"}]})).await);
    let bil = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bo@acme.test"}]})).await);
    let acme = uid(&e.insert("Customer", json!({"customer_name": "Acme", "email": "a@acme.test", "customer_group": e.list("CustomerGroup", json!({"group_name": "Individual"})).await[0]["id"]})).await);
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "priorities": [{"priority": "Medium", "response_hours": "1", "resolution_hours": "4"}, {"priority": "High", "response_hours": "1", "resolution_hours": "4"}]})).await;
    let make = |subject: &'static str, channel: &'static str, team: uuid::Uuid, agent: &'static str, customer: Option<uuid::Uuid>| {
        let e = &e;
        async move {
            let mut v = json!({"subject": subject, "channel": channel, "team": team.to_string(), "assigned_to": agent, "contact_email": "who@x.test", "contact_name": "Who"});
            if let Some(c) = customer {
                v["customer"] = json!(c.to_string());
            }
            uid(&e.insert("Ticket", v).await)
        }
    };
    let (a, b, c, d) = (
        make("One", "Email", sup, "ann@acme.test", Some(acme)).await,
        make("Two", "Email", sup, "ann@acme.test", None).await,
        make("Three", "Portal", bil, "bo@acme.test", Some(acme)).await,
        make("Four", "Chat", bil, "bo@acme.test", None).await,
    );
    k.set_tags(&e.admin, "Ticket", a, &["vip".into(), "refunds".into()]).await.unwrap();
    k.set_tags(&e.admin, "Ticket", b, &["vip".into()]).await.unwrap();
    // One answered, one solved and reopened, one solved, one aged.
    let say = |t, body: &'static str| {
        let e = &e;
        async move {
            let mut conv = e.get("Ticket", t).await["conversation"].as_array().cloned().unwrap_or_default();
            conv.push(json!({"author_kind": if body == "again" { "Customer" } else { "Agent" }, "author": "ann@acme.test", "body": body}));
            e.update("Ticket", t, json!({"conversation": conv})).await
        }
    };
    say(a, "On it").await;
    e.update("Ticket", c, json!({"status": "Resolved", "resolution_details": "Fixed"})).await;
    say(c, "again").await;
    e.update("Ticket", c, json!({"status": "Resolved", "resolution_details": "Fixed again"})).await;
    e.update("Ticket", b, json!({"status": "Resolved", "resolution_details": "Done"})).await;
    e.exec("update e_ticket set opened_at = now() - interval '10 days' where tenant_id = $1 and id = $2", &[d]).await;
    k.sweep_targets_at(Utc::now() + Duration::days(30)).await.unwrap();
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();

    // ── volume: by channel, tag, customer, team ──
    let by_channel = run("support_volume", "Channel").await;
    assert_eq!(
        (row(&by_channel, "Email")["tickets"].clone(), row(&by_channel, "Email")["resolved"].clone(), row(&by_channel, "Portal")["reopened"].clone()),
        (json!(2), json!(1), json!(1))
    );
    let by_tag = run("support_volume", "Tag").await;
    assert_eq!(
        (row(&by_tag, "vip")["tickets"].clone(), row(&by_tag, "refunds")["tickets"].clone(), row(&by_tag, "No Tag")["tickets"].clone()),
        (json!(2), json!(1), json!(2)),
        "a ticket counts under each of its tags"
    );
    assert_eq!(row(&run("support_volume", "Customer").await, "Acme")["tickets"], json!(2));
    assert_eq!(row(&run("support_volume", "Team").await, "Billing")["tickets"], json!(2));
    // ── reopen rate, per agent and team ──
    let reopened = run("support_reopens", "Agent").await;
    assert_eq!(
        (
            row(&reopened, "bo@acme.test")["solved"].clone(),
            row(&reopened, "bo@acme.test")["reopened"].clone(),
            row(&reopened, "bo@acme.test")["reopen_pct"].clone()
        ),
        (json!(1), json!(1), json!("100"))
    );
    assert_eq!(row(&reopened, "ann@acme.test")["reopened"], json!(0));
    assert_eq!(reopened["totals"]["reopens"], json!(1));
    assert_eq!(row(&run("support_reopens", "Team").await, "Billing")["reopens"], json!(1));
    // ── backlog by age and status ──
    let backlog = run("support_backlog", "Status").await;
    assert_eq!(
        (row(&backlog, "Open")["open"].clone(), row(&backlog, "Open")["under_1"].clone(), row(&backlog, "Open")["d7_14"].clone()),
        (json!(1), json!(0), json!(1)),
        "{backlog}"
    );
    assert_eq!(row(&backlog, "Replied")["open"], json!(1), "the answered ticket waits for its customer");
    assert_eq!(backlog["totals"]["open"], json!(2), "{backlog}");
    assert_eq!(row(&run("support_backlog", "Agent").await, "bo@acme.test")["d7_14"], json!(1));
    // ── against targets ──
    let compliance = run("support_sla_compliance", "SLA").await;
    let std = row(&compliance, "Standard");
    assert_eq!(
        (std["response_breached"].clone(), std["resolution_breached"].clone(), std["resolution_pending"].clone()),
        (json!(1), json!(1), json!(1)),
        "the aged ticket missed both; the one waiting on its customer is stopped, not late: {}",
        compliance["rows"]
    );
    let times = run("support_response_times", "Team").await;
    assert!(row(&times, "Support")["responded"].as_i64().unwrap() >= 1, "{times}");
    // ── an agent sees only their teams' tickets ──
    let ann = e.user("ann@acme.test", &["Support Agent"]).await;
    k.set_user_scopes(&e.admin, ann.user, "SupportTeam", &[sup], None).await.unwrap();
    let ann = k.load_ctx(e.tenant, ann.user, None, "it".into()).await.unwrap();
    let mine = ferrum_domain::reports::run(&k, &ann, "support_volume", &params("Team")).await.unwrap();
    assert!(mine["rows"].as_array().unwrap().iter().all(|r| r["group"] == json!("Support")), "{mine}");
    let none = ferrum_domain::reports::run(&k, &ann, "support_backlog", &params("Agent")).await.unwrap();
    assert!(none["rows"].as_array().unwrap().iter().all(|r| r["group"] == json!("ann@acme.test")), "{none}");
}
