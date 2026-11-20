//! The reports a service desk lives by (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first), read from the tickets' own fields and
//! the platform's target states: first response and resolution against their targets, the backlog by age, volume by channel,
//! tag and customer, the reopen rate, each by agent and team, and within an agent's record scopes.

mod common;
use chrono::{Duration, Utc};
use common::*;
use ferrum_kernel::{Ctx, Kernel};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;

#[test]
fn reports() {
    run(scenario());
}

fn row<'a>(r: &'a Value, group: &str) -> &'a Value {
    r["rows"].as_array().unwrap().iter().find(|x| x["group"] == json!(group)).unwrap_or_else(|| panic!("{group} in {r}"))
}

/// A role of the test's own that reads tickets without their priority, team, customer name or first-response hours.
const AUDIT: &str = "package: helpdesk_audit\nversion: 0.1.0\ndepends: [helpdesk]\nroles:\n  - name: Desk Auditor\n    permissions:\n      \
- {entity: Ticket, read: true, hide: [priority, team, customer_name, first_response_hours]}\n      - {entity: SupportTeam, read: true}\n      \
- {entity: TicketType, read: true}\n      - {entity: TicketStatus, read: true}\n      - {entity: ServiceLevelAgreement, read: true}\n";

async fn scenario() {
    let audit = ferrum_meta::catalog::load_package_files(&[("package.yaml".to_string(), AUDIT.to_string())].into()).expect("the test's role");
    let e = boot("reports", false, vec![audit]).await;
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
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();

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

    // ── what a caller cannot read, a report neither groups by, filters by nor shows (S9 K0-7) ──
    k.install_packages(e.tenant, &["helpdesk_audit".to_string()]).await.unwrap();
    let auditor = e.user("audit@acme.test", &["Desk Auditor"]).await;
    let agent = e.user("kim@acme.test", &["Support Agent"]).await;
    // Each section on its own, so that each one's failure shows.
    let mut failed = Vec::new();
    for (name, section) in [
        (
            "a_report_refuses_grouping_by_a_field_the_caller_cannot_read",
            tokio::spawn(a_report_refuses_grouping_by_a_field_the_caller_cannot_read(k.clone(), auditor.clone(), agent.clone(), sup.to_string())),
        ),
        (
            "a_report_s_default_falls_back_to_a_readable_grouping",
            tokio::spawn(a_report_s_default_falls_back_to_a_readable_grouping(k.clone(), auditor.clone(), agent.clone())),
        ),
        (
            "a_report_leaves_out_columns_the_caller_cannot_read",
            tokio::spawn(a_report_leaves_out_columns_the_caller_cannot_read(k.clone(), auditor.clone(), agent.clone())),
        ),
    ] {
        if let Err(err) = section.await {
            failed.push(name);
            eprintln!("{name} failed: {err:?}");
        }
    }
    assert!(failed.is_empty(), "{failed:?}");
}

fn given(pairs: &[(&str, &str)]) -> HashMap<String, String> {
    let today = Utc::now().date_naive();
    let mut p: HashMap<String, String> =
        [("from_date", (today - Duration::days(60)).to_string()), ("to_date", today.to_string())].into_iter().map(|(a, b)| (a.to_string(), b)).collect();
    p.extend(pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())));
    p
}

async fn report(k: &Kernel, who: &Ctx, name: &str, pairs: &[(&str, &str)]) -> ferrum_kernel::Result<Value> {
    ferrum_domain::reports::run(k, who, name, &given(pairs)).await
}

fn names(r: &Value) -> Vec<&str> {
    r["columns"].as_array().unwrap().iter().filter_map(|c| c["name"].as_str()).collect()
}

async fn a_report_refuses_grouping_by_a_field_the_caller_cannot_read(k: Arc<Kernel>, auditor: Ctx, agent: Ctx, team: String) {
    for (name, pairs) in [
        ("support_ticket_summary", vec![("group_by", "Priority")]),
        ("support_volume", vec![("group_by", "Customer")]),
        ("support_backlog", vec![("group_by", "Team")]),
        ("support_time_spent", vec![("group_by", "Customer")]),
        ("support_agent_workload", vec![("team", team.as_str())]),
    ] {
        let r = report(&k, &auditor, name, &pairs).await;
        assert!(matches!(r, Err(ferrum_kernel::Error::Forbidden(_))), "{name} {pairs:?} for a caller who cannot read it: {r:?}");
        report(&k, &agent, name, &pairs).await.unwrap_or_else(|err| panic!("{name} {pairs:?} for the desk: {err:?}"));
    }
    report(&k, &auditor, "support_ticket_summary", &[("group_by", "Status")]).await.expect("a readable grouping");
}

async fn a_report_s_default_falls_back_to_a_readable_grouping(k: Arc<Kernel>, auditor: Ctx, agent: Ctx) {
    let theirs = report(&k, &auditor, "support_response_times", &[]).await.expect("the times without a grouping chosen");
    assert_eq!(theirs["meta"]["group_by"], json!("Status"), "Priority, the default, is hidden from them: the first readable grouping");
    let desk = report(&k, &agent, "support_response_times", &[]).await.unwrap();
    assert_eq!(desk["meta"]["group_by"], json!("Priority"));
}

async fn a_report_leaves_out_columns_the_caller_cannot_read(k: Arc<Kernel>, auditor: Ctx, agent: Ctx) {
    let response = ["responded", "response_avg", "response_median", "response_p90"];
    let theirs = report(&k, &auditor, "support_response_times", &[("group_by", "Status")]).await.unwrap();
    assert!(names(&theirs).iter().all(|c| !response.contains(c)) && names(&theirs).contains(&"resolution_avg"), "{theirs}");
    for r in theirs["rows"].as_array().unwrap().iter().chain([&theirs["totals"]]) {
        assert!(response.iter().all(|c| r.get(*c).is_none()), "no value of a column left out: {r}");
    }
    let desk = report(&k, &agent, "support_response_times", &[("group_by", "Status")]).await.unwrap();
    assert!(response.iter().all(|c| names(&desk).contains(c)), "{desk}");

    let work = report(&k, &auditor, "support_agent_workload", &[]).await.unwrap();
    assert!(!names(&work).contains(&"urgent") && !names(&work).contains(&"high") && names(&work).contains(&"open"), "{work}");
    assert!(work["rows"].as_array().unwrap().iter().chain([&work["totals"]]).all(|r| r.get("urgent").is_none() && r.get("high").is_none()), "{work}");
    assert!(names(&report(&k, &agent, "support_agent_workload", &[]).await.unwrap()).contains(&"urgent"));
}
