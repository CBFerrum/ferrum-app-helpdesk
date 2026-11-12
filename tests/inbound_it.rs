//! Customers write to support (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): mail to a support address
//! becomes a ticket of its team, or a message on the ticket it answers, matched by the headers that thread a
//! reply and by the secret in the subject's tag, never by subject text alone; copies and files are kept; loops,
//! noise and floods are recognised and logged; unknown senders are handled as the settings say; and the replies
//! go out through Mail with threading and the agent's signature.

mod common;
use common::*;
use ferrum_kernel::Ctx;
use serde_json::{Value, json};
use uuid::Uuid;

/// Mail arriving as Mail stores it: a received Communication, saved as the system.
async fn deliver(e: &Env, mail: Value) -> ferrum_kernel::Doc {
    let sys = Ctx::system(e.tenant, e.k.registry.schema(e.tenant).await.unwrap());
    let mut m = obj(json!({"channel": "Email", "direction": "Received", "status": "Received", "seen": false}));
    m.extend(obj(mail));
    e.k.insert(&sys, "Communication", m).await.unwrap_or_else(|er| panic!("deliver: {er:?}"))
}

fn mail(from: &str, to: &str, subject: &str, text: &str, id: &str) -> Value {
    json!({"sender": from, "sender_name": "Ada Lovelace", "recipients": to, "subject": subject, "text_content": text, "content": text, "message_id": id})
}

async fn tickets(e: &Env) -> Vec<ferrum_kernel::Doc> {
    e.list("Ticket", json!({})).await
}

async fn log(e: &Env, outcome: &str) -> Vec<ferrum_kernel::Doc> {
    e.list("InboundLog", json!({"outcome": outcome})).await
}

#[test]
fn customers_write_to_support() {
    run(scenario());
}

