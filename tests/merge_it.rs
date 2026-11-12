//! Merging duplicate tickets (`FERRUM_HELPDESK_TEST_DATABASE_URL`, reset first, as the other
//! suite does; they run one after the other): the conversation
//! moves in time order and marked with where it came from, tags and watchers come along (files, comments and assignments move by the same statements, untested here), the
//! duplicate closes as merged and a customer's reply to it lands on the survivor.

use ferrum_kernel::secret::SecretKey;
use ferrum_kernel::tenant::NewTenant;
use ferrum_kernel::{Ctx, Db, DbConfig, Doc, Error, HookRegistry, Kernel, KernelSettings, SiteRequest};
use ferrum_meta::Catalog;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

const PASSWORD: &str = "Correct-Horse-9";

fn obj(v: Value) -> Doc {
    v.as_object().cloned().unwrap()
}

fn uid(d: &Doc) -> Uuid {
    d["id"].as_str().unwrap().parse().unwrap()
}

fn bodies(t: &Doc) -> Vec<String> {
    t["conversation"].as_array().unwrap().iter().map(|m| m["body"].as_str().unwrap().to_string()).collect()
}

#[test]
fn merging_duplicate_tickets() {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(16 << 20).build().unwrap();
    rt.block_on(async { tokio::spawn(scenario()).await.unwrap() });
}

