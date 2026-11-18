//! Every action says what done means (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): each of the 14 is a
//! capability with its own completion and result shape, a repeat under one key does it once, a change without a
//! key or without the right is refused.

mod common;
use common::*;
use ferrum_kernel::capability::Invocation;
use serde_json::json;

const IDS: [&str; 14] = [
    "helpdesk.ticket.merge_tickets",
    "helpdesk.ticket.apply_macro",
    "helpdesk.ticket.rate",
    "helpdesk.ticket.make_warranty_claim",
    "helpdesk.ticket.approve_outside_entitlement",
    "helpdesk.ticket.decline_outside_entitlement",
    "helpdesk.saved_reply.used",
    "helpdesk.saved_reply.render",
    "helpdesk.kb_article.mark_helpful",
    "helpdesk.kb_article.mark_not_helpful",
    "helpdesk.warranty_claim.make_maintenance_visit",
    "helpdesk.maintenance_schedule.make_visit",
    "helpdesk.inbound_log.release",
    "helpdesk.inbound_log.discard",
];

async fn every_action_says_what_done_means(e: &Env) {
    {
        for id in IDS {
            let c = e.k.capability(&e.admin, id).unwrap_or_else(|x| panic!("{id}: {x:?}"));
            assert!(!c.completion.ends_with("is done, committed with this receipt") && !c.completion.starts_with("What "), "{id}: {}", c.completion);
            assert!(c.output["properties"].is_object(), "{id}: {}", c.output);
        }
    }
}

async fn make_visit_declares_the_planned_visit(e: &Env) {
    {
        let c = e.k.capability(&e.admin, "helpdesk.maintenance_schedule.make_visit").unwrap();
        assert!(c.input["properties"]["params"]["properties"]["schedule_detail"].is_object(), "{}", c.input);
    }
}

async fn ticket_and_macro(e: &Env, name: &str) -> (uuid::Uuid, uuid::Uuid) {
    let ticket = uid(&e.insert("Ticket", json!({"subject": "Printer jam", "contact_name": "Ada", "contact_email": "ada@cust.test"})).await);
    let m = uid(&e.insert("SupportMacro", json!({"macro_name": name, "scope": "Everyone", "steps": [{"step": "Private Note", "value": "Seen"}]})).await);
    (ticket, m)
}

async fn render_is_a_query(e: &Env) {
    {
        let c = e.k.capability(&e.admin, "helpdesk.saved_reply.render").unwrap();
        assert_eq!(serde_json::to_value(c.effect).unwrap(), json!("query"));
        let (ticket, _) = ticket_and_macro(e, "Note 1").await;
        let reply = uid(&e.insert("SavedReply", json!({"title": "Hi", "scope": "Everyone", "body": "About {{ticket.subject}}"})).await);
        let r =
            e.k.invoke(&e.admin, Invocation::new("helpdesk.saved_reply.render", json!({"id": reply, "params": {"ticket": ticket.to_string()}}))).await.unwrap();
        assert_eq!(r.result["body"], json!("About Printer jam"));
    }
}

async fn apply_macro_repeats_under_one_key(e: &Env) {
    {
        let agent = e.user("sam1@acme.test", &["Support Agent"]).await;
        let (ticket, m) = ticket_and_macro(e, "Note 2").await;
        let call = || Invocation::new("helpdesk.ticket.apply_macro", json!({"id": ticket, "params": {"macro": m.to_string()}})).key("m1");
        let first = e.k.invoke(&agent, call()).await.unwrap();
        assert!(!first.repeated);
        let second = e.k.invoke(&agent, call()).await.unwrap();
        assert!(second.repeated);
        assert_eq!(e.get("SupportMacro", m).await["uses"], json!(1));
        assert_eq!(e.get("Ticket", ticket).await["conversation"].as_array().unwrap().len(), 1);
    }
}

async fn a_change_without_a_key_is_refused(e: &Env) {
    {
        let agent = e.user("sam2@acme.test", &["Support Agent"]).await;
        let (ticket, m) = ticket_and_macro(e, "Note 3").await;
        let r = e.k.invoke(&agent, Invocation::new("helpdesk.ticket.apply_macro", json!({"id": ticket, "params": {"macro": m.to_string()}}))).await;
        assert!(r.is_err());
        assert_eq!(e.get("SupportMacro", m).await["uses"], json!(0));
    }
}

async fn someone_without_the_right_is_refused(e: &Env) {
    {
        let kb = e.user("kb@acme.test", &["Knowledge Base Editor"]).await;
        let (ticket, _) = ticket_and_macro(e, "Note 4").await;
        let other = uid(&e.insert("Ticket", json!({"subject": "Same jam", "contact_name": "Ada", "contact_email": "ada@cust.test"})).await);
        let number = e.get("Ticket", other).await["number"].as_str().unwrap().to_string();
        let r = e.k.invoke(&kb, Invocation::new("helpdesk.ticket.merge_tickets", json!({"id": ticket, "params": {"numbers": number}})).key("x1")).await;
        assert!(r.is_err(), "{r:?}");
        assert_eq!(e.get("Ticket", other).await["status"], json!("Open"));
    }
}

#[test]
fn every_action_states_its_contract() {
    run(async {
        let e = boot("contracts", false, vec![]).await;
        every_action_says_what_done_means(&e).await;
        make_visit_declares_the_planned_visit(&e).await;
        render_is_a_query(&e).await;
        apply_macro_repeats_under_one_key(&e).await;
        a_change_without_a_key_is_refused(&e).await;
        someone_without_the_right_is_refused(&e).await;
    });
}
