//! Customers write to support (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first), through the real Mail app: each support
//! address is registered with Mail and kept in step; mail to one becomes a ticket of its team, or a message on the ticket
//! it answers, matched by the headers Mail keeps (In-Reply-To, References), then by the secret in the address it was
//! written to, and last by the subject's tag, never by subject text alone; copies and files are kept; loops, noise and
//! floods are recognised and logged; unknown senders are handled as the settings say; mail that fails authentication
//! from someone known is held; and the replies go out through Mail with a Reply-To that routes back, threading and the
//! agent's signature.

mod common;
use common::*;
use ferrum_app_comms::routes;
use ferrum_kernel::Ctx;
use serde_json::{Value, json};
use uuid::Uuid;

/// A raw mail from the fields a test names: `sender`, `sender_name`, `recipients`, `cc`, `subject`, `text_content`,
/// `message_id`, `in_reply_to`, `headers` and `files` (`[[name, text]]`). The mail server vouches for it unless `unverified`.
fn raw(m: &Value) -> String {
    let get = |k: &str| m[k].as_str();
    let mut head = vec![
        format!("From: \"{}\" <{}>", get("sender_name").unwrap_or("Ada Lovelace"), get("sender").unwrap()),
        format!("To: {}", get("recipients").unwrap()),
        format!("Subject: {}", get("subject").unwrap_or("")),
        format!("Message-ID: <{}>", get("message_id").unwrap()),
        "MIME-Version: 1.0".to_string(),
    ];
    if let Some(c) = get("cc") {
        head.push(format!("Cc: {c}"));
    }
    if let Some(r) = get("in_reply_to") {
        head.push(format!("In-Reply-To: <{r}>"));
        head.push(format!("References: <{r}>"));
    }
    for (k, v) in m["headers"].as_object().into_iter().flatten() {
        head.push(format!("{k}: {}", v.as_str().unwrap()));
    }
    if m["unverified"] != json!(true) {
        head.push("Authentication-Results: mx.acme.test; dkim=pass header.d=cust.test; dmarc=pass header.from=cust.test".into());
    }
    let text = get("text_content").unwrap_or("");
    let files = m["files"].as_array().cloned().unwrap_or_default();
    if files.is_empty() {
        head.push("Content-Type: text/plain; charset=utf-8".into());
        return format!("{}\r\n\r\n{text}\r\n", head.join("\r\n"));
    }
    head.push("Content-Type: multipart/mixed; boundary=\"bnd\"".into());
    let mut body = format!("--bnd\r\nContent-Type: text/plain; charset=utf-8\r\n\r\n{text}\r\n");
    for f in files {
        body.push_str(&format!(
            "--bnd\r\nContent-Type: text/plain; name=\"{0}\"\r\nContent-Disposition: attachment; filename=\"{0}\"\r\n\r\n{1}\r\n",
            f[0].as_str().unwrap(),
            f[1].as_str().unwrap()
        ));
    }
    body.push_str("--bnd--\r\n");
    format!("{}\r\n\r\n{body}", head.join("\r\n"))
}

/// Mail arriving at Mail, as a mail server hands it over: Mail stores it and hands it to the app registered for its address.
async fn deliver(e: &Env, mail: Value) -> ferrum_kernel::Doc {
    let r = e.post_mail(&raw(&mail)).await;
    assert_eq!(r["status"], json!("created"), "{r}");
    e.get("Communication", r["id"].as_str().unwrap().parse().unwrap()).await
}

fn mail(from: &str, to: &str, subject: &str, text: &str, id: &str) -> Value {
    json!({"sender": from, "sender_name": "Ada Lovelace", "recipients": to, "subject": subject, "text_content": text, "message_id": id})
}

async fn tickets(e: &Env) -> Vec<ferrum_kernel::Doc> {
    e.list("Ticket", json!({})).await
}

async fn log(e: &Env, outcome: &str) -> Vec<ferrum_kernel::Doc> {
    e.list("InboundLog", json!({"outcome": outcome})).await
}

