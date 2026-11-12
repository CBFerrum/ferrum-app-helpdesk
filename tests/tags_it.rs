//! Tags and private notes (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): tags are records with a colour,
//! put on tickets by rules (never taken off by one); a note kept private is never mailed to the customer or
//! copied, and a customer in the portal never sees it, nor can they read the ticket any other way.

mod common;
use common::*;
use ferrum_kernel::{Error, ListQuery};
use serde_json::json;

#[test]
fn tags_and_private_notes() {
    run(scenario());
}

async fn scenario() {
    let e = boot("tags", true, vec![]).await;
    let k = e.k.clone();

    // ── tags are records ──
    let vip = e.insert("TicketTag", json!({"tag_name": " VIP ", "colour": "#7c3aed"})).await;
    assert_eq!((vip["tag_name"].clone(), vip["colour"].clone()), (json!("vip"), json!("#7c3aed")), "a tag is a lowercase word");
    assert!(k.insert(&e.admin, "TicketTag", obj(json!({"tag_name": "two words"}))).await.is_err());
    assert!(k.insert(&e.admin, "TicketTag", obj(json!({"tag_name": "blue", "colour": "blue"}))).await.is_err(), "a colour is #rrggbb");
    let urgent = uid(&e.insert("TicketTag", json!({"tag_name": "urgent", "colour": "#dc2626"})).await);
    let refunds = uid(&e.insert("TicketTag", json!({"tag_name": "refunds"})).await);
    let off = uid(&e.insert("TicketTag", json!({"tag_name": "never"})).await);
    // ── rules put them on ──
    e.insert("TagRule", json!({"rule_name": "Urgent ones", "tag": urgent.to_string(), "condition": "doc.priority == 'Urgent'"})).await;
    e.insert(
        "TagRule",
        json!({"rule_name": "Refund words", "tag": refunds.to_string(), "condition": "doc.subject.contains('refund') || doc.description.contains('refund')"}),
    )
    .await;
    e.insert("TagRule", json!({"rule_name": "Off", "tag": off.to_string(), "condition": "true", "enabled": false})).await;
    assert!(
        k.insert(&e.admin, "TagRule", obj(json!({"rule_name": "Bad", "tag": urgent.to_string(), "condition": "doc.priority =="}))).await.is_err(),
        "a condition that does not compile is refused"
    );

    let calm = uid(&e.insert("Ticket", json!({"subject": "Question", "priority": "Low"})).await);
    let loud = uid(&e.insert("Ticket", json!({"subject": "Please refund me", "priority": "Urgent"})).await);
    assert!(k.tags(&e.admin, "Ticket", calm).await.unwrap().is_empty());
    let mut got = k.tags(&e.admin, "Ticket", loud).await.unwrap();
    got.sort();
    assert_eq!(got, ["refunds", "urgent"], "the rules' tags, and not the disabled rule's");
    // A rule never takes a tag off, and a person's stay.
    k.set_tags(&e.admin, "Ticket", loud, &["vip".into()]).await.unwrap();
    e.update("Ticket", loud, json!({"priority": "Low"})).await;
    let mut got = k.tags(&e.admin, "Ticket", loud).await.unwrap();
    got.sort();
    assert_eq!(got, ["refunds", "vip"], "the person's tag stays; the rule whose condition still holds puts its own back, and Urgent no longer holds");
    e.update("Ticket", loud, json!({"priority": "Urgent"})).await;
    let mut got = k.tags(&e.admin, "Ticket", loud).await.unwrap();
    got.sort();
    assert_eq!(got, ["refunds", "urgent", "vip"]);
    let by_tag = k
        .list(&e.admin, "Ticket", &ListQuery { filters: ferrum_kernel::query::parse_filters(&json!([["_tag", "=", "urgent"]])).unwrap(), ..Default::default() })
        .await
        .unwrap();
    assert_eq!(by_tag.rows.len(), 1, "queues filter by tag");

    // ── a private note: not mailed, not copied, not in the portal ──
    let agent = e.user("sam@acme.test", &["Support Agent"]).await;
    let pia = e.portal_user("pia@garage.test").await;
    let ticket = uid(&k
        .insert(
            &e.admin,
            "Ticket",
            obj(json!({"subject": "Brakes squeak", "contact_email": "pia@garage.test", "contact_name": "Pia", "cc": "boss@garage.test"})),
        )
        .await
        .unwrap());
    let mut conv = e.get("Ticket", ticket).await["conversation"].as_array().cloned().unwrap_or_default();
    conv.push(json!({"author_kind": "Agent", "body": "Public: we are on it.", "is_internal": false}));
    conv.push(json!({"author_kind": "Agent", "body": "PRIVATE: she is a difficult customer.", "is_internal": true}));
    let file = k.store_file(&agent, None, "internal.txt", "text/plain", b"secret notes").await.unwrap();
    conv.push(json!({"author_kind": "Agent", "body": "PRIVATE with a file.", "is_internal": true, "attachments": [{"id": file["id"]}]}));
    k.update(&agent, "Ticket", ticket, obj(json!({"conversation": conv})), None).await.unwrap();
    let sent = e.list("Communication", json!({"direction": "Sent"})).await;
    assert_eq!(sent.len(), 1, "one mail: the public reply");
    let text = format!("{} {} {}", sent[0]["content"], sent[0]["text_content"], sent[0]["cc"]);
    assert!(text.contains("we are on it") && !text.contains("PRIVATE") && !text.contains("difficult") && !text.contains("internal.txt"), "{text}");
    assert_eq!(sent[0]["cc"], json!("boss@garage.test"), "copies get the public reply only");
    // The portal person: the public words, none of the private ones, not the file, and no other way in.
    let view = call(&k, Some(&pia), e.tenant, "GET", &format!("portal/tickets/{ticket}"), serde_json::Value::Null).await.unwrap();
    let words = view.to_string();
    assert!(words.contains("we are on it") && !words.contains("PRIVATE") && !words.contains("difficult"), "{words}");
    assert_eq!(view["messages"].as_array().unwrap().len(), 1);
    let stored = e.get("Ticket", ticket).await;
    let private_file = stored["conversation"][2]["attachments"][0]["id"].as_str().unwrap().to_string();
    let site = k.hooks.site("helpdesk").unwrap().clone();
    let download = site
        .handle(
            &k,
            ferrum_kernel::SiteRequest {
                tenant: e.tenant,
                user: Some(pia.clone()),
                method: "GET".into(),
                path: format!("/portal/tickets/{ticket}/files/{private_file}"),
                query: Default::default(),
                headers: Default::default(),
                body: vec![],
                client_ip: "203.0.113.9".into(),
            },
        )
        .await;
    assert!(matches!(download, Err(Error::NotFound(_))), "a file on a private note is not there for them: {download:?}");
    assert!(k.get(&pia, "Ticket", ticket, false).await.is_err(), "the platform's own API is closed to them: they hold no role");
    assert!(k.list(&pia, "Ticket", &ListQuery::default()).await.is_err());
    assert!(k.tags(&pia, "Ticket", ticket).await.is_err(), "nor the tags");
}