async fn scenario() {
    let url = std::env::var("FERRUM_HELPDESK_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://ferrum:ferrum@localhost:5432/ferrum_helpdesk_test".into());
    let db = Db::connect(DbConfig { url, max_connections: 16, ..Default::default() }).await.expect("connect");
    sqlx::raw_sql("drop schema if exists public cascade; create schema public;").execute(&db.pool).await.expect("reset");
    let mut packages = ferrum_domain::packages().expect("platform packages");
    packages.extend(ferrum_app_erp::packages().expect("ERP's packages"));
    packages.extend(ferrum_app_helpdesk::packages().expect("app packages"));
    let catalog = Catalog::from_packages(packages).expect("catalog");
    let mut hooks = HookRegistry::default();
    ferrum_domain::register(&mut hooks);
    ferrum_app_erp::register(&mut hooks);
    ferrum_app_helpdesk::register(&mut hooks);
    let settings = KernelSettings { secret_key: Some(SecretKey::new([7u8; 32])), ..Default::default() };
    let k = Kernel::boot(db, catalog, hooks, settings, None).await.expect("boot");
    ferrum_domain::maintenance(&k).await.expect("partitions");
    let tenant = k
        .create_tenant(NewTenant {
            slug: "merge".into(),
            name: "Merge Co".into(),
            admin_email: "admin@merge.test".into(),
            admin_password: PASSWORD.into(),
            admin_name: None,
            packages: vec!["helpdesk".into()],
            settings: None,
        })
        .await
        .expect("tenant");
    let session = k.login("merge", "admin@merge.test", PASSWORD).await.expect("login");
    let admin = k.load_ctx(tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx");
    let agent_id = k.create_user(&admin, "agent@merge.test", "Agent", Some(PASSWORD), &["Support Agent".to_string()], false).await.unwrap();
    let s = k.login("merge", "agent@merge.test", PASSWORD).await.unwrap();
    let agent = k.load_ctx(tenant, agent_id, Some(s.token_version), "it".into()).await.unwrap();
    let pia_id = k.create_user(&admin, "pia@garage.test", "Pia", None, &[], false).await.unwrap();
    k.set_user_kind(&admin, pia_id, "portal").await.unwrap();
    let pia = k.load_ctx(tenant, pia_id, None, "it".into()).await.unwrap();

    let customer = k.insert(&admin, "Customer", obj(json!({"customer_name": "Pia's Garage", "email": "pia@garage.test"}))).await.unwrap();
    let other = k.insert(&admin, "Customer", obj(json!({"customer_name": "Someone Else", "email": "else@x.test"}))).await.unwrap();
    let ticket = |subject: &str, who: &Doc, first: &str| json!({"subject": subject, "customer": who["id"], "contact_email": who["email"], "conversation": [{"author_kind": "Customer", "body": first}]});
    let a = k.insert(&agent, "Ticket", obj(ticket("Brakes squeak", &customer, "A1 front left"))).await.unwrap();
    let b = k.insert(&agent, "Ticket", obj(ticket("Brakes squeak!!", &customer, "B1 again, please"))).await.unwrap();
    let c = k.insert(&agent, "Ticket", obj(ticket("Invoice", &other, "C1"))).await.unwrap();
    let (a_id, b_id) = (uid(&a), uid(&b));
    let say = |t: Doc, kind: &str, body: &str, internal: bool| {
        let k = k.clone();
        let agent = agent.clone();
        let body = body.to_string();
        let kind = kind.to_string();
        async move {
            let mut conv = t["conversation"].as_array().unwrap().clone();
            conv.push(json!({"author_kind": kind, "body": body, "is_internal": internal}));
            k.update(&agent, "Ticket", t["id"].as_str().unwrap().parse().unwrap(), obj(json!({"conversation": conv})), None).await.unwrap()
        }
    };
    let b = say(b, "Agent", "B2 we are looking", false).await;
    let _b = say(b, "Agent", "B3 same as A", true).await;
    let a = say(a, "Agent", "A2 on it", false).await;
    let _ = a;
    k.set_tags(&agent, "Ticket", b_id, &["vip".into(), "brakes".into()]).await.unwrap();
    k.set_tags(&agent, "Ticket", a_id, &["vip".into()]).await.unwrap();
    {
        let mut tx = k.db.tenant_tx(tenant).await.unwrap();
        sqlx::query("insert into record_follower (tenant_id, entity, record_id, user_id) values ($1, 'Ticket', $2, $3)")
            .bind(tenant)
            .bind(b_id)
            .bind(agent_id)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    let a = k.get(&admin, "Ticket", a_id, false).await.unwrap();
    let mails = |k: Arc<Kernel>| async move {
        let mut tx = k.db.tenant_tx(tenant).await.unwrap();
        let n: i64 =
            sqlx::query_scalar("select count(*) from outbox where tenant_id = $1 and topic = 'ferrum.mail'").bind(tenant).fetch_one(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        n
    };
    let (mails_before, b_number) = (mails(k.clone()).await, k.get(&admin, "Ticket", b_id, false).await.unwrap()["number"].as_str().unwrap().to_string());

    // What can not be merged.
    let refused = |r: Result<Value, Error>, says: &str| {
        let m = format!("{r:?}");
        assert!(matches!(r, Err(Error::Invalid(_) | Error::Validation(_))) && m.contains(says), "expected {says:?}: {m}");
    };
    let merge = |who: &Ctx, into: Uuid, numbers: &str| {
        let (k, who, numbers) = (k.clone(), who.clone(), numbers.to_string());
        async move { k.run_action(&who, "Ticket", into, "merge_tickets", json!({"numbers": numbers})).await }
    };
    refused(merge(&agent, a_id, c["number"].as_str().unwrap()).await, "another customer's");
    refused(merge(&agent, a_id, a["number"].as_str().unwrap()).await, "not merged into itself");
    refused(merge(&agent, a_id, "TKT-0000-000000").await, "not a ticket you may merge");

    // The merge.
    let done = merge(&agent, a_id, &b_number).await.expect("merged");
    assert_eq!(done["messages_moved"], 3);
    let a2 = k.get(&admin, "Ticket", a_id, false).await.unwrap();
    let said = bodies(&a2);
    assert_eq!(said[..5], ["A1 front left", "B1 again, please", "B2 we are looking", "B3 same as A", "A2 on it"], "in the order they were sent: {said:?}");
    assert!(said[5].contains("was merged into this ticket by agent@merge.test"), "who merged, in the conversation: {said:?}");
    let from: Vec<Option<&str>> = a2["conversation"].as_array().unwrap().iter().map(|m| m["merged_from"].as_str()).collect();
    assert_eq!(from[..2], [None, Some(b_number.as_str())], "each message says where it came from: {from:?}");
    assert_eq!(mails(k.clone()).await, mails_before, "nothing that moved was sent again");
    let b2 = k.get(&admin, "Ticket", b_id, false).await.unwrap();
    assert_eq!(
        (b2["status"].as_str(), b2["merged_into"].as_str(), b2["merged_by"].as_str()),
        (Some("Closed"), Some(a_id.to_string().as_str()), Some("agent@merge.test"))
    );
    assert!(b2["merged_at"].is_string() && bodies(&b2).len() == 1 && bodies(&b2)[0].contains("Merged into"), "{b2:?}");
    let mut tags = k.tags(&admin, "Ticket", a_id).await.unwrap();
    tags.sort();
    assert_eq!(tags, ["brakes", "vip"], "tags come along, once each");
    assert!(k.tags(&admin, "Ticket", b_id).await.unwrap().is_empty());
    let followers: i64 = {
        let mut tx = k.db.tenant_tx(tenant).await.unwrap();
        let n = sqlx::query_scalar("select count(*) from record_follower where tenant_id = $1 and record_id = $2")
            .bind(tenant)
            .bind(a_id)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        n
    };
    assert_eq!(followers, 1, "its watcher");
    for f in ["opened_at", "response_by", "resolution_by", "sla_status", "escalation_level"] {
        assert_eq!(a[f], a2[f], "the survivor's {f} is not reset");
    }
    refused(merge(&agent, a_id, &b_number).await, "is closed");
    refused(k.update(&agent, "Ticket", b_id, obj(json!({"subject": "Reopen"})), None).await.map(Value::Object), "final");

    // A customer's reply to the merged ticket lands on the one that stayed.
    let site = k.hooks.site("helpdesk").cloned().expect("endpoints");
    let req = SiteRequest {
        tenant,
        user: Some(pia.clone()),
        method: "POST".into(),
        path: format!("/portal/tickets/{b_id}/replies"),
        query: Default::default(),
        headers: Default::default(),
        body: json!({"body": "Any news?"}).to_string().into_bytes(),
        client_ip: "203.0.113.9".into(),
    };
    site.handle(&k, req).await.expect("a reply to the merged ticket");
    let a3 = k.get(&admin, "Ticket", a_id, false).await.unwrap();
    assert!(bodies(&a3).contains(&"Any news?".to_string()), "on the survivor: {:?}", bodies(&a3));
    assert_eq!(bodies(&k.get(&admin, "Ticket", b_id, false).await.unwrap()).len(), 1, "none on the duplicate");
}
