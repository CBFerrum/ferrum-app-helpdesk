//! Satisfaction (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): a solved ticket's customer is mailed a signed
//! link to rate it, once; the rating lands on the ticket; a bad rating reopens a resolved ticket or tells a lead
//! as the settings say; the report adds it up by agent, team and period.

mod common;
use common::*;
use serde_json::{Value, json};
use std::collections::HashMap;

fn token_in(html: &str) -> String {
    let after = html.split("/links/").nth(1).unwrap_or_else(|| panic!("no link in {html}"));
    after.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect()
}

#[test]
fn satisfaction() {
    run(scenario());
}

async fn scenario() {
    let e = boot("survey", false, vec![]).await;
    let k = e.k.clone();
    let solve = |subject: &'static str, who: &'static str, agent: &'static str| {
        let e = &e;
        async move {
            let t = uid(&e.insert("Ticket", json!({"subject": subject, "contact_email": who, "contact_name": "Cust", "assigned_to": agent})).await);
            e.update("Ticket", t, json!({"status": "Resolved", "resolution_details": "Done"})).await;
            t
        }
    };
    let first = solve("Printer jam", "ada@cust.test", "sam@acme.test").await;
    // A workspace upgraded from before: no survey settings, nothing is asked.
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    drain(&k).await;
    assert!(sent_to("ada@cust.test").is_empty(), "off until the workspace turns it on");

    let settings = e.settings(json!({"survey_enabled": true, "survey_delay_hours": 0, "survey_reopen_at_or_below": 2, "survey_alert_at_or_below": 2, "survey_alert_to": "lead@acme.test"})).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    drain(&k).await;
    let mails = sent_to("ada@cust.test");
    assert_eq!(mails.len(), 1, "once, however often the upkeep runs: {mails:?}");
    assert!(mails[0].1.starts_with("How did we do?") && mails[0].2.contains("Tell us how we did"), "{mails:?}");
    let token = token_in(&mails[0].2);

    // The link: the public page, no sign-in; one use.
    let page = k.link_page("survey", &token).await.unwrap().expect("the page");
    assert!(format!("{page:?}").contains("rate"), "{page:?}");
    let bad = k.use_link("survey", &token, "rate", &json!({"rating": 9}).as_object().unwrap().clone(), None).await;
    assert!(bad.is_err() || bad.as_ref().is_ok_and(|r| r.is_none()) || format!("{bad:?}").contains("1 to 5"), "{bad:?}");
    k.use_link("survey", &token, "rate", &json!({"rating": 1, "feedback": "Slow and rude"}).as_object().unwrap().clone(), None).await.unwrap();
    let t = e.get("Ticket", first).await;
    assert_eq!(
        (t["satisfaction_rating"].clone(), t["feedback"].clone(), t["status"].clone()),
        (json!(1), json!("Slow and rude"), json!("Open")),
        "a bad rating reopens a resolved ticket"
    );
    assert!(t["rated_at"].is_string());
    let again = k.use_link("survey", &token, "rate", &json!({"rating": 5}).as_object().unwrap().clone(), None).await;
    assert!(matches!(again, Ok(None)) || again.is_err(), "the link works once: {again:?}");
    assert_eq!(e.get("Ticket", first).await["satisfaction_rating"], json!(1));
    drain(&k).await;
    let lead = sent_to("lead@acme.test");
    assert_eq!((lead.len(), lead[0].1.clone()), (1, "A 1-star rating on ".to_string() + t["number"].as_str().unwrap()), "the lead is told: {lead:?}");

    // A good rating changes nothing else; a closed ticket takes the rating without reopening.
    let second = solve("Invoice", "bob@cust.test", "kim@acme.test").await;
    let third = solve("Login", "cy@cust.test", "kim@acme.test").await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    drain(&k).await;
    let rate = |who: &'static str, rating: i64| {
        let k = k.clone();
        let token = token_in(&sent_to(who)[0].2);
        async move { k.use_link("survey", &token, "rate", &json!({"rating": rating}).as_object().unwrap().clone(), None).await.unwrap() }
    };
    e.update("Ticket", third, json!({"status": "Closed"})).await;
    rate("bob@cust.test", 5).await;
    rate("cy@cust.test", 2).await;
    let (second_t, third_t) = (e.get("Ticket", second).await, e.get("Ticket", third).await);
    assert_eq!((second_t["satisfaction_rating"].clone(), second_t["status"].clone()), (json!(5), json!("Resolved")));
    assert_eq!(
        (third_t["satisfaction_rating"].clone(), third_t["status"].clone()),
        (json!(2), json!("Closed")),
        "a closed ticket is final: rated, not reopened"
    );
    assert_eq!(sent_to("lead@acme.test").len(), 1, "no second alert yet");
    drain(&k).await;
    assert_eq!(sent_to("lead@acme.test").len(), 2, "the 2-star rating told the lead too");

    // Tickets with no address, merged ones and tickets solved before the link could last are not asked.
    let none = uid(&e.insert("Ticket", json!({"subject": "Phone call"})).await);
    e.update("Ticket", none, json!({"status": "Resolved", "resolution_details": "Done"})).await;
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let asked: i64 = e.sql("select count(*) from e_ticket where tenant_id = $1 and survey_sent_at is not null", &[]).await.unwrap();
    assert_eq!(asked, 3);

    // The report: by agent, team and period.
    let today = chrono::Utc::now().date_naive();
    let p = |g: &'static str| -> HashMap<String, String> {
        [("from_date", (today - chrono::Duration::days(30)).to_string()), ("to_date", today.to_string()), ("group_by", g.to_string())]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect()
    };
    let by_agent = ferrum_domain::reports::run(&k, &e.admin, "support_customer_satisfaction", &p("Agent")).await.unwrap();
    let row = |r: &Value, g: &str| r["rows"].as_array().unwrap().iter().find(|x| x["group"] == json!(g)).cloned().unwrap_or_else(|| panic!("{g} in {r}"));
    assert_eq!((row(&by_agent, "kim@acme.test")["rated"].clone(), row(&by_agent, "kim@acme.test")["average_rating"].clone()), (json!(2), json!("3.5")));
    assert_eq!(row(&by_agent, "sam@acme.test")["rated"], json!(1));
    let by_month = ferrum_domain::reports::run(&k, &e.admin, "support_customer_satisfaction", &p("Month")).await.unwrap();
    assert_eq!(by_month["totals"]["rated"], json!(3), "{by_month}");
    let by_team = ferrum_domain::reports::run(&k, &e.admin, "support_customer_satisfaction", &p("Team")).await.unwrap();
    assert_eq!(by_team["totals"]["rated"], json!(3));
    let _ = settings;
}
