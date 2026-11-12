//! Assignment and routing (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): rules as records choose a team and
//! an agent by turns or load, by a skill or the ticket's language, among agents who are on shift and not away
//! (the platform's away periods), with a fallback; a reopened ticket whose agent cannot take it goes round again;
//! a ticket nobody answers moves on; two tickets at once never take the same turn.

mod common;
use chrono::Utc;
use common::*;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use uuid::Uuid;

#[test]
fn routing() {
    run(scenario());
}

/// A message added the way the API does: the existing rows plus the new one.
async fn say(e: &Env, ticket: Uuid, message: Value) -> ferrum_kernel::Doc {
    let mut conversation = e.get("Ticket", ticket).await["conversation"].as_array().cloned().unwrap_or_default();
    conversation.push(message);
    e.update("Ticket", ticket, json!({ "conversation": conversation })).await
}

async fn assigned(e: &Env, v: Value) -> Value {
    e.insert("Ticket", v).await["assigned_to"].clone()
}

async fn scenario() {
    let e = boot("routing", false, vec![]).await;
    let k = e.k.clone();
    // Agents: each is a user (so away periods reach them) and a team member.
    let mut user = HashMap::new();
    for who in ["ann", "bo", "cy", "dee", "boss"] {
        let id = k.create_user(&e.admin, &format!("{who}@acme.test"), who, None, &["Support Agent".to_string()], false).await.unwrap();
        user.insert(who, id);
    }
    let member = |who: &str| json!({"agent": format!("{who}@acme.test")});
    let team = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [member("ann"), member("bo"), member("cy"), member("dee")]})).await);
    let rule = |name: &str, priority: i64, condition: &str, extra: Value| {
        let mut v = json!({"rule_name": name, "team": team.to_string(), "rule_type": "Round Robin", "priority": priority, "condition": condition});
        v.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        v
    };
    let profile = |who: &str, skills: &str| json!({"agent": format!("{who}@acme.test"), "skills": skills});
    for p in [profile("ann", "en"), profile("bo", "hi, billing"), profile("cy", "hi"), profile("dee", "en, billing")] {
        e.insert("AgentProfile", p).await;
    }

    // ── by language: only agents who speak it, in turn ──
    e.insert("TicketAssignmentRule", rule("Hindi", 30, "!isEmpty(doc.language)", json!({"needs_skill": "Ticket Language"}))).await;
    e.insert("TicketAssignmentRule", rule("Billing", 20, "doc.subject.contains('invoice')", json!({"needs_skill": "Fixed Skill", "skill": "billing"}))).await;
    e.insert("TicketAssignmentRule", rule("Everyone", 1, "", json!({}))).await;
    let mut got = Vec::new();
    for _ in 0..4 {
        got.push(assigned(&e, json!({"subject": "Help", "language": "hi-IN"})).await);
    }
    assert_eq!(
        got,
        [json!("bo@acme.test"), json!("cy@acme.test"), json!("bo@acme.test"), json!("cy@acme.test")],
        "Hindi speakers, in turn: the language's base matches hi-IN"
    );
    let invoice = assigned(&e, json!({"subject": "Wrong invoice"})).await;
    let invoice2 = assigned(&e, json!({"subject": "Another invoice"})).await;
    assert_eq!([invoice, invoice2], [json!("bo@acme.test"), json!("dee@acme.test")], "the fixed skill: billing, in its own rule's turn");
    let plain = assigned(&e, json!({"subject": "Question"})).await;
    assert!(plain.is_string(), "the rule for everyone takes the rest");

    // ── on shift and not away ──
    // A calendar that is never open: bo is off shift. cy is away (the platform's away periods, through to tomorrow).
    e.exec("insert into logic_calendar (tenant_id, name, zone, weekends) values ($1, 'closed', 'UTC', '{mon,tue,wed,thu,fri,sat,sun}')", &[]).await;
    let bo_profile = uid(&e.list("AgentProfile", json!({"agent": "bo@acme.test"})).await[0]);
    e.update("AgentProfile", bo_profile, json!({"shift_calendar": "closed"})).await;
    let today = Utc::now().date_naive();
    k.set_away(&e.admin, user["cy"], today, today + chrono::Days::new(1), user["boss"], "Leave").await.unwrap();
    let mut got = Vec::new();
    for _ in 0..4 {
        got.push(assigned(&e, json!({"subject": "Help", "language": "hi"})).await);
    }
    assert!(got.iter().all(Value::is_null), "no Hindi speaker can take it: bo is off shift and cy is away");
    let hindi = uid(&e.list("TicketAssignmentRule", json!({"rule_name": "Hindi"})).await[0]);
    e.update("TicketAssignmentRule", hindi, json!({"fallback_agent": "boss@acme.test"})).await;
    assert_eq!(assigned(&e, json!({"subject": "Help", "language": "hi"})).await, json!("boss@acme.test"), "the fallback");
    let ticket = e.insert("Ticket", json!({"subject": "Help", "language": "hi"})).await;
    assert_eq!(
        (ticket["team"].clone(), ticket["routed_by"].clone()),
        (json!(team.to_string()), json!(hindi.to_string())),
        "the team is still the rule's, and it says which rule"
    );
    // Switching the check off takes everyone.
    e.update("TicketAssignmentRule", hindi, json!({"only_available": false})).await;
    assert!(matches!(assigned(&e, json!({"subject": "Help", "language": "hi"})).await.as_str(), Some("bo@acme.test" | "cy@acme.test")));
    e.update("TicketAssignmentRule", hindi, json!({"only_available": true})).await;
    // The shift opens and the leave ends: they are back in the turns.
    e.update("AgentProfile", bo_profile, json!({"shift_calendar": ""})).await;
    e.exec("delete from person_away where tenant_id = $1", &[]).await;
    let back_in_turn = [assigned(&e, json!({"subject": "Help", "language": "hi"})).await, assigned(&e, json!({"subject": "Help", "language": "hi"})).await];
    assert_eq!(back_in_turn, [json!("cy@acme.test"), json!("bo@acme.test")], "both are back in the turns");

    // ── two at once never take the same turn ──
    let gate = Arc::new(tokio::sync::Barrier::new(6));
    let tasks: Vec<_> = (0..6)
        .map(|i| {
            let (k, admin, gate) = (k.clone(), e.admin.clone(), gate.clone());
            tokio::spawn(async move {
                gate.wait().await;
                k.insert(&admin, "Ticket", obj(json!({"subject": format!("Rush {i}"), "language": "hi"}))).await.unwrap()["assigned_to"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
        })
        .collect();
    let mut per: HashMap<String, usize> = HashMap::new();
    for t in tasks {
        *per.entry(t.await.unwrap()).or_default() += 1;
    }
    assert_eq!(per, HashMap::from([("bo@acme.test".to_string(), 3), ("cy@acme.test".to_string(), 3)]), "{per:?}");

    // ── a reopened ticket whose agent cannot take it goes round again ──
    let reopened = uid(&e
        .insert("Ticket", json!({"subject": "Wrong invoice again", "contact_email": "cust@x.test", "assigned_to": "dee@acme.test", "team": team.to_string()}))
        .await);
    e.update("Ticket", reopened, json!({"status": "Resolved", "resolution_details": "Fixed"})).await;
    k.set_away(&e.admin, user["dee"], today, today, user["boss"], "Sick").await.unwrap();
    let back = say(&e, reopened, json!({"author_kind": "Customer", "body": "It is wrong again"})).await;
    assert_eq!(back["status"], json!("Open"));
    assert_ne!(back["assigned_to"], json!("dee@acme.test"), "dee is away: it goes to someone who can take it");
    assert!(back["assigned_to"].is_string() && back["routed_by"].is_string(), "{back:?}");
    assert_eq!(back["reopen_count"], json!(1));
    // An agent who can take it keeps it.
    e.exec("delete from person_away where tenant_id = $1", &[]).await;
    let mine =
        uid(&e.insert("Ticket", json!({"subject": "Mine", "contact_email": "cust@x.test", "assigned_to": "ann@acme.test", "team": team.to_string()})).await);
    e.update("Ticket", mine, json!({"status": "Resolved", "resolution_details": "Done"})).await;
    let again = say(&e, mine, json!({"author_kind": "Customer", "body": "Hm"})).await;
    assert_eq!((again["assigned_to"].clone(), again["reopen_count"].clone()), (json!("ann@acme.test"), json!(1)), "its own agent is on shift");
    // Settings can turn it off.
    let settings = e.settings(json!({"route_reopened": false})).await;
    k.set_away(&e.admin, user["ann"], today, today, user["boss"], "Sick").await.unwrap();
    e.update("Ticket", mine, json!({"status": "Resolved", "resolution_details": "Done again"})).await;
    let kept = say(&e, mine, json!({"author_kind": "Customer", "body": "Again"})).await;
    assert_eq!(kept["assigned_to"], json!("ann@acme.test"), "routing reopened tickets is a setting");
    e.update("SupportSettings", settings, json!({"route_reopened": true})).await;
    e.exec("delete from person_away where tenant_id = $1", &[]).await;

    // ── a ticket nobody answers moves on, a limited number of times ──
    let idle = uid(&e
        .insert("Ticket", json!({"subject": "Nobody answers", "contact_email": "cust@x.test", "assigned_to": "ann@acme.test", "team": team.to_string()}))
        .await);
    let age = |hours: i64| {
        let (k, tenant) = (k.clone(), e.tenant);
        async move {
            let mut tx = k.db.tenant_tx(tenant).await.unwrap();
            sqlx::query("update e_ticket set opened_at = now() - make_interval(hours => $2), last_customer_message_at = now() - make_interval(hours => $2) where id = $1")
                .bind(idle)
                .bind(hours as i32)
                .execute(&mut *tx)
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }
    };
    age(5).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.get("Ticket", idle).await["assigned_to"], json!("ann@acme.test"), "off by default: reassignment is a setting");
    e.update("SupportSettings", settings, json!({"reassign_unanswered_hours": 4, "max_reassignments": 1})).await;
    age(3).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.get("Ticket", idle).await["assigned_to"], json!("ann@acme.test"), "not yet: unanswered for 3 of the 4 hours");
    age(5).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let moved = e.get("Ticket", idle).await;
    assert!(moved["assigned_to"].is_string() && moved["assigned_to"] != json!("ann@acme.test"), "the next agent: {}", moved["assigned_to"]);
    assert_eq!(moved["reassigned_count"], json!(1));
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.get("Ticket", idle).await["reassigned_count"], json!(1), "one move, as the setting says, though it is still unanswered");
    // Answered tickets stay put.
    let answered =
        uid(&e.insert("Ticket", json!({"subject": "Answered", "contact_email": "cust@x.test", "assigned_to": "bo@acme.test", "team": team.to_string()})).await);
    say(&e, answered, json!({"author_kind": "Agent", "author": "bo@acme.test", "body": "On it"})).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.get("Ticket", answered).await["assigned_to"], json!("bo@acme.test"));
}