async fn scenario() {
    let e = boot("inbound", true, vec![]).await;
    let sys = Ctx::system(e.tenant, e.k.registry.schema(e.tenant).await.unwrap());

    // A workspace upgraded from before this release: no support address, no settings record. Mail is nobody's yet.
    let first = deliver(&e, mail("ada@cust.test", "help@acme.test", "Hello", "Anyone?", "first@cust.test")).await;
    assert!(tickets(&e).await.is_empty() && e.list("InboundLog", json!({})).await.is_empty(), "mail to no support address is not ours and is not logged");
    assert!(first["reference_entity"].is_null());

    // Two addresses, each mapped to a team.
    let support = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [{"agent": "sam@acme.test"}, {"agent": "kim@acme.test"}]})).await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bea@acme.test"}]})).await);
    e.insert("SupportAddress", json!({"address": "Support@Acme.test", "label": "Acme Support", "team": support.to_string()})).await;
    e.insert("SupportAddress", json!({"address": "billing@acme.test", "team": billing.to_string(), "default_priority": "High"})).await;
    e.insert("TicketAssignmentRule", json!({"rule_name": "Support all", "team": support.to_string(), "rule_type": "Round Robin"})).await;
    let ada = uid(&e.insert("Customer", json!({"customer_name": "Ada Ltd", "email": "ada@cust.test", "customer_group": e.list("CustomerGroup", json!({"group_name": "Individual"})).await[0]["id"]})).await);
    let agent = e.user("sam@acme.test", &["Support Agent"]).await;

    // ── a new ticket ──
    let file = e.k.store_file(&sys, None, "photo.png", "image/png", b"png-bytes").await.unwrap();
    let m1 = deliver(
        &e,
        json!({"sender": "Ada@Cust.test", "sender_name": "Ada Lovelace", "recipients": "\"Acme Support\" <support@acme.test>", "cc": "bob@cust.test, support@acme.test",
               "subject": "Printer jam", "text_content": "It jams on page two.", "content": "<p>It jams on page two.</p>", "message_id": "m1@cust.test",
               "attachments": [file]}),
    )
    .await;
    let all = tickets(&e).await;
    assert_eq!(all.len(), 1, "{:?}", e.list("InboundLog", json!({})).await);
    let t_id = uid(&all[0]);
    let t = &e.get("Ticket", t_id).await;
    assert_eq!(
        (t["channel"].clone(), t["subject"].clone(), t["contact_email"].clone(), t["contact_name"].clone()),
        (json!("Email"), json!("Printer jam"), json!("ada@cust.test"), json!("Ada Lovelace"))
    );
    assert_eq!(t["customer"], json!(ada.to_string()), "the sender matched the customer by exact address");
    assert_eq!(
        (t["team"].clone(), t["description"].clone(), t["cc"].clone()),
        (json!(support.to_string()), json!("It jams on page two."), json!("bob@cust.test"))
    );
    assert!(t["assigned_to"].as_str().is_some_and(|a| a.ends_with("@acme.test")), "the team's rule picked an agent: {}", t["assigned_to"]);
    assert!(t["mail_token"].as_str().is_some_and(|k| k.len() >= 12) && t["channel_address"].is_string());
    let msgs = t["conversation"].as_array().unwrap();
    assert_eq!(msgs.len(), 1, "the message carries the files and the mail's id");
    assert_eq!(
        (msgs[0]["author_kind"].clone(), msgs[0]["message_id"].clone(), msgs[0]["attachments"][0]["file_name"].clone()),
        (json!("Customer"), json!("m1@cust.test"), json!("photo.png"))
    );
    assert_eq!(e.get("Communication", uid(&m1)).await["reference_id"], json!(t_id.to_string()), "Mail's message is on the ticket");
    let created = log(&e, "Ticket Created").await;
    assert_eq!((created.len(), created[0]["to_address"].clone()), (1, json!("support@acme.test")));
    // The other address: its team, its priority.
    deliver(&e, mail("zed@new.test", "billing@acme.test", "Invoice question", "Where is my invoice?", "b1@new.test")).await;
    let billing_ticket = e.list("Ticket", json!({"subject": "Invoice question"})).await;
    assert_eq!(
        (billing_ticket[0]["team"].clone(), billing_ticket[0]["priority"].clone(), billing_ticket[0]["customer"].clone()),
        (json!(billing.to_string()), json!("High"), Value::Null),
        "an unknown sender is the requester, with no customer"
    );
    assert!(e.list("Ticket", json!({"subject": "Invoice question"})).await[0]["channel_address"].is_string());

    // ── the agent's reply goes out through Mail ──
    e.insert("AgentProfile", json!({"agent": "sam@acme.test", "signature": "Sam, Acme Support"})).await;
    let mut conv = e.get("Ticket", t_id).await["conversation"].as_array().unwrap().clone();
    conv.push(json!({"author_kind": "Agent", "author": "sam@acme.test", "body": "Try **page two** again: https://help.test/jams", "is_internal": false}));
    conv.push(json!({"author_kind": "Agent", "author": "sam@acme.test", "body": "Customer is rude, escalate.", "is_internal": true}));
    let after = e.k.update(&agent, "Ticket", t_id, obj(json!({"conversation": conv})), None).await.unwrap();
    let sent = e.list("Communication", json!({"direction": "Sent"})).await;
    assert_eq!(sent.len(), 1, "the private note is not sent");
    let out = &sent[0];
    let token = after["mail_token"].as_str().unwrap().to_string();
    assert_eq!(out["recipients"], json!("ada@cust.test"));
    assert_eq!(out["cc"], json!("bob@cust.test"), "copies are copied on the reply");
    assert_eq!(out["subject"], json!(format!("Re: [{}/{token}] Printer jam", after["number"].as_str().unwrap())));
    assert_eq!(out["reply_to"], json!(uid(&m1).to_string()), "threaded onto the customer's message");
    assert_eq!((out["reference_entity"].clone(), out["reference_id"].clone()), (json!("Ticket"), json!(t_id.to_string())));
    assert_eq!(out["headers"]["X-Ferrum-Helpdesk"], after["number"]);
    let html = out["content"].as_str().unwrap().to_string();
    assert!(
        html.contains("<strong>page two</strong>") && html.contains("Sam, Acme Support") && html.contains("https://help.example.test/open/Ticket/"),
        "{html}"
    );
    assert!(!html.contains("rude") && !out["text_content"].as_str().unwrap().contains("rude"));
    let reply_id = out["message_id"].as_str().unwrap().to_string();
    let stored = e.get("Ticket", t_id).await;
    assert_eq!(stored["conversation"][1]["message_id"], json!(reply_id), "the reply's Message-ID is kept to match an answer");
    assert_eq!(stored["status"], json!("Replied"));

    // ── replies: by header, by the subject's secret, never by subject text ──
    let mut r1 = mail("ada@cust.test", "support@acme.test", "Re: Printer jam", "Still jams.", "r1@cust.test");
    r1["in_reply_to"] = json!(reply_id);
    r1["cc"] = json!("cy@cust.test");
    deliver(&e, r1).await;
    let t = e.get("Ticket", t_id).await;
    assert_eq!(
        (t["conversation"].as_array().unwrap().len(), t["status"].clone(), t["cc"].clone()),
        (4, json!("Open"), json!("bob@cust.test, cy@cust.test")),
        "a message on the same ticket, reopening it, with its new copy"
    );
    assert_eq!(tickets(&e).await.len(), 2);
    let tagged = format!("Re: [{}/{token}] Printer jam", t["number"].as_str().unwrap());
    deliver(&e, mail("ada@cust.test", "support@acme.test", &tagged, "By the tag.", "r2@cust.test")).await;
    assert_eq!(e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(), 5, "the number and the secret found it");
    deliver(
        &e,
        mail(
            "ada@cust.test",
            "support@acme.test",
            &format!("Re: [{}/wrongwrongwrong] Printer jam", t["number"].as_str().unwrap()),
            "Wrong secret.",
            "r3@cust.test",
        ),
    )
    .await;
    deliver(&e, mail("ada@cust.test", "support@acme.test", "Re: Printer jam", "Only the words.", "r4@cust.test")).await;
    deliver(
        &e,
        mail("ada@cust.test", "support@acme.test", &format!("Re: [{}] Printer jam", t["number"].as_str().unwrap()), "The number alone.", "r5@cust.test"),
    )
    .await;
    assert_eq!(e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(), 5, "no guess reached the ticket");
    assert_eq!(tickets(&e).await.len(), 5, "a wrong secret, a subject and a bare number each opened a ticket of their own");

    // ── loops and noise: logged, no ticket, no answer ──
    let before = tickets(&e).await.len();
    let sent_before = e.list("Communication", json!({"direction": "Sent"})).await.len();
    for (i, (headers, from, expect)) in [
        (json!({"Auto-Submitted": "auto-replied"}), "ada@cust.test", "Automatic reply"),
        (json!({"X-Autoreply": "yes"}), "ada@cust.test", "Automatic reply"),
        (json!({"Precedence": "bulk"}), "ada@cust.test", "Automatic reply"),
        (json!({"List-Id": "<news.shop.test>"}), "ada@cust.test", "Mailing list"),
        (json!({"X-Failed-Recipients": "gone@x.test"}), "mailer-daemon@mx.test", "Bounce"),
        (json!({}), "support@acme.test", "Own address"),
        (json!({"X-Ferrum-Helpdesk": "TKT-1"}), "ada@cust.test", "Own address"),
    ]
    .into_iter()
    .enumerate()
    {
        let n = i;
        let mut m = mail(from, "support@acme.test", "Out of office", "I am away", &format!("noise-{n}@x.test"));
        m["headers"] = headers.clone();
        deliver(&e, m).await;
        let ignored = e.list("InboundLog", json!({"outcome": "Ignored", "reason": expect})).await;
        assert!(!ignored.is_empty(), "{headers}: {expect}");
    }
    assert_eq!(tickets(&e).await.len(), before, "nothing became a ticket");
    assert_eq!(e.list("Communication", json!({"direction": "Sent"})).await.len(), sent_before, "nothing was answered");
    assert_eq!(log(&e, "Ignored").await.len(), 7);

    // ── the same mail twice, at once: one ticket ──
    let twin = |n: u32| {
        let e_k = e.k.clone();
        let tenant = e.tenant;
        async move {
            let mail = ferrum_app_helpdesk::inbound::Inbound {
                message_id: Some("twin@cust.test".into()),
                from: "ada@cust.test".into(),
                to: vec!["support@acme.test".into()],
                subject: format!("Twin {n}"),
                text: "Both of us".into(),
                ..Default::default()
            };
            let ctx = Ctx::system(tenant, e_k.registry.schema(tenant).await.unwrap());
            let mut tx = e_k.db.tenant_tx(tenant).await.unwrap();
            let out = ferrum_app_helpdesk::inbound::receive(&e_k, &ctx, &mut tx, &mail).await.unwrap();
            tx.commit().await.unwrap();
            out
        }
    };
    let (a, b) = tokio::join!(tokio::spawn(twin(1)), tokio::spawn(twin(2)));
    let outcomes = [a.unwrap(), b.unwrap()];
    assert_eq!(outcomes.iter().filter(|o| matches!(o, ferrum_app_helpdesk::inbound::Outcome::Created(_))).count(), 1, "{outcomes:?}");
    assert_eq!(outcomes.iter().filter(|o| **o == ferrum_app_helpdesk::inbound::Outcome::Duplicate).count(), 1);

    // ── a flood from one sender ──
    e.insert("SupportSettings", json!({"inbound_per_hour": 3})).await;
    for i in 0..5 {
        deliver(&e, mail("flood@x.test", "support@acme.test", &format!("Spam {i}"), "Buy now", &format!("f{i}@x.test"))).await;
    }
    assert_eq!(e.list("Ticket", json!({"contact_email": "flood@x.test"})).await.len(), 3, "three an hour, then none");
    assert_eq!(e.list("InboundLog", json!({"reason": "Rate limit"})).await.len(), 2);

    // ── unknown senders, as the setting says ──
    let settings = e.list("SupportSettings", json!({})).await[0].clone();
    e.update("SupportSettings", uid(&settings), json!({"unknown_senders": "Hold for Review", "inbound_per_hour": 20})).await;
    let n = tickets(&e).await.len();
    deliver(&e, mail("new@person.test", "support@acme.test", "Can you help?", "Please.", "h1@person.test")).await;
    deliver(&e, mail("other@person.test", "support@acme.test", "Me too", "Also.", "h2@person.test")).await;
    assert_eq!(tickets(&e).await.len(), n, "held, not opened");
    let held = log(&e, "Held").await;
    assert_eq!(held.len(), 2);
    let admin = e.admin.clone();
    let released =
        e.action(&admin, "InboundLog", uid(held.iter().find(|h| h["from_address"] == json!("new@person.test")).unwrap()), "release", json!({})).await.unwrap();
    let opened = e.get("Ticket", released["ticket"].as_str().unwrap().parse().unwrap()).await;
    assert_eq!((opened["subject"].clone(), opened["contact_email"].clone()), (json!("Can you help?"), json!("new@person.test")));
    assert_eq!(
        e.list("InboundLog", json!({"from_address": "new@person.test"})).await[0]["outcome"],
        json!("Ticket Created"),
        "the log row says what became of it"
    );
    e.action(&admin, "InboundLog", uid(held.iter().find(|h| h["from_address"] == json!("other@person.test")).unwrap()), "discard", json!({})).await.unwrap();
    assert_eq!(tickets(&e).await.len(), n + 1);
    assert!(e.action(&agent, "InboundLog", Uuid::nil(), "release", json!({})).await.is_err(), "agents do not read the log");
    // A known customer is never held; "Ignore" logs only.
    deliver(&e, mail("ada@cust.test", "support@acme.test", "Known", "I am known.", "k1@cust.test")).await;
    assert_eq!(e.list("Ticket", json!({"subject": "Known"})).await.len(), 1);
    e.update("SupportSettings", uid(&settings), json!({"unknown_senders": "Ignore"})).await;
    deliver(&e, mail("stranger@x.test", "support@acme.test", "Hi", "Hello", "i1@x.test")).await;
    assert!(e.list("Ticket", json!({"contact_email": "stranger@x.test"})).await.is_empty());
    assert_eq!(e.list("InboundLog", json!({"reason": "Unknown sender", "outcome": "Ignored"})).await.len(), 1);

    // ── a ticket that is closed is final: its customer's mail opens one that follows it up ──
    let closed = e.get("Ticket", uid(&e.list("Ticket", json!({"subject": "Known"})).await[0])).await;
    e.update("Ticket", uid(&closed), json!({"status": "Resolved", "resolution_details": "Done"})).await;
    e.update("Ticket", uid(&closed), json!({"status": "Closed"})).await;
    let tag = format!("Re: [{}/{}] Known", closed["number"].as_str().unwrap(), closed["mail_token"].as_str().unwrap());
    deliver(&e, mail("ada@cust.test", "support@acme.test", &tag, "It broke again.", "k2@cust.test")).await;
    let follow = e.list("Ticket", json!({"related_ticket": uid(&closed).to_string()})).await;
    assert_eq!(follow.len(), 1, "a new ticket that says which it follows");
    assert_eq!(e.get("Ticket", uid(&closed)).await["status"], json!("Closed"));

    // ── the log is the administrator's ──
    assert!(e.k.list(&agent, "InboundLog", &ferrum_kernel::ListQuery::default()).await.is_err());

    // ── personal data: erasure reaches the log's sender, subject and held mail ──
    e.k.erase_personal_data(&e.admin, "other@person.test", "asked").await.unwrap();
    let erased = e.list("InboundLog", json!({"message_id": "h2@person.test"})).await;
    assert_eq!(erased.len(), 1);
    assert!(
        erased[0]["from_address"].as_str().is_none_or(|a| !a.contains("other@")) && erased[0]["subject"].as_str().is_none_or(|a| !a.contains("Me too")),
        "{:?}",
        erased[0]
    );
}
