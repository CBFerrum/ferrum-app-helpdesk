//! Saved replies: how often each is used, so the composer can offer the ones people reach for first.

use async_trait::async_trait;
use ferrum_domain::util::{id, sys};
use ferrum_kernel::{Doc, DocHook, Error, HookCtx, Result};
use serde_json::{Value, json};

pub struct ReplyHook;

#[async_trait]
impl DocHook for ReplyHook {
    async fn action(&self, hc: &mut HookCtx<'_>, name: &str, doc: &Doc, params: &Value) -> Result<Option<Value>> {
        if name == "render" {
            return crate::macros::render_reply(hc, doc, params).await.map(Some);
        }
        if name != "used" {
            return Ok(None);
        }
        // The kernel loaded the reply locked for this action, so two agents using it at once both count.
        let reply = id(doc, "id").ok_or_else(|| Error::internal("saved reply without id"))?;
        let mut v = Doc::new();
        v.insert("uses".into(), json!(crate::int(doc, "uses") + 1));
        hc.kernel.set_values_tx(&sys(hc.ctx), hc.tx, "SavedReply", reply, v.clone()).await?;
        Ok(Some(Value::Object(v)))
    }
}
