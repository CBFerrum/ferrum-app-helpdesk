//! What the helpdesk's integration suites share: a kernel on a freshly reset database, a workspace with the
//! Helpdesk package and, when asked, the real Mail app (the `ferrum-app-comms` crate, with an account for the
//! workspace's mail domain and its inbound webhook), and small helpers. Each suite resets the database, as the others do; they run one after another.
#![allow(dead_code)]

use async_trait::async_trait;
use ferrum_kernel::secret::SecretKey;
use ferrum_kernel::tenant::NewTenant;
use ferrum_kernel::{Ctx, Db, DbConfig, Doc, Error, HookRegistry, Kernel, KernelSettings, ListQuery, Result};
use ferrum_meta::Catalog;
use serde_json::{Value, json};
use std::sync::Arc;
use uuid::Uuid;

pub const PASSWORD: &str = "Correct-Horse-9";

pub fn obj(v: Value) -> Doc {
    v.as_object().cloned().unwrap()
}

pub fn uid(d: &Doc) -> Uuid {
    d["id"].as_str().unwrap().parse().unwrap()
}

/// Mail the server sent through its own mailer, in order: (to, subject, html).
pub static SENT: std::sync::Mutex<Vec<(String, String, String)>> = std::sync::Mutex::new(Vec::new());

struct Captured;

#[async_trait]
impl ferrum_kernel::Mailer for Captured {
    async fn send(&self, to: &str, subject: &str, html: &str) -> Result<()> {
        SENT.lock().unwrap().push((to.into(), subject.into(), html.into()));
        Ok(())
    }
}

/// Sends what is queued, as the server's dispatcher would.
pub async fn drain(k: &Kernel) {
    while k.dispatch_outbox_once().await.unwrap() > 0 {}
}

pub fn sent_to(to: &str) -> Vec<(String, String, String)> {
    SENT.lock().unwrap().iter().filter(|m| m.0 == to).cloned().collect()
}

pub struct Env {
    pub k: Arc<Kernel>,
    pub admin: Ctx,
    pub tenant: Uuid,
    /// The secret of Mail's inbound webhook for the support account (with `mail`).
    pub webhook: String,
}

/// A kernel on a reset database and a workspace `slug` with Helpdesk (and, with `mail`, the Mail stand-in).
pub async fn boot(slug: &str, mail: bool, extra: Vec<ferrum_meta::Package>) -> Env {
    let url = std::env::var("FERRUM_HELPDESK_TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://ferrum:ferrum@localhost:5432/ferrum_helpdesk_test".into());
    let db = Db::connect(DbConfig { url, max_connections: 16, ..Default::default() }).await.expect("connect");
    sqlx::raw_sql("drop schema if exists public cascade; create schema public;").execute(&db.pool).await.expect("reset");
    let mut packages = ferrum_domain::packages().expect("platform packages");
    packages.extend(ferrum_app_erp::packages().expect("ERP's packages"));
    packages.extend(ferrum_app_helpdesk::packages().expect("app packages"));
    packages.extend(extra);
    if mail {
        packages.extend(ferrum_app_comms::packages().expect("Mail's packages"));
    }
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
    if mail {
        ferrum_app_comms::register(&mut hooks);
    }
    let settings = KernelSettings { secret_key: Some(SecretKey::new([7u8; 32])), public_url: Some("https://help.example.test".into()), ..Default::default() };
    let k = Kernel::boot(db, catalog, hooks, settings, Some(Arc::new(Captured))).await.expect("boot");
    ferrum_domain::maintenance(&k).await.expect("partitions");
    k.set_file_store(Arc::new(MemFiles::default()));
    let mut packages = vec!["helpdesk".to_string()];
    if mail {
        packages.push("comms".into());
    }
    let tenant = k
        .create_tenant(NewTenant {
            slug: slug.into(),
            name: "Acme Support".into(),
            admin_email: format!("admin@{slug}.test"),
            admin_password: PASSWORD.into(),
            admin_name: None,
            packages,
            settings: None,
        })
        .await
        .expect("tenant");
    let session = k.login(slug, &format!("admin@{slug}.test"), PASSWORD).await.expect("login");
    let admin = k.load_ctx(tenant, session.user, Some(session.token_version), "it".into()).await.expect("ctx");
    let mut webhook = String::new();
    if mail {
        // The workspace's mail domain is the support account's: acme.test. Mail receives through its webhook.
        let account = k
            .insert(
                &admin,
                "EmailAccount",
                obj(json!({"account_name": "Support", "email_address": "support@acme.test", "default_outgoing": true, "smtp_host": "127.0.0.1", "smtp_security": "None"})),
            )
            .await
            .expect("Mail's account");
        webhook = k.run_action(&admin, "EmailAccount", uid(&account), "rotate_inbound_token", json!({})).await.expect("a webhook")["token"]
            .as_str()
            .unwrap()
            .to_string();
    }
    Env { k, admin, tenant, webhook }
}

/// An in-memory object store standing in for S3.
#[derive(Default)]
pub struct MemFiles(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);

#[async_trait]
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

