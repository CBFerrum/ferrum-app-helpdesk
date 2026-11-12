//! Asking how the help was, and what a rating does.
//!
//! When Support Settings say so, the customer of a solved ticket gets a message (the template
//! `helpdesk.survey`) with a signed link to the ticket's `rate` action: the platform's public page, one use, a
//! limited time, never a sign-in. The rating and comment are kept on the ticket once; a low rating can reopen
//! a ticket that is still only resolved and tell a lead. The rating in the portal does the same
//! ([`record`]). Timing and wording are settings and templates.

use ferrum_domain::util::{id, s, sys};
use ferrum_kernel::action_link::LinkSpec;
use ferrum_kernel::messages::{self, Message, Recipient};
use ferrum_kernel::{Ctx, Doc, Error, HookCtx, Kernel, Result, Tx};
use serde_json::{Map, Value, json};
use uuid::Uuid;

/// What the settings say (defaults while the record is not saved).
struct Survey {
    enabled: bool,
    delay_hours: i64,
    link_days: i64,
    reopen_at_or_below: i64,
    alert_at_or_below: i64,
    alert_to: Option<String>,
}

async fn settings(tx: &mut Tx, tenant: Uuid) -> Result<Survey> {
    type Row = (Option<bool>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<String>);
    let row: Option<Row> = sqlx::query_as(
        "select survey_enabled, survey_delay_hours, survey_link_days, survey_reopen_at_or_below, survey_alert_at_or_below, survey_alert_to::text from e_support_settings where tenant_id = $1 limit 1",
    )
    .bind(tenant)
    .fetch_optional(&mut **tx)
    .await?;
    let r = row.unwrap_or_default();
    Ok(Survey {
        enabled: r.0.unwrap_or(false),
        delay_hours: r.1.unwrap_or(1),
        link_days: r.2.unwrap_or(30),
        reopen_at_or_below: r.3.unwrap_or(0),
        alert_at_or_below: r.4.unwrap_or(0),
        alert_to: r.5.filter(|a| !a.is_empty()),
    })
}

