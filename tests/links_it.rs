//! Parent, child and linked tickets, and splitting a message (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first): a child
//! names one parent and has none of its own; a parent waits for its open children; a link is kept once and found from
//! both tickets; a customer's message is split into a ticket of its own once; a merge loses no child and no link.
//! One scenario: the suites share one database and reset it.

mod common;
use common::*;
use ferrum_kernel::capability::Invocation;
use ferrum_kernel::{Doc, Error, ListQuery};
use serde_json::{Value, json};
use uuid::Uuid;

#[test]
fn parents_links_and_splits() {
    run(scenario());
}

fn on(r: &ferrum_kernel::Result<impl std::fmt::Debug>, field: &str) -> bool {
    matches!(r, Err(Error::Validation(f)) if f.iter().any(|x| x.field == field))
}

fn says(r: &ferrum_kernel::Result<impl std::fmt::Debug>, words: &str) -> bool {
    format!("{r:?}").contains(words)
}

async fn scenario() {
    let e = boot("links", false, vec![]).await;
    let k = e.k.clone();
    let new = |subject: &'static str, extra: Value| {
        let e = &e;
        async move {
            let mut v = json!({"subject": subject, "priority": "Medium", "contact_email": "cy@cust.test", "contact_name": "Cy"});
            v.as_object_mut().unwrap().extend(extra.as_object().cloned().unwrap_or_default());
            e.k.insert(&e.admin, "Ticket", obj(v)).await
        }
    };
    let list = |filter: Value| {
        let e = &e;
        async move {
            let q = ListQuery { filters: ferrum_kernel::query::parse_filters(&json!([filter])).unwrap(), limit: Some(100), ..Default::default() };
            e.k.list(&e.admin, "Ticket", &q).await.unwrap().rows.iter().map(uid).collect::<Vec<Uuid>>()
        }
    };
    let resolve = |t: Uuid| {
        let e = &e;
        async move { e.k.update(&e.admin, "Ticket", t, obj(json!({"status": "Resolved", "resolution_details": "Done"})), None).await }
    };

    // ── a_child_names_one_parent ──
    let parent = uid(&new("Outage", json!({})).await.unwrap());
    let child = uid(&new("Outage in Pune", json!({"parent_ticket": parent.to_string()})).await.unwrap());
    assert_eq!(list(json!(["parent_ticket", "=", parent.to_string()])).await, vec![child]);

    // ── a_child_has_no_children_and_a_parent_is_no_child ──
    let r = k.update(&e.admin, "Ticket", parent, obj(json!({"parent_ticket": parent.to_string()})), None).await;
    assert!(on(&r, "parent_ticket"), "the platform refuses a record as its own parent: {r:?}");
    let r = new("Grandchild", json!({"parent_ticket": child.to_string()})).await;
    assert!(on(&r, "parent_ticket") && says(&r, "a child has no children"), "{r:?}");
    let other = uid(&new("Other", json!({})).await.unwrap());
    let r = k.update(&e.admin, "Ticket", parent, obj(json!({"parent_ticket": other.to_string()})), None).await;
    assert!(on(&r, "parent_ticket") && says(&r, "has child tickets"), "{r:?}");

    // ── a_parent_waits_for_its_open_children ──
    let child_no = e.get("Ticket", child).await["number"].as_str().unwrap().to_string();
    let r = resolve(parent).await;
    assert!(on(&r, "status") && says(&r, &child_no), "{r:?}");
    resolve(child).await.expect("the child first");
    resolve(parent).await.expect("then the parent");
    e.exec("update e_ticket set resolved_at = resolved_at - interval '8 days' where tenant_id = $1 and id = $2", &[parent]).await;
    ferrum_app_helpdesk::maintenance(&k, e.tenant).await.unwrap();
    assert_eq!(e.get("Ticket", parent).await["status"], json!("Closed"), "the upkeep closes it as before");

    // ── no_open_child_under_a_resolved_parent ──
    let done = uid(&new("Done", json!({})).await.unwrap());
    resolve(done).await.unwrap();
    let r = new("Under a resolved one", json!({"parent_ticket": done.to_string()})).await;
    assert!(on(&r, "parent_ticket") && says(&r, "reopen it first"), "{r:?}");
    let r = k.update(&e.admin, "Ticket", other, obj(json!({"parent_ticket": done.to_string()})), None).await;
    assert!(on(&r, "parent_ticket"), "{r:?}");
    // A child resolved with its parent: the customer's reply reopens the child, and the parent stays as it is.
    let p2 = uid(&new("Parent two", json!({})).await.unwrap());
    let c2 = uid(&new("Child two", json!({"parent_ticket": p2.to_string()})).await.unwrap());
    resolve(c2).await.unwrap();
    resolve(p2).await.unwrap();
    let back = e.update("Ticket", c2, json!({"conversation": [{"author_kind": "Customer", "body": "Still broken"}]})).await;
    assert_eq!(back["status"], json!("Open"), "a customer's message is never refused");

    // ── resolve_and_new_child_at_once ──
    for _ in 0..10 {
        let p = uid(&new("Race", json!({})).await.unwrap());
        let (a, b) = tokio::join!(resolve(p), new("Racer", json!({"parent_ticket": p.to_string()})));
        assert!(!(a.is_ok() && b.is_ok()), "both a resolved parent and an open child: {a:?} {b:?}");
        let resolved = e.get("Ticket", p).await["status"] == json!("Resolved");
        let open_children: Option<i64> =
            e.sql("select count(*) from e_ticket where tenant_id = $1 and parent_ticket = $2 and status in ('Open', 'Replied', 'On Hold')", &[p]).await;
        assert!(!(resolved && open_children.unwrap_or(0) > 0), "the invariant");
    }

    // ── links_are_found_from_both_tickets ──
    let la = uid(&new("Link A", json!({})).await.unwrap());
    let lb = uid(&new("Link B", json!({})).await.unwrap());
    let saved = e.update("Ticket", la, json!({"links": [{"ticket": lb.to_string()}]})).await;
    assert_eq!(saved["links"].as_array().unwrap().len(), 1);
    assert!(list(json!(["links.ticket", "=", la.to_string()])).await.is_empty());
    assert_eq!(list(json!(["links.ticket", "=", lb.to_string()])).await, vec![la]);

    // ── a_ticket_is_not_linked_to_itself_or_twice ──
    let r = k.update(&e.admin, "Ticket", lb, obj(json!({"links": [{"ticket": lb.to_string()}]})), None).await;
    assert!(matches!(&r, Err(Error::Validation(f)) if f.iter().any(|x| x.row.is_some() && x.message.contains("itself"))), "{r:?}");
    let r = k.update(&e.admin, "Ticket", lb, obj(json!({"links": [{"ticket": la.to_string()}, {"ticket": la.to_string()}]})), None).await;
    assert!(matches!(&r, Err(Error::Validation(f)) if f.iter().any(|x| x.row.as_ref().is_some_and(|r| r.1 == 1) && x.message.contains("already"))), "{r:?}");

    // ── a_message_is_split_once ──
    let src = uid(&new("Two problems", json!({"conversation": [{"author_kind": "Customer", "body": "Also, my invoice is wrong"}]})).await.unwrap());
    let say_agent = |t: Uuid, body: &'static str, internal: bool| {
        let e = &e;
        async move {
            let mut conv = e.get("Ticket", t).await["conversation"].as_array().cloned().unwrap_or_default();
            conv.push(json!({"author_kind": "Agent", "body": body, "is_internal": internal}));
            e.update("Ticket", t, json!({"conversation": conv})).await
        }
    };
    say_agent(src, "On it", false).await;
    let t = say_agent(src, "Internal", true).await;
    let conv: Vec<Doc> = t["conversation"].as_array().unwrap().iter().map(|m| obj(m.clone())).collect();
    let (msg, reply, note) =
        (conv[0]["id"].as_str().unwrap().to_string(), conv[1]["id"].as_str().unwrap().to_string(), conv[2]["id"].as_str().unwrap().to_string());
    let made = e.action(&e.admin, "Ticket", src, "split_message", json!({"message": msg})).await.expect("the split");
    let split = e.get("Ticket", made["id"].as_str().unwrap().parse().unwrap()).await;
    assert_eq!(
        (split["description"].clone(), split["contact_email"].clone(), split["subject"].clone()),
        (json!("Also, my invoice is wrong"), json!("cy@cust.test"), json!("Also, my invoice is wrong"))
    );
    assert_eq!(split["links"][0]["ticket"], json!(src.to_string()), "it links to the ticket it came from");
    let marked = e.get("Ticket", src).await["conversation"][0]["split_into"].clone();
    assert_eq!(marked, split["number"]);
    let before: Option<i64> = e.sql("select count(*) from e_ticket where tenant_id = $1", &[]).await;
    let r = e.action(&e.admin, "Ticket", src, "split_message", json!({"message": msg})).await;
    assert!(says(&r, "split off already"), "{r:?}");
    assert_eq!(e.sql::<i64>("select count(*) from e_ticket where tenant_id = $1", &[]).await, before, "no second ticket");

    // ── split_takes_only_a_customers_public_message ──
    for wrong in [reply, note, Uuid::new_v4().to_string()] {
        let r = e.action(&e.admin, "Ticket", src, "split_message", json!({"message": wrong})).await;
        assert!(on(&r, "message"), "{r:?}");
    }

    // ── split_repeats_under_one_key ──
    let src2 = uid(&new("Again two", json!({"conversation": [{"author_kind": "Customer", "body": "Second thing"}]})).await.unwrap());
    let m2 = e.get("Ticket", src2).await["conversation"][0]["id"].as_str().unwrap().to_string();
    let call = || Invocation::new("helpdesk.ticket.split_message", json!({"id": src2, "params": {"message": m2}})).key("split-1");
    let first = k.invoke(&e.admin, call()).await.expect("the split");
    let second = k.invoke(&e.admin, call()).await.expect("the repeat");
    assert!(!first.repeated && second.repeated);
    assert_eq!(first.result["id"], second.result["id"]);
    assert_eq!(list(json!(["links.ticket", "=", src2.to_string()])).await.len(), 1, "one new ticket");

    // ── split_needs_write_and_create ──
    let back = uid(&e.insert("SupportTeam", json!({"team_name": "Back", "members": [{"agent": "bo@acme.test"}]})).await);
    let src3 = uid(&new("Third", json!({"team": back.to_string(), "conversation": [{"author_kind": "Customer", "body": "Third thing"}]})).await.unwrap());
    let m3 = e.get("Ticket", src3).await["conversation"][0]["id"].as_str().unwrap().to_string();
    let reader = e.user("kb@acme.test", &["Knowledge Base Editor"]).await;
    let r = e.action(&reader, "Ticket", src3, "split_message", json!({"message": m3})).await;
    assert!(matches!(r, Err(Error::Forbidden(_)) | Err(Error::NotFound(_))), "{r:?}");
    let front = uid(&e.insert("SupportTeam", json!({"team_name": "Front", "members": [{"agent": "fay@acme.test"}]})).await);
    let fay = e.user("fay@acme.test", &["Support Agent"]).await;
    k.set_user_scopes(&e.admin, fay.user, "SupportTeam", &[front], None).await.unwrap();
    let fay = k.load_ctx(e.tenant, fay.user, None, "it".into()).await.unwrap();
    let r = e.action(&fay, "Ticket", src3, "split_message", json!({"message": m3})).await;
    assert!(matches!(r, Err(Error::NotFound(_)) | Err(Error::Forbidden(_))), "outside her teams: {r:?}");

    // ── merge_carries_children_and_links ──
    let s = uid(&new("Survivor", json!({})).await.unwrap());
    let d = uid(&new("Duplicate", json!({})).await.unwrap());
    let kid = uid(&new("Kid of the duplicate", json!({"parent_ticket": d.to_string()})).await.unwrap());
    let x = uid(&new("Linked to both", json!({})).await.unwrap());
    let y = uid(&new("Linked to the duplicate", json!({})).await.unwrap());
    let z = uid(&new("Links the duplicate", json!({})).await.unwrap());
    e.update("Ticket", s, json!({"links": [{"ticket": x.to_string()}]})).await;
    e.update("Ticket", d, json!({"links": [{"ticket": x.to_string()}, {"ticket": y.to_string()}, {"ticket": s.to_string()}]})).await;
    e.update("Ticket", z, json!({"links": [{"ticket": d.to_string()}]})).await;
    let d_no = e.get("Ticket", d).await["number"].as_str().unwrap().to_string();
    e.action(&e.admin, "Ticket", s, "merge_tickets", json!({"numbers": d_no})).await.expect("the merge");
    assert_eq!(e.get("Ticket", kid).await["parent_ticket"], json!(s.to_string()), "the child moved");
    let mut linked: Vec<String> = e.get("Ticket", s).await["links"].as_array().unwrap().iter().map(|l| l["ticket"].as_str().unwrap().to_string()).collect();
    linked.sort();
    let mut want = vec![x.to_string(), y.to_string()];
    want.sort();
    assert_eq!(linked, want, "no duplicate, no link to itself");
    assert_eq!(e.get("Ticket", z).await["links"][0]["ticket"], json!(s.to_string()), "a link to the duplicate now names the survivor");
    let pk = uid(&new("Parent to merge", json!({})).await.unwrap());
    let ck = uid(&new("Its child", json!({"parent_ticket": pk.to_string()})).await.unwrap());
    let pk_no = e.get("Ticket", pk).await["number"].as_str().unwrap().to_string();
    let r = e.action(&e.admin, "Ticket", ck, "merge_tickets", json!({"numbers": pk_no})).await;
    assert!(says(&r, "is this ticket's parent"), "{r:?}");
}
