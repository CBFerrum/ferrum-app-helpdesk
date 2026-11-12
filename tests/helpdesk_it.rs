//! End-to-end helpdesk test against a real PostgreSQL (`FERRUM_HELPDESK_TEST_DATABASE_URL`, default
//! `postgres://ferrum:ferrum@localhost:5432/ferrum_helpdesk_test`). The database is reset first.
//!
//! A company that sells serialized routers runs its helpdesk: tickets are routed to teams and
//! agents (round robin, load balancing, concurrent arrivals), measured against business-hours SLAs
//! (time zone, holiday, customer and group agreements, priority change, pauses, reopening),
//! answered, resolved, rated, closed, and auto-closed by maintenance, which also flags breaches.
//! Warranty claims read the serials' deliveries from the stock ledger, a maintenance contract plans
//! visits that are carried out once each, and the reports add it up within each user's permissions.

use chrono::{DateTime, Duration, Months, SubsecRound, Utc};
use ferrum_kernel::tenant::NewTenant;
use ferrum_kernel::{Ctx, Db, DbConfig, Doc, Error, HookRegistry, Kernel, KernelSettings, ListQuery, Result, SiteResponse};
use ferrum_meta::Catalog;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

const PASSWORD: &str = "Correct-Horse-9";

fn obj(v: Value) -> Doc {
    v.as_object().cloned().unwrap()
}

fn uid(doc: &Doc) -> Uuid {
    doc["id"].as_str().unwrap().parse().unwrap()
}

fn vid(v: &Value) -> Uuid {
    v["id"].as_str().unwrap().parse().unwrap()
}

fn ts(v: &Value) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(v.as_str().unwrap_or_else(|| panic!("not a timestamp: {v}"))).unwrap().with_timezone(&Utc)
}

fn dec(v: &Value) -> Decimal {
    Decimal::from_str(v.as_str().unwrap_or_else(|| panic!("not a decimal: {v}"))).unwrap()
}

/// Whether a result is a validation error on `field`.
fn refused<T: std::fmt::Debug>(r: &Result<T>, field: &str) -> bool {
    matches!(r, Err(Error::Validation(e)) if e.iter().any(|f| f.field == field))
}

/// Two moments within two seconds: the platform's targets take their own clock a moment after a hook does.
fn near(a: DateTime<Utc>, b: DateTime<Utc>) -> bool {
    (a - b).num_milliseconds().abs() < 2000
}

/// Marks the targets past their due moment, as the minute loop does, and brings the fields the inbox reads up to date.
async fn sweep(k: &Kernel) {
    k.sweep_targets_at(Utc::now()).await.unwrap();
    ferrum_app_helpdesk::maintenance(k).await.unwrap();
}

/// Time a ticket spent waiting (replied, on hold, resolved before a reopening), from its timeline.
fn waited(doc: &Doc) -> Duration {
    let tl: Vec<(String, DateTime<Utc>)> =
        doc["status_history"].as_array().unwrap().iter().map(|e| (e["status"].as_str().unwrap().to_string(), ts(&e["at"]))).collect();
    tl.windows(2).filter(|w| matches!(w[0].0.as_str(), "Replied" | "On Hold" | "Resolved")).fold(Duration::zero(), |sum, w| sum + (w[1].1 - w[0].1))
}

/// Report parameters for this year's tickets up to `today`, grouped by `group`.
fn span<'a>(today: &'a str, group: &'a str) -> [(&'a str, &'a str); 3] {
    [("from_date", "2026-01-01"), ("to_date", today), ("group_by", group)]
}

fn row<'a>(report: &'a Value, key: &str, value: &str) -> Option<&'a Value> {
    report["rows"].as_array().unwrap().iter().find(|r| r[key] == json!(value))
}

async fn fresh_kernel() -> Arc<Kernel> {
    let url = std::env::var("FERRUM_HELPDESK_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://ferrum:ferrum@localhost:5432/ferrum_helpdesk_test".into());
    let db = Db::connect(DbConfig { url, max_connections: 10, ..Default::default() }).await.expect("connect");
    sqlx::raw_sql("drop schema if exists public cascade; create schema public;").execute(&db.pool).await.expect("reset");
    let mut packages = ferrum_domain::packages().expect("platform packages");
    packages.extend(ferrum_app_erp::packages().expect("ERP's packages"));
    packages.extend(ferrum_app_helpdesk::packages().expect("app packages"));
    let catalog = Catalog::from_packages(packages).expect("catalog");
    let mut hooks = HookRegistry::default();
    ferrum_domain::register(&mut hooks);
    ferrum_app_erp::register(&mut hooks);
    for source in ferrum_app_erp::reports() {
        ferrum_domain::reports::add_source(source);
    }
    ferrum_app_helpdesk::register(&mut hooks);
    for source in ferrum_app_helpdesk::reports() {
        ferrum_domain::reports::add_source(source);
    }
    let settings = KernelSettings { public_url: Some("https://help.example.test".into()), ..Default::default() };
    let kernel = Kernel::boot(db, catalog, hooks, settings, Some(Arc::new(Captured))).await.expect("boot");
    kernel.set_file_store(Arc::new(MemFiles::default()));
    kernel
}

/// Mail the server sent, in order: (to, subject, html, names of the files attached).
static SENT: std::sync::Mutex<Vec<(String, String, String, Vec<String>)>> = std::sync::Mutex::new(Vec::new());

struct Captured;

#[async_trait::async_trait]
impl ferrum_kernel::Mailer for Captured {
    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<()> {
        SENT.lock().unwrap().push((to.into(), subject.into(), html.into(), vec![]));
        Ok(())
    }

    fn can_attach(&self) -> bool {
        true
    }

    async fn send_with(&self, to: &str, subject: &str, html: &str, attachments: &[ferrum_kernel::MailAttachment]) -> Result<()> {
        SENT.lock().unwrap().push((to.into(), subject.into(), html.into(), attachments.iter().map(|a| a.file_name.clone()).collect()));
        Ok(())
    }
}

/// An in-memory object store standing in for S3.
#[derive(Default)]
struct MemFiles(std::sync::Mutex<HashMap<String, Vec<u8>>>);

#[async_trait::async_trait]
impl ferrum_kernel::files::FileStore for MemFiles {
    async fn put(&self, key: &str, data: &[u8]) -> Result<()> {
        self.0.lock().unwrap().insert(key.to_string(), data.to_vec());
        Ok(())
    }
    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        self.0.lock().unwrap().get(key).cloned().ok_or_else(|| Error::NotFound("object".into()))
    }
    async fn delete(&self, key: &str) -> Result<()> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

/// Sends what is queued, as the server's dispatcher would.
async fn drain(k: &Kernel) {
    while k.dispatch_outbox_once().await.unwrap() > 0 {}
}

/// The mail sent to an address, in order.
fn sent_to(to: &str) -> Vec<(String, String, String, Vec<String>)> {
    SENT.lock().unwrap().iter().filter(|m| m.0 == to).cloned().collect()
}

#[derive(Clone)]
struct T {
    k: Arc<Kernel>,
    ctx: Ctx,
}