/// Keeps a rating, once: the ticket must be solved and unrated. A low rating reopens a ticket that is only resolved (a
/// closed one is final) and tells the lead, as the settings say. As the system, after the caller's own checks.
pub async fn record(k: &Kernel, ctx: &Ctx, tx: &mut Tx, ticket: &Doc, rating: i64, feedback: Option<String>) -> Result<()> {
    let sctx = sys(ctx);
    let ticket_id = id(ticket, "id").ok_or_else(|| Error::internal("a ticket without an id"))?;
    if !(1..=5).contains(&rating) {
        return Err(Error::field("rating", "from 1 to 5"));
    }
    if feedback.as_deref().is_some_and(|f| f.chars().count() > 2000) {
        return Err(Error::field("feedback", "at most 2000 characters"));
    }
    let status = s(ticket, "status").unwrap_or_default();
    if !matches!(status, "Resolved" | "Closed") {
        return Err(Error::invalid("a ticket is rated once it is resolved"));
    }
    if ticket.get("satisfaction_rating").and_then(Value::as_i64).is_some_and(|r| r > 0) {
        return Err(Error::invalid("this ticket has been rated already"));
    }
    let st = settings(tx, ctx.tenant).await?;
    let mut patch = Doc::new();
    patch.insert("satisfaction_rating".into(), json!(rating));
    patch.insert("feedback".into(), json!(feedback));
    patch.insert("rated_at".into(), json!(chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
    k.update_tx(&sctx, tx, "Ticket", ticket_id, patch, None).await?;
    // A rating is given on a solved ticket; reopening it is a second step, which the ticket's own rules then judge.
    let mut saved = k.get_tx(&sctx, tx, "Ticket", ticket_id, false).await?;
    if status == "Resolved" && st.reopen_at_or_below > 0 && rating <= st.reopen_at_or_below {
        let mut reopen = Doc::new();
        reopen.insert("status".into(), json!("Open"));
        saved = k.update_tx(&sctx, tx, "Ticket", ticket_id, reopen, None).await?;
    }
    if st.alert_at_or_below > 0 && rating <= st.alert_at_or_below {
        if let Some(agent) = s(&saved, "assigned_to") {
            let label = format!("{} {}", s(&saved, "number").unwrap_or("Ticket"), s(&saved, "subject").unwrap_or_default());
            crate::ticket::notify_agent_public(tx, ctx.tenant, agent, &format!("Rated {rating} out of 5: {label}"), ticket_id).await?;
        }
        if let Some(lead) = st.alert_to {
            let mut values = Map::new();
            values.insert("rating".into(), json!(rating));
            let m = Message {
                template: "helpdesk.survey_alert".into(),
                entity: Some("Ticket".into()),
                record: Some(ticket_id),
                to: vec![Recipient::address(&lead)],
                values,
                ..Default::default()
            };
            messages::send(k, tx, &sctx, &m).await?;
        }
    }
    Ok(())
}

/// `rate` as an action (the signed link's, or a button): `rating` and `feedback`.
pub async fn rate(hc: &mut HookCtx<'_>, ticket: &Doc, params: &Value) -> Result<Value> {
    let rating = params.get("rating").and_then(Value::as_i64).ok_or_else(|| Error::field("rating", "from 1 to 5"))?;
    let feedback = params.get("feedback").and_then(Value::as_str).map(str::trim).filter(|f| !f.is_empty()).map(str::to_string);
    record(hc.kernel, hc.ctx, hc.tx, ticket, rating, feedback).await?;
    Ok(json!({"rated": rating}))
}

/// Sends the survey for tickets solved `survey_delay_hours` ago (and not longer ago than the link lasts), once each:
/// a ticket is claimed by the one statement that marks it, so two servers never send twice. Returns how many were sent.
pub async fn send_due(k: &Kernel, ctx: &Ctx) -> Result<usize> {
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    let st = settings(&mut tx, ctx.tenant).await?;
    if !st.enabled {
        tx.commit().await?;
        return Ok(0);
    }
    let due: Vec<Uuid> = sqlx::query_scalar(
        "select id from e_ticket where tenant_id = $1 and status in ('Resolved', 'Closed') and survey_sent_at is null and coalesce(satisfaction_rating, 0) = 0 \
         and contact_email is not null and merged_into is null and resolved_at <= now() - make_interval(hours => $2) \
         and resolved_at > now() - make_interval(days => $3) order by resolved_at limit 100",
    )
    .bind(ctx.tenant)
    .bind(st.delay_hours as i32)
    .bind(st.link_days as i32)
    .fetch_all(&mut *tx)
    .await?;
    tx.commit().await?;
    let sctx = sys(ctx);
    let mut sent = 0;
    for ticket in due {
        let mut tx = k.db.tenant_tx(ctx.tenant).await?;
        let claimed: Option<Uuid> =
            sqlx::query_scalar("update e_ticket set survey_sent_at = now() where tenant_id = $1 and id = $2 and survey_sent_at is null returning id")
                .bind(ctx.tenant)
                .bind(ticket)
                .fetch_optional(&mut *tx)
                .await?;
        if claimed.is_none() {
            continue;
        }
        let doc = k.get_tx(&sctx, &mut tx, "Ticket", ticket, false).await?;
        let Some(to) = s(&doc, "contact_email").map(str::to_string) else { continue };
        let spec = LinkSpec {
            actions: vec!["rate".into()],
            show: vec!["subject".into()],
            uses: Some(1),
            expires_at: Some(chrono::Utc::now() + chrono::Duration::days(st.link_days)),
            recipient: Some(to.clone()),
            ..Default::default()
        };
        let link = k.issue_link(&sctx, &mut tx, "Ticket", ticket, &spec).await?;
        let mut values = Map::new();
        values.insert("url".into(), json!(link.url));
        let m = Message {
            template: "helpdesk.survey".into(),
            entity: Some("Ticket".into()),
            record: Some(ticket),
            to: vec![Recipient::address(&to)],
            values,
            secret: vec!["url".into()],
            ..Default::default()
        };
        messages::send(k, &mut tx, &sctx, &m).await?;
        tx.commit().await?;
        sent += 1;
    }
    Ok(sent)
}
