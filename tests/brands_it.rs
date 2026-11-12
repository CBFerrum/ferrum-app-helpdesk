//! A brand on a support address (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): a name, a signature and a wrapper as
//! records, made once from the labels a workspace's addresses already had, and carried by the replies to tickets that
//! came in at the address, through the real Mail app.

mod common;
use common::*;
use serde_json::{Value, json};

fn raw(from: &str, to: &str, subject: &str, id: &str) -> String {
    format!(
        "From: \"Ada\" <{from}>\r\nTo: {to}\r\nSubject: {subject}\r\nMessage-ID: <{id}>\r\nAuthentication-Results: mx.acme.test; dmarc=pass header.from=cust.test\r\n\
         Content-Type: text/plain; charset=utf-8\r\n\r\nIt does not work.\r\n"
    )
}

#[test]
fn brands() {
    run(scenario());
}

async fn scenario() {
    let e = boot("brands", true, vec![]).await;
    let k = e.k.clone();
    let team = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [{"agent": "sam@acme.test"}]})).await);
    let agent = e.user("sam@acme.test", &["Support Agent"]).await;
    // A workspace as 0.10 left it: addresses with a label, no brand.
    let acme = uid(&e.insert("SupportAddress", json!({"address": "support@acme.test", "label": "Acme Support", "team": team.to_string()})).await);
    let plain = uid(&e.insert("SupportAddress", json!({"address": "plain@acme.test", "team": team.to_string()})).await);
    let also = uid(&e.insert("SupportAddress", json!({"address": "also@acme.test", "label": "Acme Support", "team": team.to_string()})).await);
    assert!(e.list("SupportBrand", json!({})).await.is_empty());

    // The upkeep makes a brand of each label, once, and links the addresses that carried it.
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    let brands = e.list("SupportBrand", json!({})).await;
    assert_eq!((brands.len(), brands[0]["brand_name"].clone(), brands[0]["display_name"].clone()), (1, json!("Acme Support"), json!("Acme Support")));
    let brand = uid(&brands[0]);
    for address in [acme, also] {
        assert_eq!(e.get("SupportAddress", address).await["brand"], json!(brand.to_string()));
    }
    assert_eq!(e.get("SupportAddress", plain).await["brand"], Value::Null, "no label, no brand");
    ferrum_app_helpdesk::maintenance(&k).await.unwrap();
    assert_eq!(e.list("SupportBrand", json!({})).await.len(), 1, "once");
    // The agent reads brands and cannot change them.
    assert_eq!(e.k.get(&agent, "SupportBrand", brand, false).await.unwrap()["brand_name"], json!("Acme Support"));
    assert!(e.k.update(&agent, "SupportBrand", brand, obj(json!({"display_name": "Mine"})), None).await.is_err());

    // A brand is what a customer sees: its name on the reply address, its signature after the agent's, its wrapper around the reply.
    e.update(
        "SupportBrand",
        brand,
        json!({"display_name": "Acme Help", "signature": "The Acme Help team", "header": "**Acme Help desk**", "footer": "Acme Ltd, 1 Long Road"}),
    )
    .await;
    e.insert("AgentProfile", json!({"agent": "sam@acme.test", "signature": "Sam"})).await;
    let branded = {
        assert_eq!(e.post_mail(&raw("ada@cust.test", "support@acme.test", "Branded", "b1@cust.test")).await["status"], json!("created"));
        e.list("Ticket", json!({"subject": "Branded"})).await.remove(0)
    };
    let unbranded = {
        assert_eq!(e.post_mail(&raw("bob@cust.test", "plain@acme.test", "Unbranded", "b2@cust.test")).await["status"], json!("created"));
        e.list("Ticket", json!({"subject": "Unbranded"})).await.remove(0)
    };
    let reply = |ticket: ferrum_kernel::Doc, body: &'static str| {
        let (e, agent) = (&e, agent.clone());
        async move {
            let mut conv = e.get("Ticket", uid(&ticket)).await["conversation"].as_array().unwrap().clone();
            conv.push(json!({"author_kind": "Agent", "author": "sam@acme.test", "body": body, "is_internal": false}));
            e.k.update(&agent, "Ticket", uid(&ticket), obj(json!({"conversation": conv})), None).await.unwrap()
        }
    };
    let b = reply(branded.clone(), "We are on it.").await;
    reply(unbranded.clone(), "We are on it.").await;
    let sent = e.list("Communication", json!({"direction": "Sent"})).await;
    let to_ada = sent.iter().find(|m| m["recipients"] == json!("ada@cust.test")).expect("the branded reply");
    let html = to_ada["content"].as_str().unwrap();
    let (head, we, sign, foot) = (html.find("Acme Help desk"), html.find("We are on it"), html.find("The Acme Help team"), html.find("1 Long Road"));
    assert!(head < we && we < sign && sign < foot, "the wrapper outside, the agent's words, then the signatures: {html}");
    assert!(html.find("Sam").is_some_and(|s| we < Some(s) && Some(s) < sign), "the agent's signature comes before the brand's: {html}");
    assert_eq!(
        to_ada["reply_to_address"],
        json!(format!("Acme Help <support+{}@acme.test>", b["mail_token"].as_str().unwrap())),
        "answers go to the brand, tagged with the ticket's secret"
    );
    let to_bob = sent.iter().find(|m| m["recipients"] == json!("bob@cust.test")).expect("the unbranded reply");
    assert_eq!(to_bob["reply_to_address"].as_str().unwrap(), format!("plain+{}@acme.test", unbranded["mail_token"].as_str().unwrap()));
    assert!(!to_bob["content"].as_str().unwrap().contains("Acme Help"), "no brand, no wrapper");
    // A name that is not plain words is cut to what is, and a brand of nothing but such characters falls back to the address.
    e.update("SupportBrand", brand, json!({"display_name": "\"<>,;"})).await;
    let c = reply(e.get("Ticket", uid(&branded)).await, "Second reply.").await;
    let sent = e.list("Communication", json!({"direction": "Sent", "recipients": "ada@cust.test"})).await;
    assert_eq!(sent.len(), 2);
    let second = sent.iter().find(|m| m["content"].as_str().unwrap().contains("Second reply")).unwrap();
    assert_eq!(second["reply_to_address"], json!(format!("support+{}@acme.test", c["mail_token"].as_str().unwrap())));
    // The second reply threads onto the first: In-Reply-To is the customer's message, References carry the whole chain.
    assert_eq!(second["in_reply_to"], json!("b1@cust.test"));
    assert!(second["mail_references"].as_str().unwrap().contains(to_ada["message_id"].as_str().unwrap()), "{second:?}");
}