impl T {
    async fn insert(&self, entity: &str, v: Value) -> Doc {
        self.try_insert(entity, v).await.unwrap_or_else(|e| panic!("insert {entity}: {e:?}"))
    }
    async fn try_insert(&self, entity: &str, v: Value) -> Result<Doc> {
        let saved = self.k.insert(&self.ctx, entity, obj(v)).await?;
        self.fresh(entity, saved).await
    }
    /// A ticket as stored after its save: the service level the inbox reads is brought up to date just after the save itself.
    async fn fresh(&self, entity: &str, saved: Doc) -> Result<Doc> {
        if entity == "Ticket" { self.k.get(&self.ctx, entity, uid(&saved), false).await } else { Ok(saved) }
    }
    async fn update(&self, entity: &str, id: Uuid, v: Value) -> Doc {
        self.try_update(entity, id, v).await.unwrap_or_else(|e| panic!("update {entity}: {e:?}"))
    }
    async fn try_update(&self, entity: &str, id: Uuid, v: Value) -> Result<Doc> {
        let saved = self.k.update(&self.ctx, entity, id, obj(v), None).await?;
        self.fresh(entity, saved).await
    }
    async fn get(&self, entity: &str, id: Uuid) -> Doc {
        self.k.get(&self.ctx, entity, id, false).await.unwrap_or_else(|e| panic!("get {entity}: {e:?}"))
    }
    async fn submit(&self, entity: &str, id: Uuid) -> Doc {
        self.k.submit(&self.ctx, entity, id, None).await.unwrap_or_else(|e| panic!("submit {entity}: {e:?}"))
    }
    async fn action(&self, entity: &str, id: Uuid, name: &str, params: Value) -> Value {
        self.k.run_action(&self.ctx, entity, id, name, params).await.unwrap_or_else(|e| panic!("{entity}.{name}: {e:?}"))
    }
    async fn list(&self, entity: &str, filters: Value) -> Result<Vec<Doc>> {
        let q = ListQuery { filters: ferrum_kernel::query::parse_filters(&filters).unwrap(), limit: Some(500), ..Default::default() };
        Ok(self.k.list(&self.ctx, entity, &q).await?.rows)
    }
    async fn find_one(&self, entity: &str, filters: Value) -> Doc {
        let rows = self.list(entity, filters.clone()).await.unwrap();
        assert_eq!(rows.len(), 1, "{entity} {filters}");
        rows[0].clone()
    }
    /// Adds a message the way the API does: the existing rows (by id) plus the new one.
    async fn say(&self, ticket: Uuid, message: Value) -> Result<Doc> {
        let mut conversation = self.get("Ticket", ticket).await["conversation"].as_array().cloned().unwrap_or_default();
        conversation.push(message);
        self.try_update("Ticket", ticket, json!({ "conversation": conversation })).await
    }
    async fn report(&self, name: &str, params: &[(&str, &str)]) -> Result<Value> {
        let p: HashMap<String, String> = params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        ferrum_domain::reports::run(&self.k, &self.ctx, name, &p).await
    }
    async fn user(&self, email: &str, roles: &[&str], teams: &[Uuid]) -> T {
        let roles: Vec<String> = roles.iter().map(|r| r.to_string()).collect();
        let id = self.k.create_user(&self.ctx, email, email, Some(PASSWORD), &roles, false).await.expect("user");
        if !teams.is_empty() {
            self.k.set_user_scopes(&self.ctx, id, "SupportTeam", teams, None).await.expect("scopes");
        }
        T { k: self.k.clone(), ctx: self.k.load_ctx(self.ctx.tenant, id, None, "it".into()).await.expect("ctx") }
    }
    /// Rewrites stored values directly: the test's way of letting time pass.
    async fn sql(&self, sql: &'static str, id: Uuid) {
        let mut tx = self.k.db.tenant_tx(self.ctx.tenant).await.unwrap();
        sqlx::query(sql).bind(id).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    /// The workspace's settings record with `v` set: the one there is (the upkeep makes it), else a new one.
    async fn settings(&self, v: Value) -> Doc {
        match self.list("SupportSettings", json!({})).await.unwrap().first() {
            Some(d) => self.update("SupportSettings", uid(d), v).await,
            None => self.insert("SupportSettings", v).await,
        }
    }
    /// Runs a statement that takes the workspace as `$1`, as the engine.
    async fn exec(&self, sql: &'static str) {
        let mut tx = self.k.db.tenant_tx(self.ctx.tenant).await.unwrap();
        sqlx::query("select set_config('ferrum.system', 'on', true)").execute(&mut *tx).await.unwrap();
        sqlx::query(sql).bind(self.ctx.tenant).execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
    /// Titles of a user's in-app notifications, oldest first.
    async fn notifications(&self, user: Uuid) -> Vec<String> {
        let mut tx = self.k.db.tenant_tx(self.ctx.tenant).await.unwrap();
        sqlx::query_scalar("select title from notification where tenant_id = $1 and user_id = $2 order by id")
            .bind(self.ctx.tenant)
            .bind(user)
            .fetch_all(&mut *tx)
            .await
            .unwrap()
    }
    /// Outbox events of a ticket: `(topic, actor is null, payload doc)`.
    async fn events(&self, ticket: Uuid) -> Vec<(String, bool, Value)> {
        let mut tx = self.k.db.tenant_tx(self.ctx.tenant).await.unwrap();
        sqlx::query_as("select topic, payload->'actor' = 'null'::jsonb, payload->'doc' from outbox where tenant_id = $1 and record_id = $2 order by id")
            .bind(self.ctx.tenant)
            .bind(ticket)
            .fetch_all(&mut *tx)
            .await
            .unwrap()
    }
}

/// An answer as the server sends it: a stored file it names (`SiteResponse::file`) becomes its
/// body, as the server streams it from storage.
async fn served(k: &Kernel, tenant: Uuid, mut r: SiteResponse) -> SiteResponse {
    if let Some(i) = r.headers.iter().position(|(h, _)| h == ferrum_kernel::hooks::STREAM_FILE_HEADER) {
        let id: Uuid = r.headers.remove(i).1.parse().expect("a file id");
        assert!(r.body.is_empty(), "a named file is streamed, never read whole by the app");
        r.body = k.read_file(&Ctx::system(tenant, k.registry.schema(tenant).await.unwrap()), id).await.unwrap().2;
    }
    r
}

#[test]
fn support_end_to_end() {
    // Same runtime shape as the server: unoptimized async hook chains need more than 2 MiB.
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(16 << 20).build().unwrap();
    rt.block_on(async { tokio::spawn(scenario()).await.unwrap() });
}

async fn scenario() {
    let k = fresh_kernel().await;
    let tenant = k
        .create_tenant(NewTenant {
            slug: "sd".into(),
            name: "Support Desk".into(),
            admin_email: "admin@sd.test".into(),
            admin_password: PASSWORD.into(),
            admin_name: None,
            // With ERP: warranty claims read the deliveries, tickets link invoices.
            packages: vec!["helpdesk".into(), "helpdesk_erp".into()],
            settings: Some(json!({"fiscal_year_start_month": 4})),
        })
        .await
        .expect("tenant");
    let session = k.login("sd", "admin@sd.test", PASSWORD).await.expect("login");
    let t = T { k: k.clone(), ctx: k.load_ctx(tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx") };
    let today = Utc::now().date_naive();
    let today_s = today.to_string();
    let day = |offset: i64| (today + Duration::days(offset)).to_string();
    let h = Duration::hours;

    // ───────────── masters, stock and deliveries ─────────────
    let inr = t.find_one("Currency", json!({"code": "INR"})).await;
    let company = t.insert("Company", json!({"company_name": "Support Desk Pvt Ltd", "abbr": "SD", "default_currency": inr["id"]})).await;
    let cid = uid(&company).to_string();
    let stores = uid(&t.find_one("Warehouse", json!({"warehouse_name": "Stores", "company": cid})).await).to_string();
    let commercial = t.find_one("CustomerGroup", json!({"group_name": "Commercial"})).await;
    let individual = t.find_one("CustomerGroup", json!({"group_name": "Individual"})).await;
    let acme = uid(&t.insert("Customer", json!({"customer_name": "Acme Corp", "customer_group": commercial["id"], "email": "it@acme.test"})).await);
    let zed = uid(&t.insert("Customer", json!({"customer_name": "Zed Ltd", "customer_group": commercial["id"]})).await);
    let bob = uid(&t.insert("Customer", json!({"customer_name": "Bob Smith", "customer_group": individual["id"], "email": "bob@example.test"})).await);
    let products = t.find_one("ItemGroup", json!({"group_name": "Products"})).await;
    let nos = t.find_one("Uom", json!({"uom_name": "Nos"})).await;
    let router = uid(&t
        .insert(
            "Item",
            json!({"item_code": "ROUTER", "item_name": "Router X1", "item_group": products["id"], "stock_uom": nos["id"], "has_serial_no": true,
                   "standard_rate": "5000", "sup_warranty_days": 365}),
        )
        .await);
    let cable = uid(&t
        .insert("Item", json!({"item_code": "CABLE", "item_name": "Patch Cable", "item_group": products["id"], "stock_uom": nos["id"], "standard_rate": "100"}))
        .await);
    assert_eq!(t.get("Item", router).await["sup_warranty_days"], json!(365), "warranty period added to items by the support package");
    let receipt = t
        .insert(
            "StockEntry",
            json!({"company": cid, "purpose": "Material Receipt", "posting_date": "2025-01-01", "items": [
                {"item": router.to_string(), "qty": "3", "basic_rate": "1000", "serial_no": "RT-1\nRT-2\nRT-3", "target_warehouse": stores}]}),
        )
        .await;
    t.submit("StockEntry", uid(&receipt)).await;
    let deliver = |serial: &str, on: &str| {
        json!({"company": cid, "customer": acme.to_string(), "posting_date": on,
               "items": [{"item": router.to_string(), "qty": "1", "warehouse": stores, "serial_no": serial}]})
    };
    let old_delivery = t.insert("DeliveryNote", deliver("RT-2", "2025-02-01")).await;
    t.submit("DeliveryNote", uid(&old_delivery)).await;
    let recent_delivery = t.insert("DeliveryNote", deliver("RT-1", &day(-30))).await;
    t.submit("DeliveryNote", uid(&recent_delivery)).await;

    // ───────────── agreements ─────────────
    let weekdays = |from: &str, to: &str| -> Value {
        ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday"].iter().map(|d| json!({"weekday": d, "start_time": from, "end_time": to})).collect()
    };
    let targets =
        |list: &[(&str, &str, &str)]| -> Value { list.iter().map(|(p, r, z)| json!({"priority": p, "response_hours": r, "resolution_hours": z})).collect() };
    let standard = uid(&t
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Standard", "time_zone_offset": 330, "working_hours": weekdays("09:00", "18:00"),
                   "holidays": [{"holiday_date": "2026-10-26", "description": "Company holiday"}],
                   "priorities": targets(&[("Low", "16", "80"), ("Medium", "8", "40"), ("High", "4", "16"), ("Urgent", "1", "4")])}),
        )
        .await);
    let premium = uid(&t
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Acme Premium", "customer": acme.to_string(),
                   "priorities": targets(&[("Low", "4", "24"), ("Medium", "2", "8"), ("High", "1", "4"), ("Urgent", "0.5", "2")])}),
        )
        .await);
    let group_sla = uid(&t
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Commercial", "customer_group": commercial["id"], "time_zone_offset": 330, "working_hours": weekdays("08:00", "20:00"),
                   "priorities": targets(&[("Medium", "4", "24"), ("High", "2", "8")])}),
        )
        .await);
    let another_default = t.try_insert("ServiceLevelAgreement", json!({"sla_name": "Another", "priorities": targets(&[("Low", "1", "2")])})).await;
    assert!(matches!(another_default, Err(Error::Invalid(ref m)) if m.contains("Standard")), "{another_default:?}");
    let sla_with = |extra: Value| {
        let mut v = json!({"sla_name": "Broken", "customer": bob.to_string(), "priorities": targets(&[("Low", "1", "2")])});
        for (key, value) in extra.as_object().unwrap() {
            v[key] = value.clone();
        }
        v
    };
    let night =
        t.try_insert("ServiceLevelAgreement", sla_with(json!({"working_hours": [{"weekday": "Monday", "start_time": "22:00", "end_time": "06:00"}]}))).await;
    assert!(refused(&night, "end_time"), "a night shift is one row per day: {night:?}");
    assert!(refused(
        &t.try_insert("ServiceLevelAgreement", sla_with(json!({"working_hours": [{"weekday": "Monday", "start_time": "9am", "end_time": "17:00"}]}))).await,
        "start_time"
    ));
    assert!(refused(
        &t.try_insert("ServiceLevelAgreement", sla_with(json!({"priorities": targets(&[("Low", "1", "2"), ("Low", "2", "4")])}))).await,
        "priority"
    ));
    assert!(refused(
        &t.try_insert("ServiceLevelAgreement", sla_with(json!({"priorities": targets(&[("High", "4", "2")])}))).await,
        "resolution_after_response"
    ));

    // ───────────── teams and assignment rules ─────────────
    let member = |agent: &str| json!({ "agent": agent });
    let l1 = uid(&t
        .insert(
            "SupportTeam",
            json!({"team_name": "L1", "members": [member("a1@sd.test"), member("a2@sd.test"), {"agent": "a3@sd.test", "active": false},
                   {"agent": "a4@sd.test", "max_open_tickets": 1}]}),
        )
        .await);
    let l2 = uid(&t.insert("SupportTeam", json!({"team_name": "L2 Escalations", "members": [member("b1@sd.test"), member("b2@sd.test")]})).await);
    let chat = uid(&t.insert("SupportTeam", json!({"team_name": "Chat", "members": [member("c1@sd.test"), member("c2@sd.test"), member("c3@sd.test")]})).await);
    assert!(refused(&t.try_insert("SupportTeam", json!({"team_name": "Dup", "members": [member("x@sd.test"), member("X@sd.test")]})).await, "agent"));
    let rule = |name: &str, team: Uuid, condition: Value, kind: &str, priority: i64| json!({"rule_name": name, "team": team.to_string(), "condition": condition, "rule_type": kind, "priority": priority});
    t.insert("TicketAssignmentRule", rule("Urgent to L2", l2, json!("doc.priority == 'Urgent'"), "Load Balancing", 10)).await;
    t.insert("TicketAssignmentRule", rule("Chats", chat, json!("doc.channel == 'Chat'"), "Round Robin", 5)).await;
    let everything = uid(&t.insert("TicketAssignmentRule", rule("Everything else", l1, Value::Null, "Round Robin", 1)).await);
    assert!(refused(&t.try_insert("TicketAssignmentRule", rule("Broken", l1, json!("doc.priority =="), "Round Robin", 1)).await, "condition"));
    let agent = t.user("a1@sd.test", &["Support Agent"], &[l1]).await;

    // ───────────── tickets: routing and SLAs on business hours ─────────────
    // Friday 23 October 17:00 IST; Monday 26 is the Standard agreement's holiday.
    // The platform's targets start their clock when the ticket is saved: this one is saved as of when it was opened.
    let friday: DateTime<Utc> = "2026-10-23T11:30:00Z".parse().unwrap();
    k.pin_clock(Some(friday));
    let t1 =
        t.insert("Ticket", json!({"subject": "Cannot log in", "customer": bob.to_string(), "priority": "High", "opened_at": "2026-10-23T11:30:00Z"})).await;
    let t1id = uid(&t1);
    assert_eq!(t1["number"], json!("TKT-2026-000001"));
    assert_eq!((t1["team"].clone(), t1["assigned_to"].clone()), (json!(l1.to_string()), json!("a1@sd.test")));
    assert_eq!(t1["contact_email"], json!("bob@example.test"), "taken from the customer");
    assert_eq!(t1["service_level_agreement"], json!(standard.to_string()));
    assert_eq!(
        t1["response_by"],
        json!("2026-10-27T06:30:00.000Z"),
        "1h on Friday, the holiday skipped, 3h on Tuesday, on the calendar made from the default agreement"
    );
    assert_eq!(t1["resolution_by"], json!("2026-10-28T09:30:00.000Z"));
    assert_eq!(
        (t1["response_state"].clone(), t1["resolution_state"].clone()),
        (json!("running"), json!("running")),
        "the breach is the platform's, marked by its minute loop"
    );
    k.pin_clock(None);
    sweep(&k).await;
    let t1 = t.get("Ticket", t1id).await;
    assert_eq!((t1["sla_status"].clone(), t1["response_breached"].clone(), t1["resolution_breached"].clone()), (json!("Breached"), json!(true), json!(true)));
    assert_eq!((t1["response_state"].clone(), t1["resolution_state"].clone()), (json!("breached"), json!("breached")));
    let created = t.events(t1id).await;
    assert_eq!(created[0].0, "Ticket.created");
    assert_eq!(created[0].2["assigned_to"], json!("a1@sd.test"), "automations see the routed ticket");
    assert!(created.iter().any(|e| e.0 == "Ticket.updated" && e.2["response_state"] == json!("breached")), "the breach raises the ticket's updated event");
    assert_eq!(k.sweep_targets_at(Utc::now()).await.unwrap(), 0, "recorded once");
    assert!(refused(&t.try_update("Ticket", t1id, json!({"opened_at": "2026-10-22T10:00:00Z"})).await, "opened_at"));
    assert!(refused(&t.try_insert("Ticket", json!({"subject": "From the future", "opened_at": format!("{}T00:00:00Z", day(2))})).await, "opened_at"));
    assert_eq!(
        t.notifications(agent.ctx.user).await,
        ["TKT-2026-000001 Cannot log in was assigned to you", "SLA breached on TKT-2026-000001 Cannot log in"],
        "the assignee is told in the app, and of the breach once"
    );
    k.pin_clock(Some(friday));
    let nudged = t.say(t1id, json!({"author_kind": "Customer", "body": "Any news?"})).await.unwrap();
    k.pin_clock(None);
    assert_eq!((nudged["status"].clone(), nudged["sla_status"].clone()), (json!("Open"), json!("Breached")));

    k.pin_clock(Some(friday));
    let t2 =
        t.insert("Ticket", json!({"subject": "Invoice copy", "customer": zed.to_string(), "priority": "Medium", "opened_at": "2026-10-23T11:30:00Z"})).await;
    let t2id = uid(&t2);
    assert_eq!(t2["assigned_to"], json!("a2@sd.test"));
    assert_eq!(t2["service_level_agreement"], json!(group_sla.to_string()), "the customer group's agreement");
    assert_eq!(
        (t2["response_by"].clone(), t2["resolution_by"].clone()),
        (json!("2026-10-27T06:30:00.000Z"), json!("2026-10-29T08:30:00.000Z")),
        "the group's 4 and 24 hours, on the one calendar (a group's own hours are not a thing any more)"
    );
    k.pin_clock(Some(friday));
    let t2 = t.update("Ticket", t2id, json!({"priority": "Low"})).await;
    k.pin_clock(None);
    assert_eq!(t2["service_level_agreement"], json!(standard.to_string()), "the group agreement sets no Low target");
    assert_eq!((t2["response_by"].clone(), t2["resolution_by"].clone()), (json!("2026-10-28T09:30:00.000Z"), json!("2026-11-06T10:30:00.000Z")));
    assert_eq!(t2["assigned_to"], json!("a2@sd.test"), "assignment happens once");

    // From here tickets are opened now: the workspace's calendar is made round the clock, so the deadlines are plain elapsed time.
    t.exec("update logic_calendar set zone = 'UTC', weekends = '{}', holidays = '{}', hours = null, revision = revision + 1 where tenant_id = $1 and name = 'helpdesk_hours'").await;
    let t3 = t
        .insert(
            "Ticket",
            json!({"subject": "Router keeps rebooting", "customer": acme.to_string(), "priority": "High", "channel": "Portal",
                   "description": "Since the firmware update", "item": router.to_string(), "serial_no": "RT-1"}),
        )
        .await;
    let t3id = uid(&t3);
    assert_eq!(t3["assigned_to"], json!("a4@sd.test"), "a3 is inactive");
    assert_eq!(t3["service_level_agreement"], json!(premium.to_string()), "the customer's own agreement beats its group's");
    let opened = ts(&t3["opened_at"]);
    assert!(near(ts(&t3["response_by"]), opened + h(1)) && near(ts(&t3["resolution_by"]), opened + h(4)), "round the clock");
    assert_eq!(t3["sla_status"], json!("On Track"));

    let mut next = Vec::new();
    for subject in ["Password reset", "Change of address", "Close my account"] {
        next.push(t.insert("Ticket", json!({"subject": subject, "customer": bob.to_string(), "priority": "Low"})).await);
    }
    let who: Vec<Value> = next.iter().map(|d| d["assigned_to"].clone()).collect();
    assert_eq!(who, [json!("a1@sd.test"), json!("a2@sd.test"), json!("a1@sd.test")], "turns skip a3 (inactive) and a4 (at its limit of one open ticket)");
    let (t8, t9, t10) = (uid(&next[0]), uid(&next[1]), uid(&next[2]));
    assert_eq!(t.get("TicketAssignmentRule", everything).await["last_assigned_to"], json!("a1@sd.test"));

    let mut urgent = Vec::new();
    for customer in [bob, zed, bob] {
        urgent.push(t.insert("Ticket", json!({"subject": "Site down", "customer": customer.to_string(), "priority": "Urgent"})).await);
    }
    let who: Vec<Value> = urgent.iter().map(|d| d["assigned_to"].clone()).collect();
    assert_eq!(who, [json!("b1@sd.test"), json!("b2@sd.test"), json!("b1@sd.test")], "load balancing: fewest open tickets, ties in turn");
    assert!(urgent.iter().all(|d| d["team"] == json!(l2.to_string())));
    assert_eq!(urgent[1]["service_level_agreement"], json!(standard.to_string()), "the group agreement sets no Urgent target");
    let t5 = uid(&urgent[0]);

    let gate = Arc::new(tokio::sync::Barrier::new(6));
    let chats: Vec<_> = (0..6)
        .map(|i| {
            let (t, gate) = (t.clone(), gate.clone());
            tokio::spawn(async move {
                gate.wait().await;
                t.insert("Ticket", json!({"subject": format!("Chat {i}"), "customer": bob.to_string(), "priority": "Low", "channel": "Chat"})).await
            })
        })
        .collect();
    let mut per_agent: HashMap<String, usize> = HashMap::new();
    for c in chats {
        *per_agent.entry(c.await.unwrap()["assigned_to"].as_str().unwrap().to_string()).or_default() += 1;
    }
    assert_eq!(
        per_agent,
        HashMap::from([("c1@sd.test".to_string(), 2), ("c2@sd.test".to_string(), 2), ("c3@sd.test".to_string(), 2)]),
        "concurrent tickets take turns"
    );
    let chat_team =
        t.update("SupportTeam", chat, json!({"members": [member("c1@sd.test"), member("c2@sd.test"), member("c3@sd.test"), member("c4@sd.test")]})).await;
    assert_eq!(chat_team["members"][3]["active"], json!(true), "a member added later is active");

    // A ticket that was opened 5 hours ago and has been on hold for 4 of them, 1 of its 2 response hours used (saved as of when it
    // happened: the platform's clock starts when a ticket is saved).
    let sys = T { k: k.clone(), ctx: Ctx::system(tenant, k.registry.schema(tenant).await.unwrap()) };
    let now = Utc::now().trunc_subsecs(0);
    let ago = |d: Duration| (now - d).to_rfc3339();
    k.pin_clock(Some(now - h(5)));
    let t4 = sys
        .insert(
            "Ticket",
            json!({"subject": "Imported: VPN drops", "customer": acme.to_string(), "priority": "Medium", "team": l1.to_string(), "assigned_to": "a1@sd.test", "opened_at": ago(h(5))}),
        )
        .await;
    let t4id = uid(&t4);
    k.pin_clock(Some(now - h(4)));
    let t4 = sys.update("Ticket", t4id, json!({"status": "On Hold"})).await;
    k.pin_clock(None);
    assert_eq!(
        (t4["response_by"].clone(), t4["resolution_by"].clone(), t4["sla_status"].clone(), t4["response_state"].clone()),
        (Value::Null, Value::Null, json!("On Track"), json!("paused")),
        "deadlines wait while paused"
    );
    let resumed = t.update("Ticket", t4id, json!({"status": "Open"})).await;
    let back = ts(&resumed["status_history"].as_array().unwrap().last().unwrap()["at"]);
    assert!(near(ts(&resumed["response_by"]), back + h(1)) && near(ts(&resumed["resolution_by"]), back + h(7)), "the remaining time runs from the resume");

    // ───────────── the conversation drives the status ─────────────
    let made = t.action("Ticket", t3id, "make_warranty_claim", json!({})).await;
    let replied = t.say(t3id, json!({"body": "We are looking into it."})).await.unwrap();
    assert_eq!(replied["status"], json!("Replied"), "a public agent reply");
    assert_eq!((replied["conversation"][0]["author"].clone(), replied["conversation"][0]["author_kind"].clone()), (json!("admin@sd.test"), json!("Agent")));
    let first = ts(&replied["first_responded_at"]);
    assert!(first >= opened && first < opened + h(1));
    assert_eq!(
        (replied["response_breached"].clone(), replied["resolution_by"].clone(), replied["sla_status"].clone()),
        (json!(false), Value::Null, json!("On Track"))
    );
    let noted = t.say(t3id, json!({"body": "Check the power supply too.", "is_internal": true})).await.unwrap();
    assert_eq!(
        (noted["status"].clone(), noted["first_responded_at"].clone()),
        (json!("Replied"), replied["first_responded_at"].clone()),
        "internal notes change nothing"
    );
    let answered = t.say(t3id, json!({"author_kind": "Customer", "body": "It still reboots."})).await.unwrap();
    assert_eq!(answered["status"], json!("Open"), "the customer's reply hands the ticket back");
    assert_eq!(answered["conversation"][2]["author"], json!("it@acme.test"));
    assert!(near(ts(&answered["resolution_by"]), opened + h(4) + waited(&answered)), "time waiting for the customer is added");
    let mut edited = answered["conversation"].as_array().cloned().unwrap();
    edited[0]["body"] = json!("We fixed it already.");
    assert!(refused(&t.try_update("Ticket", t3id, json!({ "conversation": edited })).await, "body"), "sent messages are a record");
    let without_first = answered["conversation"].as_array().unwrap()[1..].to_vec();
    assert!(refused(&t.try_update("Ticket", t3id, json!({ "conversation": without_first })).await, "conversation"));
    assert!(refused(&t.say(t3id, json!({"author_kind": "Customer", "body": "psst", "is_internal": true})).await, "is_internal"));
    let held = t.update("Ticket", t3id, json!({"status": "On Hold"})).await;
    assert_eq!((held["resolution_by"].clone(), held["sla_status"].clone()), (Value::Null, json!("On Track")));
    let resumed = t.update("Ticket", t3id, json!({"status": "Open"})).await;
    assert!(near(ts(&resumed["resolution_by"]), opened + h(4) + waited(&resumed)), "paused time is added back");
    assert!(refused(&t.try_update("Ticket", t3id, json!({"status": "Resolved"})).await, "resolution_details"));
    assert!(refused(&t.try_update("Ticket", t3id, json!({"satisfaction_rating": 5})).await, "satisfaction_rating"), "no rating before the resolution");
    let resolved = t.update("Ticket", t3id, json!({"status": "Resolved", "resolution_details": "Rolled the firmware back to 1.2."})).await;
    assert_eq!((resolved["sla_status"].clone(), resolved["resolution_breached"].clone()), (json!("Fulfilled"), json!(false)));
    assert!(dec(&resolved["resolution_hours"]) < Decimal::ONE && resolved["first_response_hours"].is_string());
    assert!(refused(&t.try_update("Ticket", t3id, json!({"satisfaction_rating": 6})).await, "satisfaction_rating"));
    let reopened = t.say(t3id, json!({"author_kind": "Customer", "body": "It happened again this morning."})).await.unwrap();
    assert_eq!((reopened["status"].clone(), reopened["resolved_at"].clone(), reopened["sla_status"].clone()), (json!("Open"), Value::Null, json!("On Track")));
    assert!(near(ts(&reopened["resolution_by"]), opened + h(4) + waited(&reopened)), "the time it sat resolved does not count");
    t.update("Ticket", t3id, json!({"status": "Resolved", "satisfaction_rating": 4, "feedback": "Quick help"})).await;
    let closed = t.update("Ticket", t3id, json!({"status": "Closed"})).await;
    assert!(closed["closed_at"].is_string());
    assert_eq!(closed["sla_status"], json!("Fulfilled"));
    for change in [json!({"subject": "Edited"}), json!({"status": "Open"})] {
        let r = t.try_update("Ticket", t3id, change).await;
        assert!(matches!(r, Err(Error::Invalid(ref m)) if m.contains("closed ticket is final")), "{r:?}");
    }
    assert!(
        matches!(t.say(t3id, json!({"author_kind": "Customer", "body": "One more thing"})).await, Err(Error::Invalid(_))),
        "closed tickets take no messages"
    );
    assert_eq!(t.update("Ticket", t3id, json!({"satisfaction_rating": 5})).await["satisfaction_rating"], json!(5), "the rating may still come in");

    // ───────────── agents see and route within their teams ─────────────
    let visible = agent.list("Ticket", json!([])).await.unwrap();
    assert!(visible.len() >= 7 && visible.iter().all(|d| d["team"] == json!(l1.to_string())), "an agent sees the tickets of their teams");
    assert!(matches!(agent.k.get(&agent.ctx, "Ticket", t5, false).await, Err(Error::NotFound(_))), "not another team's");
    let phoned = agent.insert("Ticket", json!({"subject": "Printer jam", "customer": bob.to_string(), "priority": "Urgent", "channel": "Phone"})).await;
    assert_eq!((phoned["team"].clone(), phoned["assigned_to"].clone()), (json!(l1.to_string()), json!("a2@sd.test")), "routed within the agent's teams");
    let elsewhere = agent.try_insert("Ticket", json!({"subject": "For L2", "customer": bob.to_string(), "team": l2.to_string()})).await;
    assert!(matches!(elsewhere, Err(Error::Forbidden(_))), "{elsewhere:?}");
    let sla =
        agent.try_insert("ServiceLevelAgreement", json!({"sla_name": "Mine", "customer": bob.to_string(), "priorities": targets(&[("Low", "1", "2")])})).await;
    assert!(matches!(sla, Err(Error::Forbidden(_))), "agents cannot change agreements: {sla:?}");
    let clerk = t.user("clerk@sd.test", &["Stock User"], &[]).await;
    assert!(matches!(clerk.list("Ticket", json!([])).await, Err(Error::Forbidden(_))));
    assert!(matches!(clerk.report("support_ticket_summary", &[]).await, Err(Error::Forbidden(_))));

    // ───────────── knowledge base ─────────────
    let editor = t.user("kb@sd.test", &["Knowledge Base Editor"], &[]).await;
    let root = t.find_one("KbCategory", json!({"category_name": "All Categories"})).await;
    let routers = uid(&editor.insert("KbCategory", json!({"category_name": "Routers", "parent_category": root["id"]})).await);
    assert!(
        refused(&editor.try_insert("KbArticle", json!({"title": "Misfiled", "category": root["id"], "content": "x"})).await, "category"),
        "articles go into leaf categories"
    );
    let article = editor
        .insert("KbArticle", json!({"title": "Router reboots after a firmware update", "category": routers.to_string(), "content": "Roll back to 1.2."}))
        .await;
    let aid = uid(&article);
    assert_eq!((article["number"].clone(), article["status"].clone(), article["published_at"].clone()), (json!("KB-00001"), json!("Draft"), Value::Null));
    assert!(matches!(agent.k.run_action(&agent.ctx, "KbArticle", aid, "mark_helpful", json!({})).await, Err(Error::Invalid(_))), "drafts take no votes");
    let published = editor.update("KbArticle", aid, json!({"status": "Published"})).await;
    assert!(published["published_at"].is_string());
    for vote in ["mark_helpful", "mark_helpful", "mark_not_helpful"] {
        agent.action("KbArticle", aid, vote, json!({})).await;
    }
    editor.k.set_tags(&editor.ctx, "KbArticle", aid, &["router".into(), "firmware".into()]).await.expect("tags");
    assert_eq!(agent.list("KbArticle", json!([["_tag", "=", "firmware"]])).await.unwrap().len(), 1, "articles are found by their tags");
    let read = agent.update("KbArticle", aid, json!({"content": "Roll back to firmware 1.2, then factory-reset."})).await;
    assert_eq!((read["helpful_count"].clone(), read["not_helpful_count"].clone()), (json!(2), json!(1)));
    assert_eq!(read["published_at"], published["published_at"], "edits keep the publication date");

    // ───────────── warranty claims ─────────────
    let claim = |customer: Uuid, item: Uuid, serial: Option<&str>| json!({"customer": customer.to_string(), "item": item.to_string(), "serial_no": serial, "complaint": "Does not power on"});
    let wc1 = t.get("WarrantyClaim", vid(&made)).await;
    assert_eq!(
        (wc1["ticket"].clone(), wc1["serial_no"].clone(), wc1["complaint"].clone()),
        (json!(t3id.to_string()), json!("RT-1"), json!("Since the firmware update"))
    );
    assert_eq!((wc1["delivery_date"].clone(), wc1["warranty_expiry_date"].clone()), (json!(day(-30)), json!(day(335))), "the serial's delivery, plus 365 days");
    assert_eq!(wc1["warranty_status"], json!("Under Warranty"));
    assert!(wc1["delivery_reference"].as_str().unwrap().starts_with("Delivery Note DN-"), "{}", wc1["delivery_reference"]);
    let wc2 = t.insert("WarrantyClaim", claim(acme, router, Some("RT-2"))).await;
    assert_eq!((wc2["delivery_date"].clone(), wc2["warranty_status"].clone()), (json!("2025-02-01"), json!("Out of Warranty")));
    let wc3 = t.insert("WarrantyClaim", claim(bob, router, Some("RT-3"))).await;
    assert_eq!((wc3["delivery_date"].clone(), wc3["warranty_status"].clone()), (Value::Null, Value::Null), "still in stock: nothing to check against");
    let wc3 = t.update("WarrantyClaim", uid(&wc3), json!({"delivery_date": day(-10)})).await;
    assert_eq!(wc3["warranty_status"], json!("Under Warranty"), "a delivery date entered by hand counts when the ledger has none");
    assert_eq!(t.insert("WarrantyClaim", claim(bob, cable, None)).await["warranty_status"], json!("Out of Warranty"), "no warranty period on the item");
    let mut late = claim(bob, cable, None);
    late["delivery_date"] = json!(day(5));
    assert!(refused(&t.try_insert("WarrantyClaim", late).await, "delivered_before_claim"));
    assert!(refused(&t.try_update("WarrantyClaim", uid(&wc2), json!({"status": "Closed"})).await, "resolution"));
    let wc2 = t.update("WarrantyClaim", uid(&wc2), json!({"status": "Closed", "resolution": "Paid repair offered"})).await;
    assert_eq!(wc2["resolution_date"], json!(today_s));
    let breakdown = t.action("WarrantyClaim", uid(&wc1), "make_maintenance_visit", json!({})).await;
    let breakdown = t.get("MaintenanceVisit", vid(&breakdown)).await;
    assert_eq!((breakdown["maintenance_type"].clone(), breakdown["warranty_claim"].clone()), (json!("Breakdown"), wc1["id"].clone()));
    assert_eq!(breakdown["work_done"][0]["serial_no"], json!("RT-1"));

    // ───────────── maintenance contract ─────────────
    let start = today - Duration::days(10);
    let line = |item: Uuid, serial: Option<&str>, months: u32, periodicity: &str| {
        json!({"item": item.to_string(), "serial_no": serial, "start_date": start.to_string(),
               "end_date": start.checked_add_months(Months::new(months)).unwrap().to_string(), "periodicity": periodicity})
    };
    let ms = t
        .insert("MaintenanceSchedule", json!({"customer": acme.to_string(), "assigned_to": "eng@sd.test", "items": [line(router, Some("RT-2"), 11, "Quarterly"), line(cable, None, 2, "Monthly")]}))
        .await;
    let msid = uid(&ms);
    assert_eq!((ms["items"][0]["no_of_visits"].clone(), ms["items"][1]["no_of_visits"].clone(), ms["status"].clone()), (json!(4), json!(3), json!("Draft")));
    assert_eq!(ms["schedules"], json!([]));
    let ms = t.submit("MaintenanceSchedule", msid).await;
    let plan = ms["schedules"].as_array().unwrap().clone();
    assert_eq!((plan.len(), ms["status"].clone()), (7, json!("Active")));
    assert_eq!((plan[0]["item"].clone(), plan[0]["scheduled_date"].clone()), (json!(router.to_string()), json!(start.to_string())));
    assert_eq!(
        (plan[1]["item"].clone(), plan[2]["scheduled_date"].clone()),
        (json!(cable.to_string()), json!(start.checked_add_months(Months::new(1)).unwrap().to_string()))
    );
    assert!(plan.iter().all(|r| r["completion_status"] == json!("Pending") && r["assigned_to"] == json!("eng@sd.test")));
    assert_eq!(t.insert("WarrantyClaim", claim(acme, router, Some("RT-2"))).await["warranty_status"], json!("Under AMC"), "out of warranty but under contract");

    let v1 = vid(&t.action("MaintenanceSchedule", msid, "make_visit", json!({})).await);
    let v1doc = t.get("MaintenanceVisit", v1).await;
    assert_eq!(
        (v1doc["maintenance_type"].clone(), v1doc["schedule_detail"].clone()),
        (json!("Scheduled"), plan[0]["id"].clone()),
        "the earliest pending visit"
    );
    let same_row = |customer: Uuid, row: &Value| {
        json!({"customer": customer.to_string(), "maintenance_schedule": msid.to_string(), "schedule_detail": row["id"], "completion_status": "Partially Completed",
               "work_done": [{"item": router.to_string(), "work_done": "Second engineer"}]})
    };
    let v2 = uid(&t.insert("MaintenanceVisit", same_row(acme, &plan[0])).await);
    assert!(refused(&t.k.submit(&t.ctx, "MaintenanceVisit", v1, None).await, "completion_needed"));
    t.update("MaintenanceVisit", v1, json!({"completion_status": "Fully Completed", "customer_feedback": "Thanks"})).await;
    t.submit("MaintenanceVisit", v1).await;
    let planned = |ms: &Doc| ms["schedules"].as_array().unwrap().iter().find(|r| r["id"] == plan[0]["id"]).cloned();
    let done = planned(&t.get("MaintenanceSchedule", msid).await).unwrap();
    assert_eq!(
        (done["visit"].clone(), done["actual_date"].clone(), done["completion_status"].clone()),
        (json!(v1.to_string()), json!(today_s), json!("Fully Completed"))
    );
    let twice = t.k.submit(&t.ctx, "MaintenanceVisit", v2, None).await;
    assert!(refused(&twice, "schedule_detail"), "a planned visit is carried out once: {twice:?}");
    assert!(refused(&t.try_insert("MaintenanceVisit", same_row(acme, &plan[0])).await, "schedule_detail"));
    assert!(refused(&t.try_insert("MaintenanceVisit", same_row(bob, &plan[2])).await, "customer"));
    t.k.cancel(&t.ctx, "MaintenanceVisit", v1, None).await.expect("cancel visit");
    let freed = planned(&t.get("MaintenanceSchedule", msid).await).unwrap();
    assert_eq!((freed["visit"].clone(), freed["completion_status"].clone()), (Value::Null, json!("Pending")));
    t.submit("MaintenanceVisit", v2).await;
    assert_eq!(planned(&t.get("MaintenanceSchedule", msid).await).unwrap()["completion_status"], json!("Partially Completed"));
    assert!(refused(&t.k.run_action(&t.ctx, "MaintenanceSchedule", msid, "make_visit", json!({"schedule_detail": plan[0]["id"]})).await, "schedule_detail"));
    t.k.cancel(&t.ctx, "MaintenanceSchedule", msid, None).await.expect("cancel schedule");
    let ms = t.get("MaintenanceSchedule", msid).await;
    assert_eq!((ms["status"].clone(), ms["schedules"].as_array().unwrap().len()), (json!("Cancelled"), 1), "the visit made stays, the rest is dropped");
    assert!(matches!(t.k.run_action(&t.ctx, "MaintenanceSchedule", msid, "make_visit", json!({})).await, Err(Error::Invalid(_))));

    // ───────────── maintenance: breaches flagged, resolved tickets closed ─────────────
    // Time passes for the platform's targets: T5's targets ran out a week ago; T10's 16-hour Low response ran out yesterday.
    t.sql("update record_target set due_at = due_at - interval '7 days' where record_id = $1", t5).await;
    t.sql("update record_target set due_at = now() - interval '1 day' where record_id = $1 and target = 'respond'", t10).await;
    let flagged_before = t.events(t5).await.iter().filter(|e| e.1).count();
    sweep(&k).await;
    let t5doc = t.get("Ticket", t5).await;
    assert_eq!((t5doc["sla_status"].clone(), t5doc["response_breached"].clone()), (json!("Breached"), json!(true)));
    let flagged: Vec<_> = t.events(t5).await.into_iter().filter(|e| e.1).collect();
    assert_eq!(flagged.len(), flagged_before + 2, "each target's breach is published for automations and webhooks, once");
    assert!(flagged.iter().all(|e| e.0 == "Ticket.updated") && flagged.iter().any(|e| e.2["response_state"] == json!("breached")));
    sweep(&k).await;
    assert_eq!(t.events(t5).await.iter().filter(|e| e.1).count(), flagged.len(), "idempotent");
    assert_eq!(t.get("Ticket", t5).await["version"], t5doc["version"], "what the inbox reads is brought up to date without a new version");
    let told = t.notifications(agent.ctx.user).await;
    assert_eq!(told.iter().filter(|n| n.ends_with("was assigned to you")).count(), 4, "T1, T8, T10 and the imported one: {told:?}");
    assert_eq!(told.iter().filter(|n| n.starts_with("The customer replied on TKT-2026-000001")).count(), 1, "{told:?}");
    assert_eq!(told.iter().filter(|n| n.starts_with("SLA breached on") && n.ends_with("Close my account")).count(), 1, "{told:?}");

    t.update("Ticket", t2id, json!({"status": "Resolved", "resolution_details": "Sent the invoice copy", "satisfaction_rating": 2})).await;
    t.update("Ticket", t9, json!({"status": "Resolved", "resolution_details": "Address updated"})).await;
    t.sql("update e_ticket set resolved_at = resolved_at - interval '8 days' where id = $1", t2id).await;
    ferrum_app_helpdesk::maintenance(&k).await.expect("maintenance");
    let t2doc = t.get("Ticket", t2id).await;
    assert_eq!((t2doc["status"].clone(), t2doc["closed_at"].is_string()), (json!("Closed"), true), "resolved for over 7 days");
    assert_eq!(t.get("Ticket", t9).await["status"], json!("Resolved"), "still in the grace period");
    t.settings(json!({"auto_close_days": 30})).await;
    t.update("Ticket", t8, json!({"status": "Resolved", "resolution_details": "Done"})).await;
    t.sql("update e_ticket set resolved_at = resolved_at - interval '8 days' where id = $1", t8).await;
    ferrum_app_helpdesk::maintenance(&k).await.expect("maintenance");
    assert_eq!(t.get("Ticket", t8).await["status"], json!("Resolved"), "the tenant's grace period is 30 days");
    t.sql("update e_ticket set resolved_at = resolved_at - interval '31 days' where id = $1", t8).await;
    ferrum_app_helpdesk::maintenance(&k).await.expect("maintenance");
    assert_eq!(t.get("Ticket", t8).await["status"], json!("Closed"));

    // ───────────── reports ─────────────
    let summary = t.report("support_ticket_summary", &span(&today_s, "Status")).await.unwrap();
    assert_eq!(summary["totals"]["tickets"], json!(17));
    assert_eq!(
        (row(&summary, "group", "Closed").unwrap()["tickets"].clone(), row(&summary, "group", "Resolved").unwrap()["tickets"].clone()),
        (json!(3), json!(1))
    );
    let by_team = agent.report("support_ticket_summary", &span(&today_s, "Team")).await.unwrap();
    assert_eq!((by_team["totals"]["tickets"].clone(), by_team["rows"].as_array().unwrap().len()), (json!(8), 1), "an agent's reports cover their teams");

    let compliance = t.report("support_sla_compliance", &span(&today_s, "SLA")).await.unwrap();
    let premium_row = row(&compliance, "group", "Acme Premium").unwrap();
    assert_eq!((premium_row["tickets"].clone(), premium_row["response_met"].clone(), premium_row["response_pending"].clone()), (json!(2), json!(1), json!(1)));
    assert_eq!((premium_row["resolution_met"].clone(), premium_row["resolution_pct"].clone()), (json!(1), json!("100")));
    for r in compliance["rows"].as_array().unwrap() {
        assert!(r["response_pending"].as_i64().unwrap() >= 0 && r["resolution_pending"].as_i64().unwrap() >= 0, "{r}");
    }
    let standard_row = row(&compliance, "group", "Standard").unwrap();
    assert_eq!(standard_row["response_breached"], json!(4), "T1, T2 (answered only when resolved), the urgent one and T10");
    assert_eq!(
        (standard_row["response_met"].clone(), standard_row["response_pct"].clone()),
        (json!(2), json!("33.33")),
        "T8 and T9 were answered (resolved) in time"
    );

    let times = t.report("support_response_times", &span(&today_s, "Priority")).await.unwrap();
    let high = row(&times, "group", "High").unwrap();
    assert_eq!((high["tickets"].clone(), high["responded"].clone()), (json!(2), json!(1)));
    assert!(high["response_median"].is_string() && high["resolution_p90"].is_string());
    assert_eq!(times["totals"]["tickets"], json!(17));

    let load = t.report("support_agent_workload", &[]).await.unwrap();
    assert_eq!(row(&load, "agent", "a1@sd.test").unwrap()["open_tickets"], json!(3));
    assert_eq!(row(&load, "agent", "a1@sd.test").unwrap()["oldest_ticket"], t1["number"], "the oldest open ticket");
    assert_eq!(row(&load, "agent", "a4@sd.test").unwrap()["open_tickets"], json!(0), "idle members are listed");
    assert!(row(&load, "agent", "a3@sd.test").is_none(), "inactive members are not");
    for c in ["c1@sd.test", "c2@sd.test", "c3@sd.test"] {
        assert_eq!(row(&load, "agent", c).unwrap()["open_tickets"], json!(2));
    }
    let l2_load = t.report("support_agent_workload", &[("team", &l2.to_string())]).await.unwrap();
    assert_eq!((l2_load["totals"]["open_tickets"].clone(), l2_load["totals"]["breached"].clone()), (json!(3), json!(1)));

    let csat = t.report("support_customer_satisfaction", &span(&today_s, "Agent")).await.unwrap();
    assert_eq!(
        (csat["totals"]["rated"].clone(), csat["totals"]["average_rating"].clone(), csat["totals"]["satisfied_pct"].clone()),
        (json!(2), json!("3.5"), json!("50"))
    );
    assert_eq!(
        (row(&csat, "group", "a4@sd.test").unwrap()["average_rating"].clone(), row(&csat, "group", "a2@sd.test").unwrap()["star_2"].clone()),
        (json!("5"), json!(1))
    );

    helpdesk_alone(&k).await;
    portal(&k).await;
    replies_and_files(&k).await;
}

