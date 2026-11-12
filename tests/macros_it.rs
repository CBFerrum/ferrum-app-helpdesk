//! Saved replies with merge fields and macros (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): a reply is
//! filled in from the ticket on the server; a macro is a record of steps (set fields, add a tag, send a reply,
//! change the status) that apply together or not at all, as the person who presses it.

mod common;
use common::*;
use ferrum_kernel::Error;
use serde_json::json;

#[test]
fn replies_and_macros() {
    run(scenario());
}

async fn scenario() {
    let e = boot("macros", true, vec![]).await;
    let k = e.k.clone();
    let sam = e.user("sam@acme.test", &["Support Agent"]).await;
    let kim = e.user("kim@acme.test", &["Support Agent"]).await;
    let ada = uid(&e.insert("Customer", json!({"customer_name": "Ada Ltd", "email": "ada@cust.test", "customer_group": e.list("CustomerGroup", json!({"group_name": "Individual"})).await[0]["id"]})).await);
    let ticket = uid(&e.insert("Ticket", json!({"subject": "Printer jam", "priority": "High", "customer": ada.to_string(), "contact_name": "Ada Lovelace", "contact_email": "ada@cust.test"})).await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "kim@acme.test"}]})).await);

    // ── saved replies, filled in on the server ──
    let reply = uid(&e.insert("SavedReply", json!({"title": "Thanks", "scope": "Everyone", "body": "Hi {{customer.first_name}},\n\nAbout {{ticket.number}} ({{ticket.priority}}): {{ticket.subject}} for {{customer.company}}.\n\n{{agent.first_name}} {{customer.nickname}}"})).await);
    let filled = e.action(&sam, "SavedReply", reply, "render", json!({"ticket": ticket.to_string()})).await.unwrap();
    let number = e.get("Ticket", ticket).await["number"].as_str().unwrap().to_string();
    assert!(filled["body"].as_str().unwrap().starts_with(&format!("Hi Ada,\n\nAbout {number} (")), "{filled}");
    let ok = uid(&e.insert("SavedReply", json!({"title": "Plain", "scope": "Everyone", "body": "Hi {{customer.first_name}}, {{ticket.subject}} / {{agent.first_name}} {{customer.nickname}}!"})).await);
    assert_eq!(
        e.action(&sam, "SavedReply", ok, "render", json!({"ticket": ticket.to_string()})).await.unwrap()["body"],
        json!("Hi Ada, Printer jam / sam@acme.test !"),
        "the agent's name is theirs; an unknown field is empty"
    );

    // ── a macro: set fields, add a tag, reply, change the status, in one click ──
    let solve = uid(&e
        .insert(
            "SupportMacro",
            json!({"macro_name": "Solve and thank", "scope": "Everyone", "steps": [
                {"step": "Set Priority", "value": "Low"}, {"step": "Set Type", "value": "Question"}, {"step": "Add Tag", "value": "Solved-By-Macro"},
                {"step": "Private Note", "value": "Macro: solved for {{customer.first_name}}"},
                {"step": "Public Reply", "value": "Hi {{customer.first_name}}, we fixed {{ticket.number}}.\n\n{{agent.first_name}}"},
                {"step": "Set Resolution", "value": "Cleared the jam"}, {"step": "Set Status", "value": "Resolved"}]}),
        )
        .await);
    let result = e.action(&sam, "Ticket", ticket, "apply_macro", json!({"macro": solve.to_string()})).await.unwrap();
    assert_eq!(result["applied"], json!(7));
    let t = e.get("Ticket", ticket).await;
    assert_eq!((t["status"].clone(), t["priority"].clone(), t["resolution_details"].clone()), (json!("Resolved"), json!("Low"), json!("Cleared the jam")));
    assert!(t["ticket_type"].is_string());
    assert_eq!(k.tags(&e.admin, "Ticket", ticket).await.unwrap(), ["solved-by-macro"]);
    let conv = t["conversation"].as_array().unwrap();
    assert_eq!(
        (conv[0]["is_internal"].clone(), conv[0]["body"].clone(), conv[1]["is_internal"].clone()),
        (json!(true), json!("Macro: solved for Ada"), json!(false))
    );
    assert_eq!(conv[1]["author"], json!("sam@acme.test"), "as the person who pressed it");
    assert!(conv[1]["body"].as_str().unwrap().starts_with(&format!("Hi Ada, we fixed {number}.")), "{}", conv[1]["body"]);
    let sent = e.list("Communication", json!({"direction": "Sent"})).await;
    assert_eq!(sent.len(), 1, "the public reply went to the customer through Mail; the note did not");
    assert!(sent[0]["content"].as_str().unwrap().contains("we fixed"));
    assert_eq!(e.get("SupportMacro", solve).await["uses"], json!(1));

    // ── all or none ──
    let other = uid(&e.insert("Ticket", json!({"subject": "Another", "priority": "High", "contact_name": "Zed", "contact_email": "zed@cust.test"})).await);
    let broken = uid(&e
        .insert("SupportMacro", json!({"macro_name": "Broken", "scope": "Everyone", "steps": [{"step": "Add Tag", "value": "half"}, {"step": "Set Priority", "value": "Low"}, {"step": "Set Team", "value": "Nowhere"}]}))
        .await);
    let r = e.action(&sam, "Ticket", other, "apply_macro", json!({"macro": broken.to_string()})).await;
    assert!(matches!(&r, Err(Error::Validation(f)) if f.iter().any(|x| x.message.contains("step 3"))), "{r:?}");
    let t = e.get("Ticket", other).await;
    assert_eq!(t["priority"], json!("High"), "nothing applied");
    assert!(k.tags(&e.admin, "Ticket", other).await.unwrap().is_empty());
    // The ticket's own rules still apply to the result: resolving needs its resolution.
    let no_resolution = uid(&e.insert("SupportMacro", json!({"macro_name": "Resolve only", "scope": "Everyone", "steps": [{"step": "Add Tag", "value": "x"}, {"step": "Set Status", "value": "Resolved"}]})).await);
    let r = e.action(&sam, "Ticket", other, "apply_macro", json!({"macro": no_resolution.to_string()})).await;
    assert!(r.is_err() && k.tags(&e.admin, "Ticket", other).await.unwrap().is_empty(), "{r:?}");
    // A step that moves it to a team and an agent.
    let move_it = uid(&e.insert("SupportMacro", json!({"macro_name": "To billing", "scope": "Everyone", "steps": [{"step": "Set Team", "value": "Billing"}, {"step": "Assign To", "value": "kim@acme.test"}, {"step": "Remove Tag", "value": "x"}]})).await);
    e.action(&sam, "Ticket", other, "apply_macro", json!({"macro": move_it.to_string()})).await.unwrap();
    let t = e.get("Ticket", other).await;
    assert_eq!((t["team"].clone(), t["assigned_to"].clone()), (json!(billing.to_string()), json!("kim@acme.test")));

    // ── whose macros: a personal one is its writer's ──
    let mine = uid(&k
        .insert(&sam, "SupportMacro", obj(json!({"macro_name": "Sam's own", "scope": "Personal", "steps": [{"step": "Add Tag", "value": "sams"}]})))
        .await
        .unwrap());
    let r = e.action(&kim, "Ticket", other, "apply_macro", json!({"macro": mine.to_string()})).await;
    assert!(r.is_err(), "someone else's personal macro is not there: {r:?}");
    e.action(&sam, "Ticket", other, "apply_macro", json!({"macro": mine.to_string()})).await.unwrap();
    assert_eq!(k.tags(&e.admin, "Ticket", other).await.unwrap(), ["sams"]);
}
