//! Article suggestions for agents (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): `suggest_articles` on a ticket
//! offers the published articles that fit it, Internal ones included, as far as the caller may read articles; it is a
//! query that changes nothing. One scenario: the suites share one database and reset it.

mod common;
use common::*;
use ferrum_kernel::capability::Invocation;
use serde_json::{Value, json};

#[test]
fn suggestions_while_replying() {
    run(scenario());
}

fn titles(r: &Value) -> Vec<String> {
    r["articles"].as_array().unwrap_or_else(|| panic!("{r}")).iter().map(|a| a["title"].as_str().unwrap().to_string()).collect()
}

async fn scenario() {
    let e = boot("suggest", false, vec![]).await;
    let k = e.k.clone();
    let category = uid(&e.list("KbCategory", json!({"category_name": "General"})).await[0]);
    let article = |title: &'static str, visibility: &'static str, status: &'static str, content: &'static str| {
        let e = &e;
        async move {
            e.insert("KbArticle", json!({"title": title, "category": category.to_string(), "visibility": visibility, "status": status, "content": content, "summary": format!("About {title}")}))
                .await
        }
    };
    let jam = article("Clearing a printer jam", "Internal", "Published", "Open the rear tray.").await;
    article("Warranty terms", "Customers Only", "Published", "Routers are covered for a year.").await;
    article("Printer drafts", "Public", "Draft", "Printer notes not yet out.").await;
    article("Old printer guide", "Public", "Archived", "Printer models we no longer sell.").await;
    let sam = e.user("sam@acme.test", &["Support Agent"]).await;
    let ticket = uid(&e.insert("Ticket", json!({"subject": "Printer jam", "contact_email": "cy@cust.test", "contact_name": "Cy"})).await);

    // ── an_agent_is_offered_what_fits_internal_ones_too ──
    let r = e.action(&sam, "Ticket", ticket, "suggest_articles", json!({})).await.expect("suggestions");
    assert_eq!(titles(&r).first().map(String::as_str), Some("Clearing a printer jam"), "{r}");
    assert_eq!(r["articles"][0]["visibility"], json!("Internal"));

    // ── drafts_and_archived_articles_are_never_offered ──
    let all = titles(&e.action(&sam, "Ticket", ticket, "suggest_articles", json!({"q": "printer"})).await.unwrap());
    assert!(!all.iter().any(|t| t == "Printer drafts" || t == "Old printer guide"), "{all:?}");

    // ── words_given_are_used_instead_of_the_tickets ──
    let r = titles(&e.action(&sam, "Ticket", ticket, "suggest_articles", json!({"q": "warranty"})).await.unwrap());
    assert_eq!(r.first().map(String::as_str), Some("Warranty terms"), "{r:?}");
    assert!(!r.contains(&"Clearing a printer jam".to_string()), "{r:?}");

    // ── the_customers_latest_message_counts ──
    let other = uid(&e.insert("Ticket", json!({"subject": "A question", "contact_email": "cy@cust.test", "contact_name": "Cy"})).await);
    e.update("Ticket", other, json!({"conversation": [{"author_kind": "Customer", "body": "Is my router under warranty?"}]})).await;
    let r = titles(&e.action(&sam, "Ticket", other, "suggest_articles", json!({})).await.unwrap());
    assert_eq!(r.first().map(String::as_str), Some("Warranty terms"), "{r:?}");

    // ── suggesting_changes_nothing ──
    let (a_before, t_before) = (e.get("KbArticle", uid(&jam)).await, e.get("Ticket", ticket).await);
    let cap = k.capability(&sam, "helpdesk.ticket.suggest_articles").expect("a capability");
    assert_eq!(serde_json::to_value(cap.effect).unwrap(), json!("query"));
    let receipt = k.invoke(&sam, Invocation::new("helpdesk.ticket.suggest_articles", json!({"id": ticket, "params": {}}))).await.expect("no key needed");
    assert!(!titles(&receipt.result).is_empty());
    let (a_after, t_after) = (e.get("KbArticle", uid(&jam)).await, e.get("Ticket", ticket).await);
    assert_eq!((a_after["views"].clone(), a_after["version"].clone()), (a_before["views"].clone(), a_before["version"].clone()), "no view counted");
    assert_eq!(t_after["version"], t_before["version"]);
    let long = "x".repeat(201);
    assert!(e.action(&sam, "Ticket", ticket, "suggest_articles", json!({"q": long})).await.is_err(), "at most 200 characters");

    // ── someone_who_may_not_read_articles_is_offered_none ──
    let read_tickets: ferrum_meta::PermissionDef = serde_json::from_value(json!({"entity": "Ticket", "read": true})).unwrap();
    k.set_role_permission(&e.admin, "Ticket Reader", &read_tickets).await.expect("a role that reads tickets only");
    let reader = e.user("ray@acme.test", &["Ticket Reader"]).await;
    let r = e.action(&reader, "Ticket", ticket, "suggest_articles", json!({})).await.expect("a reader of the ticket");
    assert_eq!(r["articles"], json!([]));

    // ── someone_who_may_not_read_the_ticket_is_refused ──
    let cy = e.portal_user("cy@cust.test").await;
    assert!(e.action(&cy, "Ticket", ticket, "suggest_articles", json!({})).await.is_err(), "a portal person");
    let front = uid(&e.insert("SupportTeam", json!({"team_name": "Front", "members": [{"agent": "fay@acme.test"}]})).await);
    let back = uid(&e.insert("SupportTeam", json!({"team_name": "Back", "members": [{"agent": "bo@acme.test"}]})).await);
    let theirs = uid(&e.insert("Ticket", json!({"subject": "Printer", "team": back.to_string(), "contact_email": "cy@cust.test"})).await);
    let fay = e.user("fay@acme.test", &["Support Agent"]).await;
    k.set_user_scopes(&e.admin, fay.user, "SupportTeam", &[front], None).await.unwrap();
    let fay = k.load_ctx(e.tenant, fay.user, None, "it".into()).await.unwrap();
    assert!(e.action(&fay, "Ticket", theirs, "suggest_articles", json!({})).await.is_err(), "outside her teams");
}
