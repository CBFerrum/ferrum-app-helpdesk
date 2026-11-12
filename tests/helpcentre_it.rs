//! The help centre (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): articles have a visibility and an address;
//! Public, Published ones are the platform's published list with a page each and search; customers in the portal search,
//! are offered articles when they start a request, and their views and "was this helpful" are counted.

mod common;
use common::*;
use ferrum_kernel::published::Find;
use serde_json::{Value, json};

#[test]
fn the_help_centre() {
    run(scenario());
}

async fn scenario() {
    let e = boot("help", false, vec![]).await;
    let k = e.k.clone();
    let category = uid(&e.list("KbCategory", json!({"category_name": "General"})).await[0]);
    let article = |title: &'static str, visibility: &'static str, status: &'static str, content: &'static str| {
        let e = &e;
        async move {
            e.insert("KbArticle", json!({"title": title, "category": category.to_string(), "visibility": visibility, "status": status, "content": content, "summary": format!("About {title}")})).await
        }
    };
    let reset = article("How to reset your password", "Public", "Published", "Open Settings, choose Security and press Reset.").await;
    let reset_again = article("How to reset your password", "Public", "Published", "A second article with the same title.").await;
    let customers = article("Warranty terms", "Customers Only", "Published", "Routers are covered for a year after delivery.").await;
    let internal = article("Escalation playbook", "Internal", "Published", "Call the lead about the jam.").await;
    let draft = article("Printer jams", "Public", "Draft", "Open the rear tray to clear a printer jam.").await;
    assert_eq!(
        (reset["slug"].clone(), reset_again["slug"].clone()),
        (json!("how-to-reset-your-password"), json!("how-to-reset-your-password-2")),
        "an address from the title, one each"
    );
    let reset_id = uid(&reset);
    let renamed = e.update("KbArticle", reset_id, json!({"title": "Reset your password"})).await;
    assert_eq!(renamed["slug"], json!("how-to-reset-your-password"), "the address stays when the title changes");
    let old = e.update("KbArticle", reset_id, json!({"title": "How to reset your password"})).await;
    assert_eq!(old["slug"], reset["slug"]);

    // ── the published list: made by the upkeep, once ──
    assert!(
        k.published_list("help", "help-centre").await.unwrap().is_none(),
        "nothing is public until the upkeep has made the list (a workspace not yet updated)"
    );
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let list = k.published_list("help", "help-centre").await.unwrap().expect("the help centre");
    let titles: Vec<String> = list.rows.iter().map(|r| r["title"].as_str().unwrap().to_string()).collect();
    assert_eq!(titles.len(), 2, "only Public and Published: {titles:?}");
    assert!(!titles.iter().any(|t| t.contains("Warranty") || t.contains("Escalation") || t.contains("Printer jams")), "{titles:?}");
    let page = k
        .published_find("help", "help-centre", &Find { address: Some("how-to-reset-your-password-2".into()), ..Default::default() })
        .await
        .unwrap()
        .expect("an article's page");
    assert!(!page.rows.is_empty(), "{page:?}");
    assert!(page.rows[0]["content"].as_str().unwrap().contains("second article"), "the body is on its page: {:?}", page.rows[0]);
    let found = k.published_find("help", "help-centre", &Find { q: Some("second".into()), ..Default::default() }).await.unwrap().unwrap();
    assert_eq!(found.rows.len(), 0, "search reads the listed fields, not the body: {:?}", found.rows);
    let found = k.published_find("help", "help-centre", &Find { q: Some("reset".into()), ..Default::default() }).await.unwrap().unwrap();
    assert_eq!(found.rows.len(), 2);
    assert!(
        k.published_find("help", "help-centre", &Find { address: Some(uid_slug(&draft)), ..Default::default() }).await.unwrap().is_none(),
        "a draft has no page"
    );
    let _ = (&internal, &customers);
    // An administrator takes it down: the upkeep does not make it again.
    k.add_customization(
        &ferrum_kernel::Ctx::system(e.tenant, k.registry.schema(e.tenant).await.unwrap()),
        "KbArticle",
        serde_json::from_value(json!({"op": "unpublish_list", "name": "help-centre"})).unwrap(),
        None,
    )
    .await
    .unwrap();
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert!(k.published_list("help", "help-centre").await.unwrap().is_none(), "once for a workspace");

    // ── the portal: search, suggestions, views, votes ──
    let pia = e.portal_user("pia@garage.test").await;
    let get = |path: String| {
        let (k, pia, tenant) = (k.clone(), pia.clone(), e.tenant);
        async move { call(&k, Some(&pia), tenant, "GET", &path, Value::Null).await.unwrap() }
    };
    let titles = |v: &Value| v["articles"].as_array().unwrap().iter().map(|a| a["title"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let all = get("portal/articles".into()).await;
    assert_eq!(titles(&all).len(), 3, "Public and Customers Only, Published: {all}");
    assert!(!titles(&all).iter().any(|t| t.contains("Escalation")), "internal ones are the agents'");
    let suggested = get("portal/suggest?subject=I%20need%20to%20reset%20my%20password".into()).await;
    assert!(titles(&suggested)[0].contains("reset your password"), "a customer starting a request is offered what fits: {suggested}");
    assert_eq!(titles(&get("portal/suggest?subject=warranty%20of%20my%20router".into()).await), ["Warranty terms"]);
    let one = get(format!("portal/articles/{reset_id}")).await;
    assert_eq!((one["title"].clone(), one["views"].clone()), (json!("How to reset your password"), json!(1)));
    get(format!("portal/articles/{reset_id}")).await;
    assert_eq!(e.get("KbArticle", reset_id).await["views"], json!(2));
    let internal_id = uid(&internal);
    let hidden = call(&k, Some(&pia), e.tenant, "GET", &format!("portal/articles/{internal_id}"), Value::Null).await;
    assert!(matches!(hidden, Err(ferrum_kernel::Error::NotFound(_))), "an internal article is not there for a customer: {hidden:?}");
    let draft_id = uid(&draft);
    assert!(call(&k, Some(&pia), e.tenant, "GET", &format!("portal/articles/{draft_id}"), Value::Null).await.is_err());
    call(&k, Some(&pia), e.tenant, "POST", &format!("portal/articles/{reset_id}/vote"), json!({"helpful": true})).await.unwrap();
    let twice = call(&k, Some(&pia), e.tenant, "POST", &format!("portal/articles/{reset_id}/vote"), json!({"helpful": false})).await;
    assert!(twice.is_err(), "once a day for each customer: {twice:?}");
    let after = e.get("KbArticle", reset_id).await;
    assert_eq!((after["helpful_count"].clone(), after["not_helpful_count"].clone()), (json!(1), json!(0)));
    assert!(call(&k, None, e.tenant, "GET", "portal/articles", Value::Null).await.is_err(), "signed in only: the public page is the platform's");
}

fn uid_slug(d: &ferrum_kernel::Doc) -> String {
    d["slug"].as_str().unwrap().to_string()
}
