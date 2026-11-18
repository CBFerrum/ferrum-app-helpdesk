//! A workspace's own statuses (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): each sits inside one of four working
//! stages; choosing one moves the ticket to its stage, a change of stage that no longer fits takes it off, and the
//! queues, clocks and reports go on reading the stage. One scenario: the suites share one database and reset it.

mod common;
use common::*;
use ferrum_kernel::Error;
use serde_json::{Value, json};
use std::collections::HashMap;
use uuid::Uuid;

#[test]
fn workspace_statuses() {
    run(scenario());
}

fn on(r: &ferrum_kernel::Result<impl std::fmt::Debug>, field: &str) -> bool {
    matches!(r, Err(Error::Validation(f)) if f.iter().any(|x| x.field == field))
}

async fn scenario() {
    let e = boot("statuses", false, vec![]).await;
    let k = e.k.clone();
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "priorities": [{"priority": "Medium", "response_hours": "4", "resolution_hours": "16"}]}))
        .await;
    let supplier = uid(&e.insert("TicketStatus", json!({"status_name": "Waiting on supplier", "category": "On Hold"})).await);
    let fixed = uid(&e.insert("TicketStatus", json!({"status_name": "Fixed in next release", "category": "Resolved"})).await);
    let ticket = |subject: &'static str| {
        let e = &e;
        async move { uid(&e.insert("Ticket", json!({"subject": subject, "priority": "Medium", "contact_email": "cy@cust.test", "contact_name": "Cy"})).await) }
    };

    // ── a_custom_status_moves_the_ticket_to_its_stage ──
    let a = ticket("Part on order").await;
    let t = e.update("Ticket", a, json!({"custom_status": supplier.to_string()})).await;
    assert_eq!((t["status"].clone(), t["custom_status"].clone()), (json!("On Hold"), json!(supplier.to_string())), "the stage follows the status");
    assert_eq!(t["resolution_state"], json!("paused"), "the clock reads the stage: On Hold pauses it");

    // ── a_change_of_stage_takes_the_status_off ──
    let t = e.update("Ticket", a, json!({"conversation": [{"author_kind": "Customer", "body": "Any news?"}]})).await;
    assert_eq!(
        (t["status"].clone(), t["custom_status"].clone()),
        (json!("Open"), Value::Null),
        "a customer reply reopens it; the On Hold status no longer fits"
    );

    // ── a_resolved_status_survives_closing ──
    let b = ticket("Crash on export").await;
    let t = e.update("Ticket", b, json!({"custom_status": fixed.to_string(), "resolution_details": "Fixed in 2.4"})).await;
    assert_eq!(t["status"], json!("Resolved"));
    e.exec("update e_ticket set resolved_at = resolved_at - interval '8 days' where tenant_id = $1 and id = $2", &[b]).await;
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.expect("upkeep");
    let t = e.get("Ticket", b).await;
    assert_eq!((t["status"].clone(), t["custom_status"].clone()), (json!("Closed"), json!(fixed.to_string())), "closing keeps how it was resolved");

    // ── a_status_switched_off_or_unknown_is_refused ──
    let c = ticket("Unchanged").await;
    let off = uid(&e.insert("TicketStatus", json!({"status_name": "Old bucket", "category": "Open", "enabled": false})).await);
    let before = e.get("Ticket", c).await;
    for wrong in [off, Uuid::new_v4()] {
        let r = k.update(&e.admin, "Ticket", c, obj(json!({"custom_status": wrong.to_string()})), None).await;
        assert!(on(&r, "custom_status"), "{r:?}");
    }
    let after = e.get("Ticket", c).await;
    assert_eq!((after["version"].clone(), after["status"].clone(), after["custom_status"].clone()), (before["version"].clone(), json!("Open"), Value::Null));

    // ── a_stage_name_is_not_a_status ──
    let r = k.insert(&e.admin, "TicketStatus", obj(json!({"status_name": "open", "category": "Open"}))).await;
    assert!(on(&r, "status_name"), "{r:?}");

    // ── a_status_in_use_keeps_its_stage ──
    let r = k.update(&e.admin, "TicketStatus", fixed, obj(json!({"category": "Open"})), None).await;
    assert!(on(&r, "category"), "a closed ticket has it: {r:?}");
    assert!(k.delete(&e.admin, "TicketStatus", fixed, None).await.is_err(), "a ticket has it");
    let renamed = e.update("TicketStatus", fixed, json!({"status_name": "Fixed in a release"})).await;
    assert_eq!(renamed["status_name"], json!("Fixed in a release"));
    e.update("TicketStatus", off, json!({"category": "Replied"})).await; // no ticket has it

    // ── a_macro_sets_a_workspace_status ──
    let sam = e.user("sam@acme.test", &["Support Agent"]).await;
    let m = uid(&e
        .insert("SupportMacro", json!({"macro_name": "Park it", "scope": "Everyone", "steps": [{"step": "Set Status", "value": "Waiting on supplier"}]}))
        .await);
    let d = ticket("Needs a part").await;
    e.action(&sam, "Ticket", d, "apply_macro", json!({"macro": m.to_string()})).await.expect("the macro");
    let t = e.get("Ticket", d).await;
    assert_eq!((t["status"].clone(), t["custom_status"].clone()), (json!("On Hold"), json!(supplier.to_string())));
    let gone =
        uid(&e.insert("SupportMacro", json!({"macro_name": "Gone", "scope": "Everyone", "steps": [{"step": "Set Status", "value": "Old bucket"}]})).await);
    let r = e.action(&sam, "Ticket", c, "apply_macro", json!({"macro": gone.to_string()})).await;
    assert!(on(&r, "macro"), "a switched-off status is no step: {r:?}");

    // ── an_agent_cannot_make_a_status ──
    let r = k.insert(&sam, "TicketStatus", obj(json!({"status_name": "Mine", "category": "Open"}))).await;
    assert!(matches!(r, Err(Error::Forbidden(_))), "{r:?}");
    let q = ferrum_kernel::ListQuery { limit: Some(10), ..Default::default() };
    assert_eq!(k.list(&sam, "TicketStatus", &q).await.expect("agents read them").rows.len(), 3);

    // ── an_existing_ticket_is_untouched ──
    let t = e.update("Ticket", c, json!({"conversation": [{"author_kind": "Agent", "author": "admin@statuses.test", "body": "Looking"}]})).await;
    assert_eq!((t["status"].clone(), t["custom_status"].clone()), (json!("Replied"), Value::Null), "the status flow as before");

    // ── reports_group_by_workspace_status ──
    let today = chrono::Utc::now().date_naive();
    let p: HashMap<String, String> =
        [("from_date", (today - chrono::Duration::days(30)).to_string()), ("to_date", today.to_string()), ("group_by", "Workspace Status".into())]
            .into_iter()
            .map(|(a, b)| (a.to_string(), b))
            .collect();
    let r = ferrum_domain::reports::run(&k, &e.admin, "support_ticket_summary", &p).await.expect("the report");
    let rows = r["rows"].as_array().unwrap();
    let group = |g: &str| rows.iter().find(|x| x["group"] == json!(g)).map(|x| x["tickets"].clone());
    assert_eq!(group("Waiting on supplier"), Some(json!(1)), "{r}");
    assert_eq!(group("Fixed in a release"), Some(json!(1)), "{r}");
    assert_eq!(group("Replied"), Some(json!(1)), "no status of its own: the stage, {r}");
}
