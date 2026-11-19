//! Escalation by condition and triggers on what the customer wrote (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first):
//! the customer's latest words are a field conditions read; a workspace's trigger is the platform's automation calling
//! Apply a Macro through the gate; an escalation rule may fire on a condition, once, and its event says what changed.
//! One scenario: the suites share one database and reset it.

mod common;
use common::*;
use ferrum_kernel::{Error, automation};
use serde_json::{Value, json};
use uuid::Uuid;

#[test]
fn rules_and_triggers() {
    run(scenario());
}

async fn scenario() {
    let e = boot("rules", false, vec![]).await;
    let k = e.k.clone();
    let say = |ticket: Uuid, kind: &'static str, body: &'static str, internal: bool| {
        let e = &e;
        async move {
            let mut conv = e.get("Ticket", ticket).await["conversation"].as_array().cloned().unwrap_or_default();
            conv.push(json!({"author_kind": kind, "body": body, "is_internal": internal}));
            e.update("Ticket", ticket, json!({"conversation": conv})).await
        }
    };
    let ticket = |subject: &'static str, extra: Value| {
        let e = &e;
        async move {
            let mut v =
                json!({"subject": subject, "description": "The printer jams", "priority": "Medium", "contact_email": "cy@cust.test", "contact_name": "Cy"});
            v.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
            uid(&e.insert("Ticket", v).await)
        }
    };

    // ── the_latest_customer_words_are_a_field ──
    let a = ticket("Jams", json!({})).await;
    assert_eq!(e.get("Ticket", a).await["last_customer_message"], json!("The printer jams"), "before any message, the description");
    say(a, "Customer", "I want a refund", false).await;
    say(a, "Agent", "Let me check", false).await;
    let t = say(a, "Agent", "Refund needs approval", true).await;
    assert_eq!(t["last_customer_message"], json!("I want a refund"), "the team's words are not the customer's");

    // ── a_tag_rule_reads_the_latest_message ──
    let refunds = uid(&e.insert("TicketTag", json!({"tag_name": "refunds"})).await);
    e.insert(
        "TagRule",
        json!({"rule_name": "Refunds", "tag": refunds.to_string(), "condition": "doc.last_customer_message.contains('refund')", "enabled": true}),
    )
    .await;
    let b = ticket("Another jam", json!({})).await;
    assert!(k.tags(&e.admin, "Ticket", b).await.unwrap().is_empty(), "not before");
    say(b, "Customer", "Please refund me", false).await;
    assert_eq!(k.tags(&e.admin, "Ticket", b).await.unwrap(), ["refunds"]);

    // ── an_automation_on_the_customers_words_applies_a_macro ──
    let m = uid(&e
        .insert(
            "SupportMacro",
            json!({"macro_name": "Refund desk", "scope": "Everyone", "steps": [{"step": "Set Priority", "value": "High"}, {"step": "Private Note", "value": "Refund asked"}]}),
        )
        .await);
    let auto = obj(json!({
        "name": "refund words", "entity": "Ticket", "event": "updated",
        "condition": "doc.last_customer_message_at != old.last_customer_message_at && doc.last_customer_message.contains('refund')",
        "actions": [{"type": "invoke", "capability": "helpdesk.ticket.apply_macro", "input": {"id": "doc.id", "params": format!("{{'macro': '{m}'}}")}}],
    }));
    drain(&k).await; // what was saved before the automation is not its business
    automation::save(&k, &e.admin, &auto).await.expect("an automation may call Apply a Macro");
    let c = ticket("Third", json!({})).await;
    drain(&k).await;
    say(c, "Customer", "I would like a refund", false).await;
    drain(&k).await;
    let t = e.get("Ticket", c).await;
    let notes = |t: &ferrum_kernel::Doc| t["conversation"].as_array().unwrap().iter().filter(|x| x["is_internal"] == json!(true)).count();
    assert_eq!((t["priority"].clone(), notes(&t)), (json!("High"), 1), "{t:?}");
    assert_eq!(e.get("SupportMacro", m).await["uses"], json!(1));
    drain(&k).await;
    assert_eq!(notes(&e.get("Ticket", c).await), 1, "once");
    let d = ticket("Fourth", json!({})).await;
    say(d, "Customer", "Just jammed again", false).await;
    drain(&k).await;
    assert_eq!(e.get("Ticket", d).await["priority"], json!("Medium"), "no word, no macro");
    assert_eq!(e.get("SupportMacro", m).await["uses"], json!(1));

    // ── a_condition_that_does_not_compile_is_refused ──
    let r =
        k.insert(&e.admin, "EscalationRule", obj(json!({"rule_name": "Broken", "level": 1, "trigger": "Condition", "condition": "doc.reopen_count >="}))).await;
    assert!(matches!(&r, Err(Error::Validation(f)) if f.iter().any(|x| x.field == "condition")), "{r:?}");

    // ── an_agent_cannot_save_an_escalation_rule ──
    let sam = e.user("sam@acme.test", &["Support Agent"]).await;
    let r = k.insert(&sam, "EscalationRule", obj(json!({"rule_name": "Mine", "level": 1, "trigger": "Condition", "condition": "true"}))).await;
    assert!(matches!(r, Err(Error::Forbidden(_))), "{r:?}");

    // ── a_condition_rule_fires_once ──
    let lead = e.user("lead@acme.test", &["Support Manager"]).await;
    e.insert(
        "EscalationRule",
        json!({"rule_name": "Reopened", "level": 1, "trigger": "Condition", "condition": "doc.reopen_count >= 1", "assign_to": "lead@acme.test"}),
    )
    .await;
    let f = ticket("Comes back", json!({})).await;
    e.update("Ticket", f, json!({"status": "Resolved", "resolution_details": "Cleaned"})).await;
    say(f, "Customer", "It jams again", false).await;
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    let t = e.get("Ticket", f).await;
    assert_eq!((t["escalation_level"].clone(), t["assigned_to"].clone()), (json!(1), json!("lead@acme.test")));
    let told: Option<i64> = e.sql("select count(*) from notification where tenant_id = $1 and user_id = $2 and title like 'Escalated:%'", &[lead.user]).await;
    assert_eq!(told, Some(1), "once");

    // ── an_escalation_says_what_changed ──
    let was: Option<Value> = e
        .sql(
            "select payload->'was' from outbox where tenant_id = $1 and record_id = $2 and topic = 'Ticket.updated' and payload->'was' ? 'escalation_level'",
            &[f],
        )
        .await;
    assert_eq!(was.as_ref().map(|w| w["escalation_level"].clone()), Some(json!(0)), "{was:?}");
    let g = ticket("Escalates later", json!({})).await;
    e.update("Ticket", g, json!({"status": "Resolved", "resolution_details": "Cleaned"})).await;
    drain(&k).await;
    automation::save(
        &k,
        &e.admin,
        &obj(json!({"name": "escalated", "entity": "Ticket", "event": "updated", "condition": "doc.escalation_level > old.escalation_level",
                    "actions": [{"type": "set_fields", "values": {"language": "'esc'"}}]})),
    )
    .await
    .expect("an automation on the escalation");
    say(g, "Customer", "Back again", false).await;
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    drain(&k).await;
    assert_eq!(e.get("Ticket", g).await["language"], json!("esc"), "the automation saw the escalation");

    // ── a_condition_narrows_an_sla_rule ──
    e.insert("ServiceLevelAgreement", json!({"sla_name": "Standard", "priorities": [{"priority": "Low", "response_hours": "1", "resolution_hours": "4"}]}))
        .await;
    e.insert(
        "EscalationRule",
        json!({"rule_name": "Hindi breach", "level": 2, "trigger": "Breached", "condition": "doc.language == 'hi'", "notify": "lead@acme.test"}),
    )
    .await;
    let en = ticket("English", json!({"priority": "Low", "language": "en"})).await;
    let hi = ticket("Hindi", json!({"priority": "Low", "language": "hi"})).await;
    e.exec("update record_target set due_at = now() - interval '1 hour' where tenant_id = $1 and record_id in ($2, $3)", &[en, hi]).await;
    k.sweep_targets_at(chrono::Utc::now()).await.unwrap();
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    let (en, hi) = (e.get("Ticket", en).await, e.get("Ticket", hi).await);
    assert_eq!((en["sla_status"].clone(), en["escalation_level"].clone()), (json!("Breached"), json!(0)), "another language");
    assert_eq!(hi["escalation_level"], json!(2));
}
