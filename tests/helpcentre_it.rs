//! The help centre (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): articles have a visibility and an address;
//! Public, Published ones are the platform's published list with a page each and search; customers in the portal search,
//! are offered articles when they start a request, and their views and "was this helpful" are counted.

mod common;
use common::*;
use ferrum_kernel::published::{Count, Counted, Find};
use serde_json::{Value, json};
use std::collections::HashMap;

#[test]
fn the_help_centre() {
    run(async {
        scenario().await;
        an_older_list_gets_counting().await;
    });
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
    // ── the public page counts its visitors' views and votes, into the article's own fields ──
    let slug = "how-to-reset-your-password-2";
    let count = |visitor: &'static str, what: Count| {
        let k = k.clone();
        async move { k.published_count("help", "help-centre", slug, visitor, "203.0.113.5", what).await.unwrap() }
    };
    assert_eq!(count("v1", Count::View).await, Some(Counted::Counted));
    assert_eq!(count("v1", Count::View).await, Some(Counted::Already), "once a visitor a day");
    assert_eq!(count("v2", Count::View).await, Some(Counted::Counted));
    assert_eq!(count("v1", Count::Vote(true)).await, Some(Counted::Counted));
    assert_eq!(count("v1", Count::Vote(false)).await, Some(Counted::Counted), "another answer moves the vote");
    let second = e.get("KbArticle", uid(&reset_again)).await;
    assert_eq!((second["views"].clone(), second["helpful_count"].clone(), second["not_helpful_count"].clone()), (json!(2), json!(0), json!(1)));
    assert_eq!(second["version"], reset_again["version"], "counted without a new version of the article");
    let uncounted = k.published_count("help", "help-centre", &uid_slug(&draft), "v1", "203.0.113.5", Count::View).await.unwrap();
    assert_eq!(uncounted, None, "a draft has no page to count");

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

    // ── the report by article: both ways in, together ──
    let report = ferrum_domain::reports::run(&k, &e.admin, "support_articles", &HashMap::new()).await.unwrap();
    let rows = report["rows"].as_array().unwrap();
    let of = |title: &str, n: usize| rows.iter().filter(|r| r["title"] == json!(title)).nth(n).unwrap_or_else(|| panic!("{title} in {report}")).clone();
    assert_eq!((rows.len(), report["totals"]["views"].clone(), report["totals"]["votes"].clone()), (5, json!(4), json!(2)));
    let (liked, disliked) = (rows.iter().find(|r| r["helpful"] == json!(1)).unwrap(), rows.iter().find(|r| r["not_helpful"] == json!(1)).unwrap());
    assert_eq!(
        (liked["views"].clone(), liked["not_helpful"].clone(), liked["helpful_pct"].clone()),
        (json!(2), json!(0), json!("100")),
        "the portal's: two views, one vote"
    );
    assert_eq!(
        (disliked["views"].clone(), disliked["helpful"].clone(), disliked["helpful_pct"].clone()),
        (json!(2), json!(0), json!("0")),
        "the public page's: two visitors, one vote"
    );
    assert_eq!((of("Warranty terms", 0)["views"].clone(), of("Warranty terms", 0)["votes"].clone()), (json!(0), json!(0)));
    let published = ferrum_domain::reports::run(
        &k,
        &e.admin,
        "support_articles",
        &[("status".to_string(), "Published".to_string()), ("visibility".to_string(), "Customers Only".to_string())].into(),
    )
    .await
    .unwrap();
    assert_eq!(published["rows"].as_array().unwrap().len(), 1, "by status and visibility");
    let agent = e.user("sam@acme.test", &["Support Agent"]).await;
    assert!(ferrum_domain::reports::run(&k, &agent, "support_articles", &HashMap::new()).await.is_ok(), "agents read the help centre's numbers");
}

/// A workspace that was updated to 0.10.0: its help centre is a list that does not count. The upkeep makes it count, once, and keeps
/// what an administrator changed in it.
async fn an_older_list_gets_counting() {
    let e = boot("help010", false, vec![]).await;
    let k = e.k.clone();
    let sctx = ferrum_kernel::Ctx::system(e.tenant, k.registry.schema(e.tenant).await.unwrap());
    let category = uid(&e.list("KbCategory", json!({"category_name": "General"})).await[0]);
    e.insert("KbArticle", json!({"title": "Pairing a router", "category": category.to_string(), "visibility": "Public", "status": "Published", "content": "Hold the button.", "summary": "About pairing"})).await;
    // As 0.10.0 made it, with the one change an administrator made (seven rows, not a hundred), and the flag it left.
    k.add_customization(
        &sctx,
        "KbArticle",
        serde_json::from_value(
            json!({"op": "publish_list", "list": {"name": "help-centre", "label": "Help centre", "description": "Answers to common questions",
            "fields": ["title", "summary", "category", "published_at"], "detail": ["content"], "slug": "slug", "summary": "summary",
            "filter": [["status", "=", "Published"], ["visibility", "=", "Public"]], "sort": ["published_at", "desc"], "limit": 7}}),
        )
        .unwrap(),
        None,
    )
    .await
    .unwrap();
    e.settings(json!({})).await;
    let mut tx = k.db.tenant_tx(e.tenant).await.unwrap();
    sqlx::query("update e_support_settings set defaults_made = true where tenant_id = $1").bind(e.tenant).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let count = || {
        let k = k.clone();
        async move { k.published_count("help010", "help-centre", "pairing-a-router", "v1", "203.0.113.5", Count::View).await.unwrap() }
    };
    assert_eq!(count().await, None, "the list as it was does not count");
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(count().await, Some(Counted::Counted), "now it counts");
    let schema = k.registry.schema(e.tenant).await.unwrap();
    let list = &schema.published_list("help-centre").expect("the list").list;
    assert_eq!(
        (list.limit, list.count_views.as_deref(), list.votes.as_ref().map(|v| (v.up.as_str(), v.down.as_str()))),
        (Some(7), Some("views"), Some(("helpful_count", "not_helpful_count"))),
        "the administrator's seven rows are kept"
    );
    // Once: an administrator who turns counting off is not overruled.
    k.add_customization(
        &sctx,
        "KbArticle",
        serde_json::from_value(json!({"op": "publish_list", "list": {"name": "help-centre", "label": "Help centre",
        "fields": ["title"], "slug": "slug", "limit": 3}}))
        .unwrap(),
        None,
    )
    .await
    .unwrap();
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let schema = k.registry.schema(e.tenant).await.unwrap();
    assert_eq!(schema.published_list("help-centre").unwrap().list.count_views, None);
}

fn uid_slug(d: &ferrum_kernel::Doc) -> String {
    d["slug"].as_str().unwrap().to_string()
}