/// Helpdesk in a workspace without ERP (Ferrum docs/plan/apps-ui.md §13): customers and items are
/// the platform's, nothing of ERP is there, and tickets work.
async fn helpdesk_alone(k: &Arc<Kernel>) {
    let tenant = k
        .create_tenant(NewTenant {
            slug: "desk".into(),
            name: "Desk Only".into(),
            admin_email: "admin@desk.test".into(),
            admin_password: PASSWORD.into(),
            admin_name: None,
            packages: vec!["helpdesk".into()],
            settings: None,
        })
        .await
        .expect("tenant");
    let session = k.login("desk", "admin@desk.test", PASSWORD).await.expect("login");
    let t = T { k: k.clone(), ctx: k.load_ctx(tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx") };
    assert_eq!(t.ctx.schema.packages, ["core", "contacts", "parties", "products", "helpdesk"]);
    for erp in ["SalesInvoice", "SalesOrder", "Warehouse", "Account"] {
        assert!(t.ctx.schema.entity(erp).is_none(), "{erp} without ERP");
    }
    assert!(t.ctx.schema.entity("Ticket").unwrap().field("sales_invoice").is_none());
    let customer = t.insert("Customer", json!({"customer_name": "Walk-in Co"})).await;
    let ticket = t.insert("Ticket", json!({"subject": "Printer jammed", "customer": customer["id"], "priority": "Medium"})).await;
    assert_eq!(ticket["status"], json!("Open"));
}

/// A call to Helpdesk's endpoints as `who` (`/api/app/helpdesk/{path}`).
async fn call(k: &Kernel, who: Option<&Ctx>, method: &str, path: &str, body: Value) -> Result<Value> {
    let site = k.hooks.site("helpdesk").expect("helpdesk's endpoints").clone();
    let req = ferrum_kernel::SiteRequest {
        tenant: who.map(|c| c.tenant).unwrap_or_default(),
        user: who.cloned(),
        method: method.into(),
        path: format!("/{path}"),
        query: Default::default(),
        headers: Default::default(),
        body: if body.is_null() { vec![] } else { body.to_string().into_bytes() },
        client_ip: "203.0.113.9".into(),
    };
    let r = site.handle(k, req).await?;
    Ok(serde_json::from_slice(&r.body).unwrap())
}

/// The portal's "Your tickets" (Ferrum docs/plan/apps-ui.md §9.1) and agents' My work: a customer
/// with no roles raises, reads and answers only their own tickets, never sees internal notes, and
/// rates the help once it is resolved.
async fn portal(k: &Arc<Kernel>) {
    let session = k.login("desk", "admin@desk.test", PASSWORD).await.expect("login");
    let tenant = session.tenant;
    let t = T { k: k.clone(), ctx: k.load_ctx(tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx") };
    let customer = t.insert("Customer", json!({"customer_name": "Pia's Garage", "email": "pia@garage.test"})).await;
    let agent = t.user("agent@desk.test", &["Support Agent"], &[]).await;
    let pia_id = k.create_user(&t.ctx, "Pia@Garage.test", "Pia", None, &[], false).await.unwrap();
    k.set_user_kind(&t.ctx, pia_id, "portal").await.unwrap();
    let pia = k.load_ctx(tenant, pia_id, None, "it".into()).await.unwrap();
    assert!(pia.roles.is_empty() && !pia.is_admin);
    assert!(k.list(&pia, "Ticket", &ListQuery::default()).await.is_err(), "no rights of her own");

    // Raised from the portal: hers, from the portal, for the customer her address belongs to.
    let raised = call(k, Some(&pia), "POST", "portal/tickets", json!({"subject": "Brakes squeak", "description": "Since **Monday**.", "priority": "High"}))
        .await
        .unwrap();
    let id = vid(&raised);
    assert_eq!((raised["status"].as_str(), raised["label"].as_str(), raised["can_reply"].as_bool()), (Some("Open"), Some("With the team"), Some(true)));
    let doc = t.get("Ticket", id).await;
    assert_eq!((doc["channel"].as_str(), doc["contact_email"].as_str(), doc["contact_name"].as_str()), (Some("Portal"), Some("pia@garage.test"), Some("Pia")));
    assert_eq!(doc["customer"], customer["id"]);
    assert!(
        call(k, Some(&pia), "POST", "portal/tickets", json!({"subject": "x", "description": "y", "priority": "Urgent"})).await.is_err(),
        "customers do not choose Urgent"
    );
    assert!(call(k, Some(&pia), "POST", "portal/tickets", json!({"subject": "  ", "description": "y"})).await.is_err());
    assert!(matches!(call(k, None, "GET", "portal/tickets", Value::Null).await, Err(Error::Unauthorized(_))));

    // The team answers, and notes something for itself.
    t.update("Ticket", id, json!({"assigned_to": "agent@desk.test"})).await;
    agent.say(id, json!({"author_kind": "Agent", "body": "Which wheel?", "is_internal": false})).await.unwrap();
    agent.say(id, json!({"author_kind": "Agent", "body": "Probably the pads, again", "is_internal": true})).await.unwrap();
    let seen = call(k, Some(&pia), "GET", &format!("portal/tickets/{id}"), Value::Null).await.unwrap();
    let bodies: Vec<&str> = seen["messages"].as_array().unwrap().iter().map(|m| m["body"].as_str().unwrap()).collect();
    assert_eq!(bodies, ["Which wheel?"], "internal notes never reach the customer");
    assert_eq!(seen["messages"][0]["mine"], json!(false));
    assert_eq!(seen["label"].as_str(), Some("Waiting for your reply"));
    assert_eq!(seen["description"].as_str(), Some("Since **Monday**."));

    // Her reply goes back to the team.
    let replied = call(k, Some(&pia), "POST", &format!("portal/tickets/{id}/replies"), json!({"body": "Front left."})).await.unwrap();
    assert_eq!(replied["status"].as_str(), Some("Open"));
    let last = replied["messages"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((last["mine"].as_bool(), last["author"].as_str(), last["body"].as_str()), (Some(true), Some("You"), Some("Front left.")));
    let stored = t.get("Ticket", id).await;
    let row = stored["conversation"].as_array().unwrap().last().unwrap().clone();
    assert_eq!((row["author_kind"].as_str(), row["author"].as_str()), (Some("Customer"), Some("pia@garage.test")));

    // Someone else's ticket is not there for her.
    let other = t.insert("Ticket", json!({"subject": "Olaf's", "contact_email": "olaf@other.test", "priority": "Low"})).await;
    assert!(matches!(call(k, Some(&pia), "GET", &format!("portal/tickets/{}", uid(&other)), Value::Null).await, Err(Error::NotFound(_))));
    assert!(matches!(call(k, Some(&pia), "POST", &format!("portal/tickets/{}/replies", uid(&other)), json!({"body": "hi"})).await, Err(Error::NotFound(_))));
    let mine = call(k, Some(&pia), "GET", "portal/tickets", Value::Null).await.unwrap();
    assert_eq!(mine["tickets"].as_array().unwrap().iter().map(|x| x["subject"].as_str().unwrap()).collect::<Vec<_>>(), ["Brakes squeak"]);

    // My work: the agent's open tickets, High marked; nothing for her.
    let work = call(k, Some(&agent.ctx), "GET", "work", Value::Null).await.unwrap();
    let items = work["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{work}");
    assert_eq!((items[0]["title"].as_str(), items[0]["priority"].as_str()), (Some("Brakes squeak"), Some("High")));
    assert_eq!(items[0]["url"].as_str(), Some(format!("/helpdesk/t/{id}").as_str()));
    assert_eq!(call(k, Some(&pia), "GET", "work", Value::Null).await.unwrap()["items"], json!([]));

    // Rated once resolved, and only then.
    assert!(call(k, Some(&pia), "POST", &format!("portal/tickets/{id}/rating"), json!({"rating": 5})).await.is_err());
    t.update("Ticket", id, json!({"status": "Resolved", "resolution_details": "Pads replaced."})).await;
    let resolved = call(k, Some(&pia), "GET", &format!("portal/tickets/{id}"), Value::Null).await.unwrap();
    assert_eq!((resolved["resolution"].as_str(), resolved["can_rate"].as_bool()), (Some("Pads replaced."), Some(true)));
    assert!(call(k, Some(&pia), "POST", &format!("portal/tickets/{id}/rating"), json!({"rating": 6})).await.is_err());
    let rated = call(k, Some(&pia), "POST", &format!("portal/tickets/{id}/rating"), json!({"rating": 5, "feedback": "Quick, thanks"})).await.unwrap();
    assert_eq!((rated["rating"].as_i64(), rated["can_rate"].as_bool()), (Some(5), Some(false)));
    assert_eq!(t.get("Ticket", id).await["satisfaction_rating"], json!(5));
    assert_eq!(call(k, Some(&agent.ctx), "GET", "work", Value::Null).await.unwrap()["items"], json!([]), "resolved is no longer work");

    // ───────────── what a customer is entitled to ─────────────
    let covered = uid(&t.insert("Customer", json!({"customer_name": "Covered Ltd"})).await);
    let open_ticket = |subject: &str, opened: DateTime<Utc>| json!({"subject": subject, "customer": covered.to_string(), "opened_at": opened.to_rfc3339(), "priority": "Medium"});
    let now = Utc::now();
    let today = now.date_naive();
    assert_eq!(t.insert("Ticket", open_ticket("Before any contract", now)).await["entitlement"], json!("None"));
    let tight = uid(&t
        .insert(
            "ServiceLevelAgreement",
            json!({"sla_name": "Contracted", "priorities": [{"priority": "Medium", "response_hours": "1", "resolution_hours": "2"}]}),
        )
        .await);
    let start = today - Duration::days(30);
    t.insert("SupportContract", json!({"contract_name": "Lapsed", "customer": covered.to_string(), "start_date": (today - Duration::days(400)).to_string(), "end_date": (today - Duration::days(35)).to_string()})).await;
    let lapsed = t.insert("Ticket", open_ticket("Only a lapsed one", now)).await;
    assert_eq!(lapsed["entitlement"], json!("Expired"));
    let contract = uid(&t.insert("SupportContract", json!({"contract_name": "Gold", "customer": covered.to_string(), "start_date": start.to_string(), "service_level_agreement": tight.to_string(), "included_tickets": 2})).await);
    assert!(
        t.try_insert(
            "SupportContract",
            json!({"contract_name": "Backwards", "customer": covered.to_string(), "start_date": today.to_string(), "end_date": start.to_string()})
        )
        .await
        .is_err()
    );
    k.pin_clock(Some(now - Duration::days(3)));
    let first = t.insert("Ticket", open_ticket("Covered one", now - Duration::days(3))).await;
    k.pin_clock(None);
    assert_eq!((first["entitlement"].as_str(), first["support_contract"].as_str()), (Some("Covered"), Some(contract.to_string().as_str())));
    assert_eq!(first["service_level_agreement"], json!(tight.to_string()), "the contract's agreement applies, whatever else would");
    t.insert("Ticket", open_ticket("Covered two", now)).await;
    let third = t.insert("Ticket", open_ticket("One too many", now)).await;
    assert_eq!(third["entitlement"], json!("Exhausted"), "said, not refused: an agent decides");

    // ───────────── work outside entitlement waits for approval, where the workspace says so (FEO-030) ─────────────
    let settings = t.settings(json!({"outside_entitlement": "Hold for Approval"})).await;
    let helper = t.user("helper@sd.test", &["Support Agent"], &[]).await;
    let approver = t.user("approver@sd.test", &["Support Manager"], &[]).await;
    let held = t.insert("Ticket", open_ticket("Past the contract", now)).await;
    assert_eq!(
        (held["status"].as_str(), held["entitlement"].as_str(), held["entitlement_approval"].as_str()),
        (Some("On Hold"), Some("Exhausted"), Some("Waiting")),
        "{held:?}"
    );
    let held_id = uid(&held);
    let quiet = helper.say(held_id, json!({"author_kind": "Agent", "body": "On it", "is_internal": false})).await.unwrap_err();
    assert!(quiet.to_string().contains("waits for a Support Manager's approval"), "no reply to the customer meanwhile: {quiet}");
    helper.say(held_id, json!({"author_kind": "Agent", "body": "Asking the manager", "is_internal": true})).await.expect("a note can");
    assert!(helper.try_update("Ticket", held_id, json!({"status": "Open"})).await.is_err(), "only the decision moves it");
    let not_theirs = helper.k.run_action(&helper.ctx, "Ticket", held_id, "approve_outside_entitlement", json!({"reason": "Goodwill"})).await;
    assert!(matches!(not_theirs, Err(Error::Forbidden(_))), "an agent does not decide it: {not_theirs:?}");
    assert!(approver.k.run_action(&approver.ctx, "Ticket", held_id, "approve_outside_entitlement", json!({"reason": " "})).await.is_err(), "saying why");
    approver.action("Ticket", held_id, "approve_outside_entitlement", json!({"reason": "Their renewal is being signed"})).await;
    let approved = t.get("Ticket", held_id).await;
    assert_eq!(
        (approved["status"].as_str(), approved["entitlement_approval"].as_str(), approved["entitlement_decided_by"].as_str()),
        (Some("Open"), Some("Approved"), Some("approver@sd.test"))
    );
    helper.say(held_id, json!({"author_kind": "Agent", "body": "On it", "is_internal": false})).await.expect("now the customer hears from us");
    let declined = uid(&t.insert("Ticket", open_ticket("Another past the contract", now)).await);
    approver.action("Ticket", declined, "decline_outside_entitlement", json!({"reason": "No renewal is coming"})).await;
    let no = t.get("Ticket", declined).await;
    assert_eq!((no["status"].as_str(), no["entitlement_approval"].as_str()), (Some("Resolved"), Some("Declined")));
    assert!(no["resolution_details"].as_str().unwrap().contains("No renewal is coming"), "{no:?}");
    t.update("SupportSettings", uid(&settings), json!({"outside_entitlement": "Inform"})).await;

    // ───────────── escalations ─────────────
    let lead = t.user("lead@sd.test", &["Support Agent"], &[]).await;
    t.insert("EscalationRule", json!({"rule_name": "Tell the lead", "level": 1, "trigger": "Breached", "notify": "lead@sd.test"})).await;
    t.insert(
        "EscalationRule",
        json!({"rule_name": "Take it over", "level": 2, "trigger": "Breached", "assign_to": "lead@sd.test", "raise_priority_to": "Urgent"}),
    )
    .await;
    t.insert("EscalationRule", json!({"rule_name": "Switched off", "level": 3, "trigger": "Breached", "enabled": false, "assign_to": "nobody@sd.test"})).await;
    let late = uid(&first);
    k.sweep_targets_at(Utc::now()).await.unwrap();
    assert_eq!(t.get("Ticket", late).await["response_state"], json!("breached"), "the platform marked it");
    ferrum_app_helpdesk::maintenance(k).await.unwrap();
    let once = t.get("Ticket", late).await;
    assert_eq!((once["escalation_level"].as_i64(), once["assigned_to"].as_str()), (Some(1), None), "the first rule, and only it");
    assert!(t.notifications(lead.ctx.user).await.iter().any(|n| n.starts_with("Escalated")), "who it names is told");
    let version = once["version"].clone();
    ferrum_app_helpdesk::maintenance(k).await.unwrap();
    let twice = t.get("Ticket", late).await;
    assert_eq!(
        (twice["escalation_level"].as_i64(), twice["assigned_to"].as_str(), twice["priority"].as_str()),
        (Some(2), Some("lead@sd.test"), Some("Urgent")),
        "then the next"
    );
    assert_eq!(twice["version"], version, "an agent's open form stays valid");
    ferrum_app_helpdesk::maintenance(k).await.unwrap();
    assert_eq!(t.get("Ticket", late).await["escalation_level"], json!(2), "nothing further: the rest is off");
    let done = t.update("Ticket", late, json!({"status": "Resolved", "resolution_details": "Fixed."})).await;
    assert_eq!(done["escalation_level"], json!(2), "it stays on the record");

    // Two agents answering at once: the second is told, and nothing is lost.
    let busy = t.insert("Ticket", json!({"subject": "Two agents", "customer": covered.to_string()})).await;
    let v = busy["version"].as_i64();
    let say = |text: &str| obj(json!({"conversation": [{"body": text, "author_kind": "Agent"}]}));
    t.k.update(&t.ctx, "Ticket", uid(&busy), say("First"), v).await.expect("the first");
    assert!(matches!(t.k.update(&t.ctx, "Ticket", uid(&busy), say("Second"), v).await, Err(Error::Conflict(_))), "the second saw an older ticket");
}

/// A call to Helpdesk's endpoints that answers with a file, not JSON.
async fn call_raw(k: &Kernel, who: &Ctx, path: &str) -> Result<ferrum_kernel::SiteResponse> {
    let site = k.hooks.site("helpdesk").expect("helpdesk's endpoints").clone();
    let req = ferrum_kernel::SiteRequest {
        tenant: who.tenant,
        user: Some(who.clone()),
        method: "GET".into(),
        path: format!("/{path}"),
        query: Default::default(),
        headers: Default::default(),
        body: vec![],
        client_ip: "203.0.113.9".into(),
    };
    site.handle(k, req).await
}

/// What a file belongs to: (entity, record).
async fn file_home(t: &T, id: Uuid) -> (Option<String>, Option<Uuid>) {
    let mut tx = t.k.db.tenant_tx(t.ctx.tenant).await.unwrap();
    sqlx::query_as("select entity, record_id from file_object where tenant_id = $1 and id = $2").bind(t.ctx.tenant).bind(id).fetch_one(&mut *tx).await.unwrap()
}

/// What the interface needs of a conversation: a reply reaches the customer by email, with its files; the inbox's
/// fields follow what was said; a customer downloads what was sent to them and nothing the team kept to
/// itself; saved replies reach agents as their scope says.
async fn replies_and_files(k: &Arc<Kernel>) {
    let session = k.login("desk", "admin@desk.test", PASSWORD).await.expect("login");
    let t = T { k: k.clone(), ctx: k.load_ctx(session.tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx") };
    let priya = t.user("priya@desk.test", &["Support Agent"], &[]).await;
    let raj = t.user("raj@desk.test", &["Support Agent"], &[]).await;
    let boss = t.user("boss@desk.test", &["Support Manager"], &[]).await;

    // ───────────── a reply goes out by email ─────────────
    let opened = t
        .insert("Ticket", json!({"subject": "Printer jam <urgent>", "description": "It jams.", "contact_name": "Ada", "contact_email": "Ada@Shop.test", "priority": "Medium"}))
        .await;
    let ticket = uid(&opened);
    let number = opened["number"].as_str().unwrap().to_string();
    assert_eq!(opened["last_message_preview"], json!("It jams."), "before anyone writes, the request itself");
    assert!(opened["last_customer_message_at"].is_null() && opened["last_agent_message_at"].is_null());

    let answered = priya
        .say(
            ticket,
            json!({"author_kind": "Agent", "body": "Try the **rear tray**.\n\nSee [the guide](https://help.example.test/g) <b>now</b>", "is_internal": false}),
        )
        .await
        .unwrap();
    let reply = answered["conversation"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(reply["emailed"], json!(true), "the reply says it was emailed");
    assert_eq!(answered["status"], json!("Replied"));
    assert!(!answered["last_agent_message_at"].is_null() && answered["last_customer_message_at"].is_null());
    assert!(answered["last_message_preview"].as_str().unwrap().starts_with("Try the **rear tray**. See [the guide]"), "{}", answered["last_message_preview"]);
    assert!(sent_to("ada@shop.test").is_empty(), "queued with the save, sent by the dispatcher");
    drain(k).await;
    let mails = sent_to("ada@shop.test");
    assert_eq!(mails.len(), 1, "{mails:?}");
    let token = t.get("Ticket", ticket).await["mail_token"].as_str().unwrap().to_string();
    assert_eq!(mails[0].1, format!("Re: [{number}/{token}] Printer jam <urgent>"), "the subject names the ticket and carries its secret");
    let html = &mails[0].2;
    assert!(html.contains("<strong>rear tray</strong>") && html.contains("<a href=\"https://help.example.test/g\">the guide</a>"), "{html}");
    assert!(html.contains("&lt;b&gt;now&lt;/b&gt;") && !html.contains("<b>"), "what an agent typed is text: {html}");
    assert!(html.contains(&format!("https://help.example.test/open/Ticket/{ticket}")), "the customer is told where to follow it: {html}");

    // A note and the customer's own words are not mailed to the customer; the inbox's fields follow them.
    let noted = priya.say(ticket, json!({"author_kind": "Agent", "body": "Probably the roller", "is_internal": true})).await.unwrap();
    assert_eq!(noted["conversation"].as_array().unwrap().last().unwrap()["emailed"], json!(false));
    assert_eq!(noted["last_message_preview"], answered["last_message_preview"], "a note is not what the customer last heard");
    priya.say(ticket, json!({"author_kind": "Customer", "body": "Still jams.\nAnd now it squeaks."})).await.unwrap();
    let after = t.get("Ticket", ticket).await;
    assert_eq!(after["last_message_preview"], json!("Still jams. And now it squeaks."), "on one line");
    assert!(!after["last_customer_message_at"].is_null());
    drain(k).await;
    assert_eq!(sent_to("ada@shop.test").len(), 1, "nothing more went out");

    // Nobody to email: the reply is in the conversation, and says it went nowhere.
    let walk_in = t.insert("Ticket", json!({"subject": "Phone call", "priority": "Low"})).await;
    let called = priya.say(uid(&walk_in), json!({"author_kind": "Agent", "body": "Called back."})).await.unwrap();
    assert_eq!(called["conversation"][0]["emailed"], json!(false));
    let mails_before = SENT.lock().unwrap().len();
    drain(k).await;
    assert_eq!(SENT.lock().unwrap().len(), mails_before, "no address, no mail");

    // ───────────── files go with a reply ─────────────
    let fix = k.store_file(&priya.ctx, None, "fix.pdf", "application/pdf", b"%PDF-1.7 fix").await.unwrap();
    let fix_id: Uuid = fix["id"].as_str().unwrap().parse().unwrap();
    let sent = priya
        .say(ticket, json!({"author_kind": "Agent", "body": "The fix is attached.", "attachments": [{"id": fix_id.to_string(), "file_name": "innocent.txt", "size": 1}]}))
        .await
        .unwrap();
    let message = sent["conversation"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(
        message["attachments"],
        json!([{"id": fix_id.to_string(), "file_name": "fix.pdf", "content_type": "application/pdf", "size": 12}]),
        "described by the stored file, not by what the client said"
    );
    assert_eq!(file_home(&t, fix_id).await, (Some("Ticket".to_string()), Some(ticket)), "the file follows the ticket's access now");
    drain(k).await;
    let with_file = sent_to("ada@shop.test");
    assert_eq!((with_file.len(), with_file[1].3.clone()), (2, vec!["fix.pdf".to_string()]), "the file travels with the email");

    // What may not be sent: someone else's upload, a file of another record, one that does not exist, too many.
    let rajs = k.store_file(&raj.ctx, None, "private.pdf", "application/pdf", b"%PDF-1.7 raj").await.unwrap();
    assert!(
        refused(&priya.say(ticket, json!({"author_kind": "Agent", "body": "x", "attachments": [{"id": rajs["id"]}]})).await, "attachments"),
        "another agent's upload"
    );
    let elsewhere = k.store_file(&t.ctx, Some(("Ticket", uid(&walk_in))), "other.pdf", "application/pdf", b"%PDF-1.7 other").await.unwrap();
    assert!(
        refused(&priya.say(ticket, json!({"author_kind": "Agent", "body": "x", "attachments": [{"id": elsewhere["id"]}]})).await, "attachments"),
        "another ticket's file"
    );
    assert!(refused(
        &priya.say(ticket, json!({"author_kind": "Agent", "body": "x", "attachments": [{"id": Uuid::now_v7().to_string()}]})).await,
        "attachments"
    ));
    assert!(refused(&priya.say(ticket, json!({"author_kind": "Agent", "body": "x", "attachments": "fix.pdf"})).await, "attachments"));
    let many: Vec<Value> = (0..11).map(|_| json!({"id": Uuid::now_v7().to_string()})).collect();
    assert!(refused(&priya.say(ticket, json!({"author_kind": "Agent", "body": "x", "attachments": many})).await, "attachments"), "at most ten");
    assert!(
        refused(
            &t.try_insert(
                "Ticket",
                json!({"subject": "With a file", "conversation": [{"author_kind": "Customer", "body": "hi", "attachments": [{"id": fix_id.to_string()}]}]})
            )
            .await,
            "attachments"
        ),
        "a ticket that does not exist yet has nothing to attach to"
    );
    assert_eq!(file_home(&t, rajs["id"].as_str().unwrap().parse().unwrap()).await, (None, None), "a refused reply attaches nothing");

    // The interface sends the messages already there by id alone, the new one with its files, and takes the
    // ticket over in the same save: the old messages stay as they were.
    let cv = priya.k.store_file(&priya.ctx, None, "cv.txt", "text/plain", b"details").await.unwrap();
    let mut rows: Vec<Value> = t.get("Ticket", ticket).await["conversation"].as_array().unwrap().iter().map(|m| json!({"id": m["id"]})).collect();
    let before = rows.len();
    rows.push(json!({"author_kind": "Agent", "body": "As the page sends it.", "is_internal": false, "attachments": [{"id": cv["id"]}]}));
    let by_page = priya.try_update("Ticket", ticket, json!({"conversation": rows, "assigned_to": "priya@desk.test"})).await.unwrap();
    let kept = by_page["conversation"].as_array().unwrap();
    assert_eq!(
        (kept.len(), kept[0]["body"].as_str(), kept[before]["emailed"].clone()),
        (before + 1, Some("Try the **rear tray**.\n\nSee [the guide](https://help.example.test/g) <b>now</b>"), json!(true))
    );
    assert_eq!(kept[before]["attachments"][0]["file_name"], json!("cv.txt"));
    assert_eq!(by_page["assigned_to"], json!("priya@desk.test"));
    drain(k).await;
    assert_eq!(sent_to("ada@shop.test").len(), 3);

    // A note can carry a file too; it is not mailed.
    let memo = k.store_file(&priya.ctx, None, "memo.txt", "text/plain", b"for us").await.unwrap();
    let memo_id: Uuid = memo["id"].as_str().unwrap().parse().unwrap();
    priya.say(ticket, json!({"author_kind": "Agent", "body": "For us.", "is_internal": true, "attachments": [{"id": memo_id.to_string()}]})).await.unwrap();
    assert_eq!(file_home(&t, memo_id).await.0.as_deref(), Some("Ticket"));
    drain(k).await;
    assert_eq!(sent_to("ada@shop.test").len(), 3);

    // ───────────── a customer downloads what was sent to them ─────────────
    let dee_id = k.create_user(&t.ctx, "dee@shop.test", "Dee", None, &[], false).await.unwrap();
    k.set_user_kind(&t.ctx, dee_id, "portal").await.unwrap();
    let dee = k.load_ctx(session.tenant, dee_id, None, "it".into()).await.unwrap();
    let hers = uid(&t.insert("Ticket", json!({"subject": "Dee's order", "contact_email": "dee@shop.test", "priority": "Low"})).await);
    let public = k.store_file(&priya.ctx, None, "invoice.pdf", "application/pdf", b"%PDF-1.7 invoice").await.unwrap();
    let hidden = k.store_file(&priya.ctx, None, "cost.xlsx", "application/octet-stream", b"our margins").await.unwrap();
    priya.say(hers, json!({"author_kind": "Agent", "body": "Your invoice.", "attachments": [{"id": public["id"]}]})).await.unwrap();
    priya
        .say(hers, json!({"author_kind": "Agent", "body": "Do not send them this.", "is_internal": true, "attachments": [{"id": hidden["id"]}]}))
        .await
        .unwrap();
    let seen = call(k, Some(&dee), "GET", &format!("portal/tickets/{hers}"), Value::Null).await.unwrap();
    assert_eq!(seen["messages"].as_array().unwrap().len(), 1, "the note never reaches her");
    assert_eq!(seen["messages"][0]["files"], json!([{"id": public["id"], "file_name": "invoice.pdf", "size": 16}]));
    let got = served(k, dee.tenant, call_raw(k, &dee, &format!("portal/tickets/{hers}/files/{}", public["id"].as_str().unwrap())).await.unwrap()).await;
    assert_eq!((got.body.as_slice(), got.content_type.as_str()), (&b"%PDF-1.7 invoice"[..], "application/pdf"));
    assert!(got.headers.iter().any(|h| h.0 == "content-disposition" && h.1 == "attachment; filename=\"invoice.pdf\""), "{:?}", got.headers);
    for (what, ticket, file) in [
        ("a file on a note", hers, hidden["id"].as_str().unwrap().to_string()),
        ("another ticket's file", hers, fix_id.to_string()),
        ("not a file", hers, "nonsense".to_string()),
    ] {
        let r = call_raw(k, &dee, &format!("portal/tickets/{ticket}/files/{file}")).await;
        assert!(matches!(r, Err(Error::NotFound(_))), "{what}: {r:?}");
    }
    let r = call_raw(k, &dee, &format!("portal/tickets/{ticket}/files/{fix_id}")).await;
    assert!(matches!(r, Err(Error::NotFound(_))), "someone else's ticket: {r:?}");

    // ───────────── saved replies ─────────────
    let titles = |rows: &[Doc]| -> Vec<String> {
        let mut v: Vec<String> = rows.iter().map(|r| r["title"].as_str().unwrap().to_string()).collect();
        v.sort();
        v
    };
    let starters = ["Is this resolved", "Thanks for getting in touch", "We are looking into it"];
    assert_eq!(titles(&priya.list("SavedReply", json!({})).await.unwrap()), starters, "the starter replies are everyone's");
    let mine = priya.insert("SavedReply", json!({"title": "My sign-off", "body": "Best, {{agent.first_name}}"})).await;
    assert_eq!(mine["scope"], json!("Personal"));
    assert_eq!(priya.list("SavedReply", json!({})).await.unwrap().len(), 4);
    assert_eq!(titles(&raj.list("SavedReply", json!({})).await.unwrap()), starters, "a personal reply is its writer's only");
    assert!(raj.k.get(&raj.ctx, "SavedReply", uid(&mine), false).await.is_err());
    assert_eq!(boss.list("SavedReply", json!({})).await.unwrap().len(), 3, "not even a manager reads another's personal reply");
    assert_eq!(t.list("SavedReply", json!({})).await.unwrap().len(), 4, "an administrator reads them all");
    assert!(
        refused(&priya.try_insert("SavedReply", json!({"title": "For the front line", "scope": "Team", "body": "x"})).await, "team"),
        "a team's reply names its team"
    );
    let front = uid(&t.insert("SupportTeam", json!({"team_name": "Front line", "members": [{"agent": "priya@desk.test"}]})).await);
    priya.insert("SavedReply", json!({"title": "For the front line", "scope": "Team", "team": front.to_string(), "body": "x"})).await;
    assert_eq!(raj.list("SavedReply", json!({})).await.unwrap().len(), 4, "a team's reply is for agents with no team limit");

    let thanks = uid(&t.find_one("SavedReply", json!({"title": "Thanks for getting in touch"})).await);
    assert_eq!(priya.action("SavedReply", thanks, "used", json!({})).await["uses"], json!(1));
    assert_eq!(raj.action("SavedReply", thanks, "used", json!({})).await["uses"], json!(2), "counted for everyone it is shared with");
    assert!(priya.try_update("SavedReply", thanks, json!({"body": "changed"})).await.is_err(), "a shared reply is its writer's and the managers' to change");
    assert_eq!(boss.update("SavedReply", thanks, json!({"body": "Hi {{customer.first_name}}"})).await["body"], json!("Hi {{customer.first_name}}"));
}