async fn registered(e: &Env) -> Vec<String> {
    let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
    let r = routes::registered(&mut tx, e.tenant, Some("helpdesk")).await.unwrap();
    tx.commit().await.unwrap();
    r.into_iter().map(|r| r.address).collect()
}

/// Another app's registration (or, with `remove`, taking it back).
async fn other_app(e: &Env, address: &str, remove: bool) {
    let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
    if remove {
        routes::unregister(&mut tx, e.tenant, "crm", address).await.unwrap();
    } else {
        routes::register(&mut tx, e.tenant, &routes::Registration { app: "crm", address, keep_in_mailbox: true }).await.unwrap();
    }
    tx.commit().await.unwrap();
}

#[test]
fn customers_write_to_support() {
    run(scenario());
}

async fn scenario() {
    let e = boot("inbound", true, vec![]).await;

    // A workspace with no support address yet: Mail stores the mail and nobody is handed it.
    let first = deliver(&e, mail("ada@cust.test", "help@acme.test", "Hello", "Anyone?", "first@cust.test")).await;
    assert!(tickets(&e).await.is_empty() && e.list("InboundLog", json!({})).await.is_empty(), "mail to no support address is not ours and is not logged");
    assert!(first["reference_entity"].is_null() && first["routed_to"].is_null());

    // ── addresses are registered with Mail and kept in step ──
    let support = uid(&e.insert("SupportTeam", json!({"team_name": "Support", "members": [{"agent": "sam@acme.test"}, {"agent": "kim@acme.test"}]})).await);
    let billing = uid(&e.insert("SupportTeam", json!({"team_name": "Billing", "members": [{"agent": "bea@acme.test"}]})).await);
    let support_address = uid(&e.insert("SupportAddress", json!({"address": "Support@Acme.test", "label": "Acme Support", "team": support.to_string()})).await);
    let billing_address =
        uid(&e.insert("SupportAddress", json!({"address": "billing@acme.test", "team": billing.to_string(), "default_priority": "High"})).await);
    assert_eq!(registered(&e).await, ["billing@acme.test", "support@acme.test"], "registered as saved, in lower case");
    assert_eq!(e.get("SupportAddress", support_address).await["mail_registered"], json!(true));
    // Changed, disabled, enabled, deleted: Mail's registration follows.
    e.update("SupportAddress", billing_address, json!({"address": "invoices@acme.test"})).await;
    assert_eq!(registered(&e).await, ["invoices@acme.test", "support@acme.test"], "the old address is taken back");
    e.update("SupportAddress", billing_address, json!({"enabled": false})).await;
    assert_eq!(
        (registered(&e).await, e.get("SupportAddress", billing_address).await["mail_registered"].clone()),
        (vec!["support@acme.test".to_string()], json!(false))
    );
    e.update("SupportAddress", billing_address, json!({"address": "billing@acme.test", "enabled": true})).await;
    assert_eq!(registered(&e).await, ["billing@acme.test", "support@acme.test"]);
    let scratch = uid(&e.insert("SupportAddress", json!({"address": "scratch@acme.test", "team": support.to_string()})).await);
    assert_eq!(registered(&e).await.len(), 3);
    e.k.delete(&e.admin, "SupportAddress", scratch, None).await.unwrap();
    assert_eq!(registered(&e).await, ["billing@acme.test", "support@acme.test"], "a deleted address is taken back");
    // What Mail refuses is said on the address, not lost: another app holds it, or it is not a mail domain of the workspace.
    other_app(&e, "desk@acme.test", false).await;
    let desk = uid(&e.insert("SupportAddress", json!({"address": "desk@acme.test", "team": support.to_string()})).await);
    let held_by_other = e.get("SupportAddress", desk).await;
    assert_eq!(held_by_other["mail_registered"], json!(false));
    assert!(held_by_other["registration_note"].as_str().unwrap().contains("overlaps desk@acme.test"), "{held_by_other:?}");
    let elsewhere = uid(&e.insert("SupportAddress", json!({"address": "help@elsewhere.test"})).await);
    assert!(e.get("SupportAddress", elsewhere).await["registration_note"].as_str().unwrap().contains("not one of the workspace's mail domains"));
    // The upkeep: an address a workspace had before registrations existed is registered; one nothing stands behind is taken back;
    // what was refused is tried again, and the reason goes when it is registered.
    {
        let mut tx = e.k.db.tenant_tx(e.tenant).await.unwrap();
        routes::unregister(&mut tx, e.tenant, "helpdesk", "support@acme.test").await.unwrap();
        routes::unregister(&mut tx, e.tenant, "helpdesk", "billing@acme.test").await.unwrap();
        routes::register(&mut tx, e.tenant, &routes::Registration { app: "helpdesk", address: "old@acme.test", keep_in_mailbox: true }).await.unwrap();
        tx.commit().await.unwrap();
    }
    other_app(&e, "desk@acme.test", true).await;
    ferrum_app_helpdesk::maintenance(&e.k).await.unwrap();
    assert_eq!(registered(&e).await, ["billing@acme.test", "desk@acme.test", "support@acme.test"]);
    let fixed = e.get("SupportAddress", desk).await;
    assert_eq!((fixed["mail_registered"].clone(), fixed["registration_note"].clone()), (json!(true), Value::Null));
    // A Mail that has no registrations (an older one) is left alone, and nothing breaks.
    sqlx::raw_sql("alter table comms_mail_route rename to comms_mail_route_older").execute(&e.k.db.pool).await.unwrap();
    ferrum_app_helpdesk::maintenance(&e.k).await.unwrap();
    e.update("SupportAddress", scratch_free(&e).await, json!({"label": "still saves"})).await;
    sqlx::raw_sql("alter table comms_mail_route_older rename to comms_mail_route").execute(&e.k.db.pool).await.unwrap();
    e.k.delete(&e.admin, "SupportAddress", desk, None).await.unwrap();
    e.k.delete(&e.admin, "SupportAddress", elsewhere, None).await.unwrap();
    assert_eq!(registered(&e).await, ["billing@acme.test", "support@acme.test"]);

    e.insert("TicketAssignmentRule", json!({"rule_name": "Support all", "team": support.to_string(), "rule_type": "Round Robin"})).await;
    let ada = uid(&e.insert("Customer", json!({"customer_name": "Ada Ltd", "email": "ada@cust.test", "customer_group": e.list("CustomerGroup", json!({"group_name": "Individual"})).await[0]["id"]})).await);
    let agent = e.user("sam@acme.test", &["Support Agent"]).await;

    // ── a new ticket ──
    let m1 = deliver(
        &e,
        json!({"sender": "Ada@Cust.test", "sender_name": "Ada Lovelace", "recipients": "\"Acme Support\" <support@acme.test>", "cc": "bob@cust.test, support@acme.test",
               "subject": "Printer jam", "text_content": "It jams on page two.", "message_id": "m1@cust.test", "files": [["photo.txt", "bytes of a photo"]]}),
    )
    .await;
    let all = tickets(&e).await;
    assert_eq!(all.len(), 1, "once, not twice: {:?}", e.list("InboundLog", json!({})).await);
    assert_eq!(m1["routed_to"], json!("helpdesk"), "Mail says which app took it");
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
        (json!("Customer"), json!("m1@cust.test"), json!("photo.txt"))
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
    assert!(billing_ticket[0]["channel_address"].is_string());
    // Found by a Bcc: the recipient Mail matched, not a header of the mail.
    let mut bcc = mail("ada@cust.test", "someone@cust.test", "Via Bcc", "Copied blind.", "bcc@cust.test");
    bcc["headers"] = json!({"Delivered-To": "support@acme.test"});
    deliver(&e, bcc).await;
    assert_eq!(e.list("Ticket", json!({"subject": "Via Bcc"})).await.len(), 1);

    // ── the agent's reply goes out through Mail, with a Reply-To that routes back ──
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
    assert_eq!(
        out["reply_to_address"],
        json!(format!("Acme Support <support+{token}@acme.test>")),
        "answers come back to the support address, in its brand's name (made from its label by the upkeep), tagged with the ticket's secret"
    );
    assert_eq!(
        (out["in_reply_to"].clone(), out["mail_references"].clone()),
        (json!("m1@cust.test"), json!("m1@cust.test")),
        "threaded onto the customer's message"
    );
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
    assert_eq!(stored["conversation"][1]["communication"], json!(uid(out).to_string()));
    assert_eq!(stored["status"], json!("Replied"));

    // ── replies: by header, then by the address's secret, then by the subject's secret, never by subject text ──
    let mut r1 = mail("ada@cust.test", "support@acme.test", "Re: Printer jam", "Still jams.", "r1@cust.test");
    r1["in_reply_to"] = json!(reply_id);
    r1["cc"] = json!("cy@cust.test");
    let r1 = deliver(&e, r1).await;
    let t = e.get("Ticket", t_id).await;
    assert_eq!(
        (t["conversation"].as_array().unwrap().len(), t["status"].clone(), t["cc"].clone()),
        (4, json!("Open"), json!("bob@cust.test, cy@cust.test")),
        "a message on the same ticket, reopening it, with its new copy"
    );
    assert_eq!((tickets(&e).await.len(), r1["thread_key"].clone()), (3, json!("m1@cust.test")), "Mail threaded it too: one conversation");
    // By the address it was written to, with nothing else to go by.
    deliver(&e, mail("ada@cust.test", &format!("support+{token}@acme.test"), "A new subject line", "By the address.", "r2@cust.test")).await;
    assert_eq!(e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(), 5, "the secret in the address found it");
    deliver(&e, mail("ada@cust.test", "support+wrongwrongwrong@acme.test", "Wrong address secret", "Guessing.", "r2b@cust.test")).await;
    assert_eq!(e.list("Ticket", json!({"subject": "Wrong address secret"})).await.len(), 1, "a guess is a new ticket");
    // By the subject's tag, last.
    let tagged = format!("Re: [{}/{token}] Printer jam", t["number"].as_str().unwrap());
    deliver(&e, mail("ada@cust.test", "support@acme.test", &tagged, "By the tag.", "r3@cust.test")).await;
    assert_eq!(e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(), 6, "the number and the secret found it");
    // The headers come before the subject: a message that answers one of ours and carries another ticket's tag stays with the first.
    let other_ticket = e.list("Ticket", json!({"subject": "Invoice question"})).await[0].clone();
    let mut crossed = mail(
        "ada@cust.test",
        "support@acme.test",
        &format!("Re: [{}/{}] Invoice question", other_ticket["number"].as_str().unwrap(), other_ticket["mail_token"].as_str().unwrap()),
        "Crossed keys.",
        "r3b@cust.test",
    );
    crossed["in_reply_to"] = json!(reply_id);
    deliver(&e, crossed).await;
    assert_eq!(
        (
            e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(),
            e.get("Ticket", uid(&other_ticket)).await["conversation"].as_array().map_or(0, Vec::len)
        ),
        (7, 1),
        "the other ticket keeps its one message"
    );
    let before = tickets(&e).await.len();
    for (n, subject) in [
        ("wrong secret", format!("Re: [{}/wrongwrongwrong] Printer jam", t["number"].as_str().unwrap())),
        ("no tag", "Re: Printer jam".to_string()),
        ("number alone", format!("Re: [{}] Printer jam", t["number"].as_str().unwrap())),
    ] {
        deliver(&e, mail("ada@cust.test", "support@acme.test", &subject, n, &format!("neg-{n}@cust.test"))).await;
    }
    assert_eq!(e.get("Ticket", t_id).await["conversation"].as_array().unwrap().len(), 7, "no guess reached the ticket");
    assert_eq!(tickets(&e).await.len(), before + 3, "a wrong secret, a subject and a bare number each opened a ticket of their own");

    // ── loops and noise: logged, no ticket, no answer ──
    let before = tickets(&e).await.len();
    let sent_before = e.list("Communication", json!({"direction": "Sent"})).await.len();
    let noise = [
        (json!({"Auto-Submitted": "auto-replied"}), "ada@cust.test", "Automatic reply"),
        (json!({"X-Autoreply": "yes"}), "ada@cust.test", "Automatic reply"),
        (json!({"Precedence": "bulk"}), "ada@cust.test", "Automatic reply"),
        (json!({"List-Id": "<news.shop.test>"}), "ada@cust.test", "Mailing list"),
        (json!({"X-Failed-Recipients": "gone@x.test"}), "mailer-daemon@mx.test", "Bounce"),
        (json!({"Return-Path": "<>"}), "ada@cust.test", "Bounce"),
        (json!({}), "support@acme.test", "Own address"),
        (json!({"X-Ferrum-Helpdesk": "TKT-1"}), "ada@cust.test", "Own address"),
    ];
    let count = noise.len();
    for (n, (headers, from, expect)) in noise.into_iter().enumerate() {
        let mut m = mail(from, "support@acme.test", "Out of office", "I am away", &format!("noise-{n}@x.test"));
        m["headers"] = headers.clone();
        deliver(&e, m).await;
        let ignored = e.list("InboundLog", json!({"outcome": "Ignored", "reason": expect})).await;
        assert!(!ignored.is_empty(), "{headers}: {expect}");
    }
    assert_eq!(tickets(&e).await.len(), before, "nothing became a ticket");
    assert_eq!(e.list("Communication", json!({"direction": "Sent"})).await.len(), sent_before, "nothing was answered");
    assert_eq!(log(&e, "Ignored").await.len(), count);

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
    // The same mail handed to Mail twice is the same message: Mail takes it once, so the app is handed it once.
    let again = raw(&mail("ada@cust.test", "support@acme.test", "Twice to Mail", "Once.", "twice@cust.test"));
    assert_eq!(e.post_mail(&again).await["status"], json!("created"));
    assert_eq!(e.post_mail(&again).await["status"], json!("duplicate"));
    assert_eq!(e.list("Ticket", json!({"subject": "Twice to Mail"})).await.len(), 1);

    // ── a flood from one sender ──
    e.settings(json!({"inbound_per_hour": 3})).await;
    for i in 0..5 {
        deliver(&e, mail("flood@x.test", "support@acme.test", &format!("Spam {i}"), "Buy now", &format!("f{i}@x.test"))).await;
    }
    assert_eq!(e.list("Ticket", json!({"contact_email": "flood@x.test"})).await.len(), 3, "three an hour, then none");
    assert_eq!(e.list("InboundLog", json!({"reason": "Rate limit"})).await.len(), 2);

    // ── unknown senders, as the setting says ──
    let settings = e.settings(json!({"unknown_senders": "Hold for Review", "inbound_per_hour": 20})).await;
    let n = tickets(&e).await.len();
    deliver(&e, mail("new@person.test", "support@acme.test", "Can you help?", "Please.", "h1@person.test")).await;
    deliver(&e, mail("other@person.test", "support@acme.test", "Me too", "Also, in confidence: the door code is 4417.", "h2@person.test")).await;
    assert_eq!(tickets(&e).await.len(), n, "held, not opened");
    let held = log(&e, "Held").await;
    assert_eq!(held.len(), 2);
    // An administrator reads what was held; the role that works the log does not, unless given the field group.
    let h2 = held.iter().find(|h| h["from_address"] == json!("other@person.test")).unwrap();
    assert_eq!(e.get("InboundLog", uid(h2)).await["body"], json!("Also, in confidence: the door code is 4417."), "an administrator reads the held mail");
    let manager = e.user("mia@acme.test", &["Support Manager"]).await;
    let seen = e.k.get(&manager, "InboundLog", uid(h2), false).await.unwrap();
    assert!(seen.get("body").is_none_or(Value::is_null) && seen["subject"] == json!("Me too"), "a field of its own group: {seen:?}");
    let admin = e.admin.clone();
    let released =
        e.action(&admin, "InboundLog", uid(held.iter().find(|h| h["from_address"] == json!("new@person.test")).unwrap()), "release", json!({})).await.unwrap();
    let opened = e.get("Ticket", released["ticket"].as_str().unwrap().parse().unwrap()).await;
    assert_eq!((opened["subject"].clone(), opened["contact_email"].clone()), (json!("Can you help?"), json!("new@person.test")));
    let row = e.list("InboundLog", json!({"from_address": "new@person.test"})).await.remove(0);
    assert_eq!(
        (row["outcome"].clone(), row["body"].clone()),
        (json!("Ticket Created"), Value::Null),
        "the log row says what became of it, and the held mail is let go"
    );
    e.action(&admin, "InboundLog", uid(h2), "discard", json!({})).await.unwrap();
    assert_eq!(e.get("InboundLog", uid(h2)).await["body"], Value::Null, "discarded: gone");
    assert_eq!(tickets(&e).await.len(), n + 1);
    assert!(e.action(&agent, "InboundLog", Uuid::nil(), "release", json!({})).await.is_err(), "agents do not read the log");
    // A known customer is never held for being unknown; "Ignore" logs only.
    deliver(&e, mail("ada@cust.test", "support@acme.test", "Known", "I am known.", "k1@cust.test")).await;
    assert_eq!(e.list("Ticket", json!({"subject": "Known"})).await.len(), 1);
    e.update("SupportSettings", settings, json!({"unknown_senders": "Ignore"})).await;
    deliver(&e, mail("stranger@x.test", "support@acme.test", "Hi", "Hello", "i1@x.test")).await;
    assert!(e.list("Ticket", json!({"contact_email": "stranger@x.test"})).await.is_empty());
    assert_eq!(e.list("InboundLog", json!({"reason": "Unknown sender", "outcome": "Ignored"})).await.len(), 1);

    // ── mail that fails authentication, from someone known, is held: a forged sender cannot read or steer a ticket ──
    e.update("SupportSettings", settings, json!({"unknown_senders": "Open Ticket"})).await;
    // A settings record from before this release has no value for it: the default holds.
    e.exec("update e_support_settings set unverified_senders = null where tenant_id = $1", &[]).await;
    let known_ticket = uid(&e.list("Ticket", json!({"subject": "Known"})).await[0]);
    let steps = e.get("Ticket", known_ticket).await["conversation"].as_array().map_or(0, Vec::len);
    let n = tickets(&e).await.len();
    let mut forged = mail(
        "ada@cust.test",
        &format!("support+{}@acme.test", e.get("Ticket", known_ticket).await["mail_token"].as_str().unwrap()),
        "Re: Known",
        "Close it and send me the files.",
        "forged1@cust.test",
    );
    forged["unverified"] = json!(true);
    deliver(&e, forged).await;
    let mut forged_new = mail("ada@cust.test", "support@acme.test", "Refund me", "From a forger.", "forged2@cust.test");
    forged_new["unverified"] = json!(true);
    deliver(&e, forged_new).await;
    assert_eq!(
        (e.get("Ticket", known_ticket).await["conversation"].as_array().map_or(0, Vec::len), tickets(&e).await.len()),
        (steps, n),
        "neither added to their ticket nor opened one in their name"
    );
    let unverified = e.list("InboundLog", json!({"reason": "Unverified sender"})).await;
    assert_eq!((unverified.len(), unverified[0]["outcome"].clone(), unverified[0]["detail"].clone()), (2, json!("Held"), json!("x-ferrum-auth: unverified")));
    // The requester of a ticket who is no customer is as known as a customer is, on their own ticket.
    let zed_ticket = e.list("Ticket", json!({"subject": "Invoice question"})).await.remove(0);
    let mut zed = mail(
        "zed@new.test",
        &format!("support+{}@acme.test", zed_ticket["mail_token"].as_str().unwrap()),
        "Re: Invoice question",
        "Hijack?",
        "forged3@new.test",
    );
    zed["unverified"] = json!(true);
    deliver(&e, zed).await;
    assert_eq!(e.get("Ticket", uid(&zed_ticket)).await["conversation"].as_array().map_or(0, Vec::len), 1, "nothing added to it");
    assert_eq!(e.list("InboundLog", json!({"reason": "Unverified sender"})).await.len(), 3);
    // A stranger whose mail is unverified is an unknown sender, as the setting says (Open Ticket here).
    let mut stranger = mail("nobody@else.test", "support@acme.test", "From nobody", "Just a question.", "forged4@else.test");
    stranger["unverified"] = json!(true);
    deliver(&e, stranger).await;
    assert_eq!(e.list("Ticket", json!({"subject": "From nobody"})).await.len(), 1);
    // Staff can let one through: it goes where the mail was written, and the check is not made twice.
    let to_release = unverified.iter().find(|h| h["subject"] == json!("Re: Known")).unwrap();
    e.action(&admin, "InboundLog", uid(to_release), "release", json!({})).await.unwrap();
    assert_eq!(
        e.get("Ticket", known_ticket).await["conversation"].as_array().map_or(0, Vec::len),
        steps + 1,
        "released by a person: added to the ticket it answers"
    );
    // Accepting is a setting.
    e.update("SupportSettings", settings, json!({"unverified_senders": "Accept"})).await;
    let mut accepted = mail("ada@cust.test", "support@acme.test", "Accepted anyway", "Unverified but taken.", "accepted@cust.test");
    accepted["unverified"] = json!(true);
    deliver(&e, accepted).await;
    assert_eq!(e.list("Ticket", json!({"subject": "Accepted anyway"})).await.len(), 1);
    e.update("SupportSettings", settings, json!({"unverified_senders": "Hold for Review", "unknown_senders": "Ignore"})).await;

    // ── a ticket that is closed is final: its customer's mail opens one that follows it up ──
    let closed = e.get("Ticket", known_ticket).await;
    e.update("Ticket", uid(&closed), json!({"status": "Resolved", "resolution_details": "Done"})).await;
    e.update("Ticket", uid(&closed), json!({"status": "Closed"})).await;
    let tag = format!("Re: [{}/{}] Known", closed["number"].as_str().unwrap(), closed["mail_token"].as_str().unwrap());
    deliver(&e, mail("ada@cust.test", "support@acme.test", &tag, "It broke again.", "k2@cust.test")).await;
    let follow = e.list("Ticket", json!({"related_ticket": uid(&closed).to_string()})).await;
    assert_eq!(follow.len(), 1, "a new ticket that says which it follows");
    assert_eq!(e.get("Ticket", uid(&closed)).await["status"], json!("Closed"));

    // ── the log is the administrator's ──
    assert!(e.k.list(&agent, "InboundLog", &ferrum_kernel::ListQuery::default()).await.is_err());

    // ── personal data: erasure reaches the log's sender, subject and held mail, and what Mail kept of it ──
    e.update("SupportSettings", settings, json!({"unknown_senders": "Hold for Review"})).await;
    deliver(&e, mail("gone@person.test", "support@acme.test", "Secret", "My address is 2 Long Road.", "gone@person.test")).await;
    let gone = e.list("InboundLog", json!({"from_address": "gone@person.test"})).await;
    assert_eq!(gone[0]["body"], json!("My address is 2 Long Road."));
    e.k.erase_personal_data(&e.admin, "gone@person.test", "asked").await.unwrap();
    let erased = e.list("InboundLog", json!({"message_id": "gone@person.test"})).await;
    assert_eq!(erased.len(), 1);
    assert!(
        erased[0]["from_address"].as_str().is_none_or(|a| !a.contains("gone@")) && erased[0]["subject"].as_str().is_none_or(|a| !a.contains("Secret")),
        "{:?}",
        erased[0]
    );
    assert!(erased[0]["body"].is_null() && erased[0]["payload"].is_null(), "the held mail's words go with the sender: {:?}", erased[0]);
}

/// A support address that exists, to save.
async fn scratch_free(e: &Env) -> Uuid {
    uid(&e.list("SupportAddress", json!({})).await[0])
}
