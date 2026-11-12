//! What the helpdesk's integration suites share: a kernel on a freshly reset database, a workspace with the
//! Helpdesk package and, when asked, a stand-in for the Mail app's records (the real Mail is a different
//! repository), and small helpers. Each suite resets the database, as the others do; they run one after another.
#![allow(dead_code)]

use async_trait::async_trait;
use ferrum_kernel::secret::SecretKey;
use ferrum_kernel::tenant::NewTenant;
use ferrum_kernel::{Ctx, Db, DbConfig, Doc, DocHook, Error, HookCtx, HookRegistry, Kernel, KernelSettings, ListQuery, Result};
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

/// Mail's records as far as Helpdesk meets them (the comms package's `Communication` and `EmailAccount`).
const MAIL_PACKAGE: &str = "package: mailstub\nversion: 0.1.0\nlabel: Mail stand-in\ndescription: Test stand-in for the Mail app's records\ndepends: [core]\nmodules:\n  - {name: mailstub, label: Mail, icon: mail, order: 90, items: [Communication, EmailAccount]}\n";
const MAIL_ENTITIES: &str = r#"
- name: EmailAccount
  label: Email Account
  kind: master
  module: mailstub
  title_field: account_name
  fields:
    - {name: account_name, type: data, required: true, unique: true}
    - {name: email_address, type: email, required: true}
- name: Communication
  label: Communication
  kind: master
  module: mailstub
  title_field: subject
  fields:
    - {name: subject, type: data, max_length: 998}
    - {name: channel, type: select, options: [Email, SMS, Chat, Phone, Note, Meeting], default: Email, required: true}
    - {name: direction, type: select, options: [Sent, Received], default: Sent, required: true}
    - {name: status, type: select, options: [Queued, Sending, Sent, Failed, Received, Withheld]}
    - {name: email_account, type: link, to: EmailAccount}
    - {name: sender, type: data}
    - {name: sender_name, type: data}
    - {name: recipients, type: text}
    - {name: cc, type: text}
    - {name: content, type: text}
    - {name: text_content, type: text}
    - {name: attachments, type: json}
    - {name: reference_entity, type: data}
    - {name: reference_id, type: data}
    - {name: reply_to, type: link, to: Communication}
    - {name: message_id, type: data, unique: true}
    - {name: in_reply_to, type: data}
    - {name: mail_references, type: text}
    - {name: headers, type: json}
    - {name: seen, type: check}
"#;

/// What Mail's own hook does to a message it sends: a Message-ID, and Queued.
struct MailStub;

#[async_trait]
impl DocHook for MailStub {
    async fn before_validate(&self, _hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
        if doc.get("direction").and_then(Value::as_str) == Some("Sent") && doc.get("message_id").is_none_or(Value::is_null) {
            doc.insert("message_id".into(), json!(format!("{}@acme.test", Uuid::now_v7().simple())));
            doc.insert("status".into(), json!("Queued"));
        }
        Ok(())
    }
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
        packages.extend(
            ferrum_meta::catalog::load_embedded(&[("mailstub", "package.yaml", MAIL_PACKAGE), ("mailstub", "entities/mail.yaml", MAIL_ENTITIES)])
                .expect("the mail stand-in"),
        );
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
        hooks.register("Communication", Arc::new(MailStub));
    }
    let settings = KernelSettings { secret_key: Some(SecretKey::new([7u8; 32])), public_url: Some("https://help.example.test".into()), ..Default::default() };
    let k = Kernel::boot(db, catalog, hooks, settings, Some(Arc::new(Captured))).await.expect("boot");
    ferrum_domain::maintenance(&k).await.expect("partitions");
    k.set_file_store(Arc::new(MemFiles::default()));
    let mut packages = vec!["helpdesk".to_string()];
    if mail {
        packages.push("mailstub".into());
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
    Env { k, admin, tenant }
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
    pub async fn insert(&self, entity: &str, v: Value) -> Doc {
        self.k.insert(&self.admin, entity, obj(v)).await.unwrap_or_else(|e| panic!("insert {entity}: {e:?}"))
    }
    pub async fn update(&self, entity: &str, id: Uuid, v: Value) -> Doc {
        self.k.update(&self.admin, entity, id, obj(v), None).await.unwrap_or_else(|e| panic!("update {entity}: {e:?}"))
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