impl Env {
    /// A mail arriving at Mail's webhook as a mail server hands it over: raw, as RFC 822. Mail's answer (`status`, `id`).
    pub async fn post_mail(&self, raw: &str) -> Value {
        ferrum_app_comms::inbound::webhook(&self.k, self.tenant, &self.webhook, Some("message/rfc822"), raw.as_bytes()).await.expect("Mail takes it")
    }

    pub async fn insert(&self, entity: &str, v: Value) -> Doc {
        self.k.insert(&self.admin, entity, obj(v)).await.unwrap_or_else(|e| panic!("insert {entity}: {e:?}"))
    }
    pub async fn update(&self, entity: &str, id: Uuid, v: Value) -> Doc {
        self.k.update(&self.admin, entity, id, obj(v), None).await.unwrap_or_else(|e| panic!("update {entity}: {e:?}"))
    }
    /// The workspace's settings record with `v` set: the one there is (the upkeep may have made it), else a new one.
    pub async fn settings(&self, v: Value) -> Uuid {
        match self.list("SupportSettings", json!({})).await.first() {
            Some(d) => {
                self.update("SupportSettings", uid(d), v).await;
                uid(d)
            }
            None => uid(&self.insert("SupportSettings", v).await),
        }
    }
    pub async fn get(&self, entity: &str, id: Uuid) -> Doc {
        self.k.get(&self.admin, entity, id, false).await.unwrap_or_else(|e| panic!("get {entity}: {e:?}"))
    }
    pub async fn list(&self, entity: &str, filters: Value) -> Vec<Doc> {
        let q = ListQuery { filters: ferrum_kernel::query::parse_filters(&filters).unwrap(), limit: Some(500), ..Default::default() };
        self.k.list(&self.admin, entity, &q).await.unwrap().rows
    }
    pub async fn action(&self, ctx: &Ctx, entity: &str, id: Uuid, name: &str, params: Value) -> Result<Value> {
        self.k.run_action(ctx, entity, id, name, params).await
    }
    pub async fn user(&self, email: &str, roles: &[&str]) -> Ctx {
        let roles: Vec<String> = roles.iter().map(|r| r.to_string()).collect();
        let id = self.k.create_user(&self.admin, email, email, None, &roles, false).await.expect("user");
        self.k.load_ctx(self.tenant, id, None, "it".into()).await.expect("ctx")
    }
    /// A portal person (outside the workspace): holds no role.
    pub async fn portal_user(&self, email: &str) -> Ctx {
        let id = self.k.create_user(&self.admin, email, email, None, &[], false).await.expect("user");
        self.k.set_user_kind(&self.admin, id, "portal").await.expect("kind");
        self.k.load_ctx(self.tenant, id, None, "it".into()).await.expect("ctx")
    }
    pub async fn sql<T: Send + Unpin + for<'r> sqlx::Decode<'r, sqlx::Postgres> + sqlx::Type<sqlx::Postgres>>(
        &self,
        sql: &'static str,
        binds: &[Uuid],
    ) -> Option<T> {
        let mut tx = self.k.db.tenant_tx(self.tenant).await.unwrap();
        let mut q = sqlx::query_scalar::<_, T>(sql).bind(self.tenant);
        for b in binds {
            q = q.bind(*b);
        }
        let r = q.fetch_optional(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
        r
    }
    pub async fn exec(&self, sql: &'static str, binds: &[Uuid]) {
        let mut tx = self.k.db.tenant_tx(self.tenant).await.unwrap();
        sqlx::query("select set_config('ferrum.system', 'on', true)").execute(&mut *tx).await.unwrap();
        let mut q = sqlx::query(sql).bind(self.tenant);
        for b in binds {
            q = q.bind(*b);
        }
        q.execute(&mut *tx).await.unwrap();
        tx.commit().await.unwrap();
    }
}

/// Runs a suite's scenario the way the server runs: a thread with room for unoptimized hook chains.
pub fn run<F: std::future::Future<Output = ()> + Send + 'static>(scenario: F) {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().thread_stack_size(16 << 20).build().unwrap();
    rt.block_on(async { tokio::spawn(scenario).await.unwrap() });
}

/// A call to Helpdesk's own endpoints (`/api/app/helpdesk/…`) as `who`, or as no one (a public page).
pub async fn call(k: &Kernel, who: Option<&Ctx>, tenant: Uuid, method: &str, path: &str, body: Value) -> Result<Value> {
    let site = k.hooks.site("helpdesk").expect("helpdesk's endpoints").clone();
    let (path, query) = match path.split_once('?') {
        Some((p, q)) => (p, q.split('&').filter_map(|kv| kv.split_once('=')).map(|(k, v)| (k.to_string(), json!(v.replace("%20", " ")))).collect()),
        None => (path, serde_json::Map::new()),
    };
    let req = ferrum_kernel::SiteRequest {
        tenant,
        user: who.cloned(),
        method: method.into(),
        path: format!("/{path}"),
        query,
        headers: Default::default(),
        body: if body.is_null() { vec![] } else { body.to_string().into_bytes() },
        client_ip: "203.0.113.9".into(),
    };
    let r = site.handle(k, req).await?;
    Ok(serde_json::from_slice(&r.body).unwrap_or(Value::Null))
}
