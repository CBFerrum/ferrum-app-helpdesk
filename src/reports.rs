//! Helpdesk reports: ticket volume, SLA compliance, response and resolution times in business
//! hours, agent workload and customer satisfaction. Every query carries the caller's record scopes
//! on tickets, so an agent limited to some teams sees those teams' numbers only. A report groups, filters and shows only
//! the fields the caller may read (S9 K0-7): a grouping or filter on another is refused, a default they cannot read gives
//! way to the next they can, and a column of another is left out of the columns, the rows and the totals.

use crate::child_table;
use async_trait::async_trait;
use chrono::NaiveDate;
use ferrum_domain::reports::{Params, ReportCompany, ReportSource, col, param_def, period, read_tx, readable_choice, readable_columns};
use ferrum_domain::util::money;
use ferrum_kernel::{Action, Ctx, Error, Kernel, Result, Tx, row_scope};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::{Postgres, QueryBuilder};
use std::collections::HashMap;
use uuid::Uuid;

pub struct SupportReports;

const T: &str = "Ticket";

/// A grouping: its label, the fields it reads (`(entity, field)`: the ticket's own and the linked record's name it shows)
/// and its SQL over the ticket `a`, its team `tm`, type `tt`, agreement `sl`, workspace status `cs` and tag `rt`.
type Group = (&'static str, &'static [(&'static str, &'static str)], &'static str);

/// The ticket reports' groupings; the last, Tag, only the volume report's (a ticket's tags are read with it).
const GROUPS: [Group; 11] = [
    ("Status", &[(T, "status")], "a.status"),
    ("Workspace Status", &[(T, "custom_status"), (T, "status"), ("TicketStatus", "status_name")], "coalesce(cs.status_name, a.status)"),
    ("Priority", &[(T, "priority")], "a.priority"),
    ("Team", &[(T, "team"), ("SupportTeam", "team_name")], "coalesce(tm.team_name, 'No Team')"),
    ("Agent", &[(T, "assigned_to")], "coalesce(a.assigned_to, 'Unassigned')"),
    ("Ticket Type", &[(T, "ticket_type"), ("TicketType", "type_name")], "coalesce(tt.type_name, 'No Type')"),
    ("Channel", &[(T, "channel")], "a.channel"),
    ("Customer", &[(T, "customer_name"), (T, "contact_email")], "coalesce(a.customer_name, a.contact_email, 'Unknown')"),
    ("SLA", &[(T, "service_level_agreement"), ("ServiceLevelAgreement", "sla_name")], "coalesce(sl.sla_name, 'No SLA')"),
    ("Month", &[(T, "rated_at"), (T, "resolved_at"), (T, "opened_at")], "to_char(coalesce(a.rated_at, a.resolved_at, a.opened_at), 'YYYY-MM')"),
    ("Tag", &[], "coalesce(rt.tag, 'No Tag')"),
];

fn group_param(default: &str) -> Value {
    json!({"name": "group_by", "label": "Group By", "type": "select", "options": GROUPS[..10].iter().map(|g| g.0).collect::<Vec<_>>(), "default": default})
}

/// Refused unless the caller may read every one of `fields`.
fn need(ctx: &Ctx, fields: &[(&str, &str)]) -> Result<()> {
    fields.iter().try_for_each(|(entity, field)| readable_choice(ctx, entity, field))
}

/// The grouping of `groups` the caller chose, refused when it reads a field they may not; none chosen, `default` or else
/// the first in the list's order whose fields they may read. A grouping reads fields of several entities, so each is
/// checked with `readable_choice` (the platform's `readable_default` looks at one entity's fields).
fn grouping(ctx: &Ctx, p: &Params<'_>, groups: &[Group], default: &str) -> Result<(&'static str, &'static str)> {
    let named = |want: &str| groups.iter().find(|g| g.0.eq_ignore_ascii_case(want));
    let g = match p.str("group_by") {
        Some(want) => {
            let g =
                named(want).ok_or_else(|| Error::field("group_by", format!("use one of: {}", groups.iter().map(|g| g.0).collect::<Vec<_>>().join(", "))))?;
            need(ctx, g.1)?;
            g
        }
        None => named(default)
            .into_iter()
            .chain(groups)
            .find(|g| need(ctx, g.1).is_ok())
            .ok_or_else(|| Error::Forbidden("you may read none of the fields this report groups by".into()))?,
    };
    Ok((g.0, g.2))
}

/// The report `out` as the caller may see it: a column reading a field of `entity` they may not read (by its name, or as
/// `reads` says: column, field; a column may read several) is left out of the columns, the rows and the totals.
fn shown(ctx: &Ctx, entity: &str, mut out: Value, reads: &[(&str, &str)]) -> Value {
    let columns = out["columns"].as_array().cloned().unwrap_or_default();
    let mut probe: Vec<Value> = columns.iter().filter(|c| !reads.iter().any(|(n, _)| c["name"] == json!(n))).cloned().collect();
    probe.extend(reads.iter().map(|(n, f)| json!({"name": n, "field": f})));
    let kept = readable_columns(ctx, entity, probe.clone());
    let gone: Vec<Value> = probe.into_iter().filter(|c| !kept.contains(c)).map(|c| c["name"].clone()).collect();
    if gone.is_empty() {
        return out;
    }
    out["columns"] = Value::Array(columns.into_iter().filter(|c| !gone.contains(&c["name"])).collect());
    let strip = |v: &mut Value| {
        if let Value::Object(m) = v {
            m.retain(|k, _| !gone.contains(&json!(k)));
        }
    };
    if let Some(rows) = out["rows"].as_array_mut() {
        rows.iter_mut().for_each(strip);
    }
    strip(&mut out["totals"]);
    out
}

/// Tickets belong to no company: the default period follows the tenant's fiscal year.
fn period_of(ctx: &Ctx, p: &Params<'_>) -> Result<(NaiveDate, NaiveDate)> {
    let co = ReportCompany { id: Uuid::nil(), name: String::new(), currency: String::new(), fy_month: ctx.fy_start_month, precision: 2 };
    period(p, &co)
}

/// `select <columns> from` the caller's visible tickets with their team, type and agreement.
fn tickets(ctx: &Ctx, columns: &str) -> Result<QueryBuilder<Postgres>> {
    tickets_with(ctx, columns, "")
}

/// [`tickets`] with more joins (`rt` for a ticket's tags, one row each).
fn tickets_with(ctx: &Ctx, columns: &str, joins: &str) -> Result<QueryBuilder<Postgres>> {
    let ent = ctx.schema.entity("Ticket").ok_or_else(|| Error::NotFound("Ticket".into()))?;
    let scope = row_scope(ctx, ent, Action::Read)?;
    let mut qb = QueryBuilder::new(format!(
        "select {columns} from e_ticket a \
         left join e_support_team tm on tm.tenant_id = a.tenant_id and tm.id = a.team \
         left join e_ticket_type tt on tt.tenant_id = a.tenant_id and tt.id = a.ticket_type \
         left join e_service_level_agreement sl on sl.tenant_id = a.tenant_id and sl.id = a.service_level_agreement \
         left join e_ticket_status cs on cs.tenant_id = a.tenant_id and cs.id = a.custom_status {joins} \
         where a.tenant_id = "
    ));
    qb.push_bind(ctx.tenant);
    scope.push_sql(&mut qb, "a");
    Ok(qb)
}

fn within(qb: &mut QueryBuilder<Postgres>, column: &str, (from, to): (NaiveDate, NaiveDate)) {
    qb.push(format!(" and {column} >= ")).push_bind(from).push(format!(" and {column} < ")).push_bind(to).push(" + 1");
}

fn pct(part: i64, whole: i64) -> Value {
    if whole == 0 { Value::Null } else { money((Decimal::from(part) * Decimal::ONE_HUNDRED / Decimal::from(whole)).round_dp(2)) }
}

fn dec(v: Option<Decimal>) -> Value {
    v.map_or(Value::Null, money)
}

fn meta(span: (NaiveDate, NaiveDate), group: &str) -> Value {
    json!({"from_date": span.0, "to_date": span.1, "group_by": group})
}

async fn summary(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "opened_at")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "Status")?;
    let mut qb = tickets(
        ctx,
        &format!(
            "{expr}, count(*), count(*) filter (where a.status = 'Open'), count(*) filter (where a.status = 'Replied'), \
             count(*) filter (where a.status = 'On Hold'), count(*) filter (where a.status = 'Resolved'), \
             count(*) filter (where a.status = 'Closed'), count(*) filter (where a.sla_status = 'Breached')"
        ),
    )?;
    within(&mut qb, "a.opened_at", span);
    qb.push(" group by 1 order by 2 desc, 1");
    let data: Vec<(String, i64, i64, i64, i64, i64, i64, i64)> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let keys = ["tickets", "open", "replied", "on_hold", "resolved", "closed", "breached"];
    let mut totals = [0i64; 7];
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(group, a, b, c, d, e, f, g)| {
            let mut row = json!({"group": group});
            for ((k, t), x) in keys.iter().zip(totals.iter_mut()).zip([a, b, c, d, e, f, g]) {
                row[*k] = json!(x);
                *t += x;
            }
            row
        })
        .collect();
    let out = json!({
        "columns": [col("group", label, "data"), col("tickets", "Tickets", "int"), col("open", "Open", "int"), col("replied", "Replied", "int"),
            col("on_hold", "On Hold", "int"), col("resolved", "Resolved", "int"), col("closed", "Closed", "int"), col("breached", "SLA Breached", "int")],
        "rows": rows,
        "totals": keys.iter().zip(totals).map(|(k, t)| (k.to_string(), json!(t))).collect::<serde_json::Map<_, _>>(),
        "meta": meta(span, label),
    });
    Ok(shown(
        ctx,
        T,
        out,
        &[("open", "status"), ("replied", "status"), ("on_hold", "status"), ("resolved", "status"), ("closed", "status"), ("breached", "sla_status")],
    ))
}

async fn compliance(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "opened_at"), (T, "service_level_agreement")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "SLA")?;
    let mut qb = tickets(
        ctx,
        &format!(
            "{expr}, count(*), count(*) filter (where a.first_responded_at is not null and not a.response_breached), \
             count(*) filter (where a.response_breached), count(*) filter (where a.resolved_at is not null and not a.resolution_breached), \
             count(*) filter (where a.resolution_breached), count(*) filter (where a.next_reply_breached), count(*) filter (where a.update_breached)"
        ),
    )?;
    within(&mut qb, "a.opened_at", span);
    qb.push(" and a.service_level_agreement is not null group by 1 order by 1");
    let data: Vec<(String, i64, i64, i64, i64, i64, i64, i64)> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let row = |group: String, [n, rm, rb, zm, zb, nl, um]: [i64; 7]| {
        json!({"group": group, "tickets": n,
            "response_met": rm, "response_breached": rb, "response_pending": n - rm - rb, "response_pct": pct(rm, rm + rb),
            "resolution_met": zm, "resolution_breached": zb, "resolution_pending": n - zm - zb, "resolution_pct": pct(zm, zm + zb),
            "next_reply_late": nl, "update_missed": um})
    };
    let mut t = [0i64; 7];
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(g, n, rm, rb, zm, zb, nl, um)| {
            let r = [n, rm, rb, zm, zb, nl, um];
            t.iter_mut().zip(r).for_each(|(a, b)| *a += b);
            row(g, r)
        })
        .collect();
    let mut totals = row(String::new(), t);
    if let Value::Object(m) = &mut totals {
        m.remove("group");
    }
    let out = json!({
        "columns": [col("group", label, "data"), col("tickets", "Tickets", "int"),
            col("response_met", "Response Met", "int"), col("response_breached", "Response Breached", "int"), col("response_pending", "Response Pending", "int"),
            col("response_pct", "Response Compliance %", "decimal"),
            col("resolution_met", "Resolution Met", "int"), col("resolution_breached", "Resolution Breached", "int"), col("resolution_pending", "Resolution Pending", "int"),
            col("resolution_pct", "Resolution Compliance %", "decimal"),
            col("next_reply_late", "Next Reply Late", "int"), col("update_missed", "Update Missed", "int")],
        "rows": rows,
        "totals": totals,
        "meta": meta(span, label),
    });
    let (rf, rb, zf, zb) = ("first_responded_at", "response_breached", "resolved_at", "resolution_breached");
    Ok(shown(
        ctx,
        T,
        out,
        &[
            ("response_met", rf),
            ("response_met", rb),
            ("response_breached", rb),
            ("response_pending", rf),
            ("response_pending", rb),
            ("response_pct", rf),
            ("response_pct", rb),
            ("resolution_met", zf),
            ("resolution_met", zb),
            ("resolution_breached", zb),
            ("resolution_pending", zf),
            ("resolution_pending", zb),
            ("resolution_pct", zf),
            ("resolution_pct", zb),
            ("next_reply_late", "next_reply_breached"),
            ("update_missed", "update_breached"),
        ],
    ))
}

type TimesRow = (String, i64, i64, Option<Decimal>, Option<Decimal>, Option<Decimal>, i64, Option<Decimal>, Option<Decimal>, Option<Decimal>);

async fn times(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "opened_at")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "Priority")?;
    let stats = |f: &str| {
        format!(
            "count(a.{f}), round(avg(a.{f}), 2), round((percentile_cont(0.5) within group (order by a.{f}))::numeric, 2), \
             round((percentile_cont(0.9) within group (order by a.{f}))::numeric, 2)"
        )
    };
    let columns = format!("count(*), {}, {}", stats("first_response_hours"), stats("resolution_hours"));
    let mut grouped = tickets(ctx, &format!("{expr}, {columns}"))?;
    within(&mut grouped, "a.opened_at", span);
    grouped.push(" group by 1 order by 1");
    let data: Vec<TimesRow> = grouped.build_query_as().fetch_all(&mut **tx).await?;
    let mut all = tickets(ctx, &format!("'All'::text, {columns}"))?;
    within(&mut all, "a.opened_at", span);
    let total: TimesRow = all.build_query_as().fetch_one(&mut **tx).await?;
    let row = |(group, n, rn, ravg, rmed, rp90, zn, zavg, zmed, zp90): TimesRow| {
        json!({"group": group, "tickets": n,
            "responded": rn, "response_avg": dec(ravg), "response_median": dec(rmed), "response_p90": dec(rp90),
            "resolved": zn, "resolution_avg": dec(zavg), "resolution_median": dec(zmed), "resolution_p90": dec(zp90)})
    };
    let mut totals = row(total);
    if let Value::Object(m) = &mut totals {
        m.remove("group");
    }
    let out = json!({
        "columns": [col("group", label, "data"), col("tickets", "Tickets", "int"), col("responded", "Responded", "int"),
            col("response_avg", "First Response Avg (h)", "decimal"), col("response_median", "First Response Median (h)", "decimal"),
            col("response_p90", "First Response 90th Pct (h)", "decimal"), col("resolved", "Resolved", "int"),
            col("resolution_avg", "Resolution Avg (h)", "decimal"), col("resolution_median", "Resolution Median (h)", "decimal"),
            col("resolution_p90", "Resolution 90th Pct (h)", "decimal")],
        "rows": data.into_iter().map(row).collect::<Vec<_>>(),
        "totals": totals,
        "meta": {"from_date": span.0, "to_date": span.1, "group_by": label, "unit": "business hours (round the clock without an SLA), pauses excluded"},
    });
    let (r, z) = ("first_response_hours", "resolution_hours");
    Ok(shown(
        ctx,
        T,
        out,
        &[
            ("responded", r),
            ("response_avg", r),
            ("response_median", r),
            ("response_p90", r),
            ("resolved", z),
            ("resolution_avg", z),
            ("resolution_median", z),
            ("resolution_p90", z),
        ],
    ))
}

type WorkRow = (String, i64, i64, i64, i64, i64, i64, i64, i64, Option<Decimal>, Option<String>);

async fn workload(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    let team = p.uuid("team")?;
    need(ctx, &[(T, "assigned_to"), (T, "status")])?;
    if team.is_some() {
        need(ctx, &[(T, "team")])?;
    }
    let mut qb = tickets(
        ctx,
        "coalesce(a.assigned_to, ''), count(*), count(*) filter (where a.status = 'Open'), count(*) filter (where a.status = 'Replied'), \
         count(*) filter (where a.status = 'On Hold'), count(*) filter (where a.priority = 'Urgent'), count(*) filter (where a.priority = 'High'), \
         count(*) filter (where a.sla_status = 'At Risk'), count(*) filter (where a.sla_status = 'Breached'), \
         round((extract(epoch from now() - min(a.opened_at)) / 86400)::numeric, 2), (array_agg(a.number order by a.opened_at))[1]",
    )?;
    qb.push(" and a.status in ('Open', 'Replied', 'On Hold')");
    if let Some(t) = team {
        qb.push(" and a.team = ").push_bind(t);
    }
    qb.push(" group by 1 order by 2 desc, 1");
    let mut data: Vec<WorkRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    // Active team members without open tickets belong in a workload view too.
    let members = child_table(ctx, "SupportTeam", "members")?;
    let mut q = QueryBuilder::new(format!("select distinct m.agent from {members} m where m.tenant_id = "));
    q.push_bind(ctx.tenant).push(" and m.active and m.agent is not null");
    if let Some(t) = team {
        q.push(" and m.parent_id = ").push_bind(t);
    }
    if let Some(allowed) = ctx.scopes.get("SupportTeam").filter(|_| !ctx.is_admin && !ctx.is_system) {
        q.push(" and m.parent_id = any(").push_bind(allowed.clone()).push(")");
    }
    let mut idle: Vec<String> = q.build_query_scalar().fetch_all(&mut **tx).await?;
    idle.retain(|agent| !data.iter().any(|r| &r.0 == agent));
    idle.sort();
    data.extend(idle.into_iter().map(|agent| (agent, 0, 0, 0, 0, 0, 0, 0, 0, None, None)));
    let keys = ["open_tickets", "open", "replied", "on_hold", "urgent", "high", "at_risk", "breached"];
    let mut totals = [0i64; 8];
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(agent, n, o, r, h, u, hi, ar, br, oldest, number)| {
            let mut row =
                json!({"agent": if agent.is_empty() { "Unassigned".to_string() } else { agent }, "oldest_open_days": dec(oldest), "oldest_ticket": number});
            for ((k, t), x) in keys.iter().zip(totals.iter_mut()).zip([n, o, r, h, u, hi, ar, br]) {
                row[*k] = json!(x);
                *t += x;
            }
            row
        })
        .collect();
    let out = json!({
        "columns": [col("agent", "Agent", "data"), col("open_tickets", "Open Tickets", "int"), col("open", "Open", "int"), col("replied", "Replied", "int"),
            col("on_hold", "On Hold", "int"), col("urgent", "Urgent", "int"), col("high", "High", "int"), col("at_risk", "At Risk", "int"),
            col("breached", "Breached", "int"), col("oldest_open_days", "Oldest Open (Days)", "decimal"), col("oldest_ticket", "Oldest Ticket", "data")],
        "rows": rows,
        "totals": keys.iter().zip(totals).map(|(k, t)| (k.to_string(), json!(t))).collect::<serde_json::Map<_, _>>(),
    });
    Ok(shown(
        ctx,
        T,
        out,
        &[
            ("urgent", "priority"),
            ("high", "priority"),
            ("at_risk", "sla_status"),
            ("breached", "sla_status"),
            ("oldest_open_days", "opened_at"),
            ("oldest_ticket", "opened_at"),
        ],
    ))
}

type CsatRow = (String, i64, Option<Decimal>, i64, i64, i64, i64, i64);

async fn satisfaction(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "satisfaction_rating"), (T, "rated_at"), (T, "resolved_at")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "Agent")?;
    let columns = "count(*), round(avg(a.satisfaction_rating), 2), count(*) filter (where a.satisfaction_rating = 1), \
                   count(*) filter (where a.satisfaction_rating = 2), count(*) filter (where a.satisfaction_rating = 3), \
                   count(*) filter (where a.satisfaction_rating = 4), count(*) filter (where a.satisfaction_rating = 5)";
    let rated = |qb: &mut QueryBuilder<Postgres>| {
        qb.push(" and a.satisfaction_rating is not null");
        within(qb, "coalesce(a.rated_at, a.resolved_at)", span);
    };
    let mut grouped = tickets(ctx, &format!("{expr}, {columns}"))?;
    rated(&mut grouped);
    grouped.push(" group by 1 order by 3 desc, 1");
    let data: Vec<CsatRow> = grouped.build_query_as().fetch_all(&mut **tx).await?;
    let mut all = tickets(ctx, &format!("'All'::text, {columns}"))?;
    rated(&mut all);
    let total: CsatRow = all.build_query_as().fetch_one(&mut **tx).await?;
    let row = |(group, n, avg, s1, s2, s3, s4, s5): CsatRow| {
        json!({"group": group, "rated": n, "average_rating": dec(avg), "star_1": s1, "star_2": s2, "star_3": s3, "star_4": s4, "star_5": s5,
            "satisfied_pct": pct(s4 + s5, n)})
    };
    let mut totals = row(total);
    if let Value::Object(m) = &mut totals {
        m.remove("group");
    }
    Ok(json!({
        "columns": [col("group", label, "data"), col("rated", "Rated Tickets", "int"), col("average_rating", "Average Rating", "decimal"),
            col("star_1", "1 Star", "int"), col("star_2", "2 Stars", "int"), col("star_3", "3 Stars", "int"), col("star_4", "4 Stars", "int"),
            col("star_5", "5 Stars", "int"), col("satisfied_pct", "Satisfied % (4-5)", "decimal")],
        "rows": data.into_iter().map(row).collect::<Vec<_>>(),
        "totals": totals,
        "meta": meta(span, label),
    }))
}

type VolumeRow = (String, i64, i64, i64, i64);

/// Tickets opened in the period by channel, tag, customer, team, agent, type, priority or month: how many, how many solved, how many
/// were reopened and how many missed a target.
async fn volume(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "opened_at")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS, "Channel")?;
    let joins = if label == "Tag" { "left join record_tag rt on rt.tenant_id = a.tenant_id and rt.entity = 'Ticket' and rt.record_id = a.id" } else { "" };
    let mut qb = tickets_with(
        ctx,
        &format!(
            "{expr}, count(distinct a.id), count(distinct a.id) filter (where a.resolved_at is not null), count(distinct a.id) filter (where a.reopen_count > 0), \
             count(distinct a.id) filter (where a.sla_status = 'Breached')"
        ),
        joins,
    )?;
    within(&mut qb, "a.opened_at", span);
    qb.push(" group by 1 order by 2 desc, 1");
    let data: Vec<VolumeRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let mut totals = (0, 0, 0, 0);
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(g, n, z, r, b)| {
            totals = (totals.0 + n, totals.1 + z, totals.2 + r, totals.3 + b);
            json!({"group": g, "tickets": n, "resolved": z, "reopened": r, "breached": b})
        })
        .collect();
    let out = json!({
        "columns": [col("group", label, "data"), col("tickets", "Tickets", "int"), col("resolved", "Resolved", "int"), col("reopened", "Reopened", "int"), col("breached", "SLA Breached", "int")],
        "rows": rows,
        // A ticket with several tags is counted under each, so a tag's total is not the number of tickets.
        "totals": {"tickets": totals.0, "resolved": totals.1, "reopened": totals.2, "breached": totals.3},
        "meta": meta(span, label),
    });
    Ok(shown(ctx, T, out, &[("resolved", "resolved_at"), ("reopened", "reopen_count"), ("breached", "sla_status")]))
}

type BacklogRow = (String, i64, i64, i64, i64, i64, i64, i64, Option<Decimal>);

/// The tickets still being worked on, by how long they have been open, by status, agent, team or priority.
async fn backlog(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "status")])?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "Status")?;
    let age = "extract(epoch from now() - a.opened_at) / 86400";
    let mut qb = tickets(
        ctx,
        &format!(
            "{expr}, count(*), count(*) filter (where {age} < 1), count(*) filter (where {age} >= 1 and {age} < 3), count(*) filter (where {age} >= 3 and {age} < 7), \
             count(*) filter (where {age} >= 7 and {age} < 14), count(*) filter (where {age} >= 14 and {age} < 30), count(*) filter (where {age} >= 30), \
             round(max({age})::numeric, 1)"
        ),
    )?;
    qb.push(" and a.status in ('Open', 'Replied', 'On Hold') group by 1 order by 2 desc, 1");
    let data: Vec<BacklogRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let keys = ["open", "under_1", "d1_3", "d3_7", "d7_14", "d14_30", "over_30"];
    let mut totals = [0i64; 7];
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(g, n, a, b, c, d, e, f, oldest)| {
            let mut row = json!({"group": g, "oldest_days": dec(oldest)});
            for ((k, t), x) in keys.iter().zip(totals.iter_mut()).zip([n, a, b, c, d, e, f]) {
                row[*k] = json!(x);
                *t += x;
            }
            row
        })
        .collect();
    let out = json!({
        "columns": [col("group", label, "data"), col("open", "Open Tickets", "int"), col("under_1", "Under 1 Day", "int"), col("d1_3", "1 to 3 Days", "int"),
            col("d3_7", "3 to 7 Days", "int"), col("d7_14", "7 to 14 Days", "int"), col("d14_30", "14 to 30 Days", "int"), col("over_30", "Over 30 Days", "int"),
            col("oldest_days", "Oldest (Days)", "decimal")],
        "rows": rows,
        "totals": keys.iter().zip(totals).map(|(k, t)| (k.to_string(), json!(t))).collect::<serde_json::Map<_, _>>(),
        "meta": {"group_by": label},
    });
    let o = "opened_at";
    Ok(shown(ctx, T, out, &[("under_1", o), ("d1_3", o), ("d3_7", o), ("d7_14", o), ("d14_30", o), ("over_30", o), ("oldest_days", o)]))
}

type ReopenRow = (String, i64, i64, i64);

/// How often solved tickets come back: of the tickets opened in the period that were ever solved, how many were reopened, and how many times.
async fn reopens(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    need(ctx, &[(T, "opened_at"), (T, "resolved_at"), (T, "reopen_count")])?;
    let span = period_of(ctx, p)?;
    let (label, expr) = grouping(ctx, p, &GROUPS[..10], "Agent")?;
    let mut qb = tickets(ctx, &format!("{expr}, count(*), count(*) filter (where a.reopen_count > 0), coalesce(sum(a.reopen_count), 0)::bigint"))?;
    qb.push(" and (a.resolved_at is not null or a.reopen_count > 0)");
    within(&mut qb, "a.opened_at", span);
    qb.push(" group by 1 order by 3 desc, 1");
    let data: Vec<ReopenRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let mut totals = (0, 0, 0);
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(g, solved, back, times)| {
            totals = (totals.0 + solved, totals.1 + back, totals.2 + times);
            json!({"group": g, "solved": solved, "reopened": back, "reopens": times, "reopen_pct": pct(back, solved)})
        })
        .collect();
    Ok(json!({
        "columns": [col("group", label, "data"), col("solved", "Tickets Solved", "int"), col("reopened", "Reopened", "int"), col("reopens", "Times Reopened", "int"), col("reopen_pct", "Reopen Rate %", "decimal")],
        "rows": rows,
        "totals": {"solved": totals.0, "reopened": totals.1, "reopens": totals.2, "reopen_pct": pct(totals.1, totals.0)},
        "meta": meta(span, label),
    }))
}

/// Groupings of time entries `tw` on tickets `a` (Agent and Month read only the entries, which the report needs whole).
const TIME_GROUPS: [Group; 5] = [
    ("Agent", &[], "tw.agent"),
    ("Customer", &[(T, "customer_name"), (T, "contact_email")], "coalesce(a.customer_name, a.contact_email, 'Unknown')"),
    ("Team", &[(T, "team"), ("SupportTeam", "team_name")], "coalesce(tm.team_name, 'No Team')"),
    ("Ticket", &[(T, "number")], "coalesce(a.number, '')"),
    ("Month", &[], "to_char(tw.spent_on, 'YYYY-MM')"),
];

/// Time logged on tickets in the period (by the day it was spent), within the caller's record scopes on tickets.
async fn time_spent(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    let span = period_of(ctx, p)?;
    need(ctx, &[(T, "time")])?;
    let (label, expr) = grouping(ctx, p, &TIME_GROUPS, "Agent")?;
    let table = child_table(ctx, "Ticket", "time")?;
    let mut qb = tickets_with(
        ctx,
        &format!(
            "{expr}, count(*), coalesce(sum(tw.minutes), 0)::bigint, round(coalesce(sum(tw.minutes), 0) / 60.0, 2), coalesce(sum(tw.minutes) filter (where tw.billable), 0)::bigint"
        ),
        &format!("join {table} tw on tw.tenant_id = a.tenant_id and tw.parent_id = a.id"),
    )?;
    within(&mut qb, "tw.spent_on", span);
    qb.push(" group by 1 order by 3 desc, 1");
    let data: Vec<(String, i64, i64, Decimal, i64)> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let mut totals = (0i64, 0i64, 0i64);
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(group, entries, minutes, hours, billable)| {
            totals = (totals.0 + entries, totals.1 + minutes, totals.2 + billable);
            json!({"group": group, "entries": entries, "minutes": minutes, "hours": money(hours), "billable_minutes": billable})
        })
        .collect();
    Ok(json!({
        "columns": [col("group", label, "data"), col("entries", "Entries", "int"), col("minutes", "Minutes", "int"), col("hours", "Hours", "decimal"),
            col("billable_minutes", "Billable Minutes", "int")],
        "rows": rows,
        "totals": {"entries": totals.0, "minutes": totals.1, "hours": money((Decimal::from(totals.1) / Decimal::from(60)).round_dp(2)), "billable_minutes": totals.2},
        "meta": meta(span, label),
    }))
}

type ArticleRow = (String, String, Option<String>, Option<String>, i64, i64, i64);

/// The help centre by article: how often each was read and what readers said of it, from the signed-in portal and the
/// public pages together, as many as the caller's record scopes on articles reach.
async fn articles(ctx: &Ctx, p: &Params<'_>, tx: &mut Tx) -> Result<Value> {
    let ent = ctx.schema.entity("KbArticle").ok_or_else(|| Error::NotFound("KbArticle".into()))?;
    let scope = row_scope(ctx, ent, Action::Read)?;
    let mut qb = QueryBuilder::new(
        "select a.title, coalesce(c.category_name, 'No Category'), a.visibility, a.status, coalesce(a.views, 0)::bigint, \
         coalesce(a.helpful_count, 0)::bigint, coalesce(a.not_helpful_count, 0)::bigint from e_kb_article a \
         left join e_kb_category c on c.tenant_id = a.tenant_id and c.id = a.category where a.tenant_id = ",
    );
    qb.push_bind(ctx.tenant);
    for (param, column) in [("status", "a.status"), ("visibility", "a.visibility")] {
        if let Some(v) = p.str(param).filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("all")) {
            readable_choice(ctx, "KbArticle", param)?;
            qb.push(format!(" and {column} = ")).push_bind(v.to_string());
        }
    }
    scope.push_sql(&mut qb, "a");
    // Most read first, unless the caller may not read how often (the order would tell).
    let ranked = need(ctx, &[("KbArticle", "views"), ("KbArticle", "helpful_count")]).is_ok();
    qb.push(if ranked { " order by 5 desc, 6 desc, 1 limit 500" } else { " order by 1 limit 500" });
    let data: Vec<ArticleRow> = qb.build_query_as().fetch_all(&mut **tx).await?;
    let mut totals = (0, 0, 0);
    let rows: Vec<Value> = data
        .into_iter()
        .map(|(title, category, visibility, status, views, helpful, not_helpful)| {
            totals = (totals.0 + views, totals.1 + helpful, totals.2 + not_helpful);
            json!({"title": title, "category": category, "visibility": visibility, "status": status, "views": views, "helpful": helpful,
                "not_helpful": not_helpful, "votes": helpful + not_helpful, "helpful_pct": pct(helpful, helpful + not_helpful)})
        })
        .collect();
    let out = json!({
        "columns": [col("title", "Article", "data"), col("category", "Category", "data"), col("visibility", "Visibility", "data"), col("status", "Status", "data"),
            col("views", "Views", "int"), col("helpful", "Helpful", "int"), col("not_helpful", "Not Helpful", "int"), col("votes", "Votes", "int"),
            col("helpful_pct", "Helpful %", "decimal")],
        "rows": rows,
        "totals": {"views": totals.0, "helpful": totals.1, "not_helpful": totals.2, "votes": totals.1 + totals.2, "helpful_pct": pct(totals.1, totals.1 + totals.2)},
        "meta": {"status": p.str("status").unwrap_or("All"), "visibility": p.str("visibility").unwrap_or("All")},
    });
    let (h, n) = ("helpful_count", "not_helpful_count");
    Ok(shown(ctx, "KbArticle", out, &[("helpful", h), ("not_helpful", n), ("votes", h), ("votes", n), ("helpful_pct", h), ("helpful_pct", n)]))
}

#[async_trait]
impl ReportSource for SupportReports {
    fn catalog(&self) -> Vec<Value> {
        let entry =
            |name: &str, label: &str, params: Vec<Value>| json!({"name": name, "label": label, "module": "support", "entity": "Ticket", "params": params});
        let span = || vec![param_def("from_date"), param_def("to_date")];
        vec![
            entry("support_ticket_summary", "Ticket Summary", [span(), vec![group_param("Status")]].concat()),
            entry("support_sla_compliance", "SLA Compliance", [span(), vec![group_param("SLA")]].concat()),
            entry("support_response_times", "Response & Resolution Times", [span(), vec![group_param("Priority")]].concat()),
            entry("support_agent_workload", "Agent Workload", vec![json!({"name": "team", "label": "Support Team", "type": "link", "to": "SupportTeam"})]),
            entry("support_customer_satisfaction", "Customer Satisfaction", [span(), vec![group_param("Agent")]].concat()),
            entry("support_backlog", "Backlog by Age", vec![group_param("Status")]),
            entry(
                "support_volume",
                "Ticket Volume",
                [span(), vec![json!({"name": "group_by", "label": "Group By", "type": "select", "options": GROUPS.map(|g| g.0), "default": "Channel"})]]
                    .concat(),
            ),
            entry("support_reopens", "Reopen Rate", [span(), vec![group_param("Agent")]].concat()),
            entry(
                "support_time_spent",
                "Time Spent",
                [span(), vec![json!({"name": "group_by", "label": "Group By", "type": "select", "options": TIME_GROUPS.map(|g| g.0), "default": "Agent"})]]
                    .concat(),
            ),
            json!({"name": "support_articles", "label": "Help Centre by Article", "module": "support", "entity": "KbArticle", "params": [
                {"name": "status", "label": "Status", "type": "select", "options": ["All", "Draft", "Published", "Archived"], "default": "All"},
                {"name": "visibility", "label": "Visibility", "type": "select", "options": ["All", "Internal", "Customers Only", "Public"], "default": "All"}]}),
        ]
    }

    async fn run(&self, k: &Kernel, ctx: &Ctx, name: &str, params: &HashMap<String, String>) -> Result<Value> {
        let p = Params { raw: params };
        let mut tx = read_tx(k, ctx).await?;
        let out = match name {
            "support_ticket_summary" => summary(ctx, &p, &mut tx).await,
            "support_sla_compliance" => compliance(ctx, &p, &mut tx).await,
            "support_response_times" => times(ctx, &p, &mut tx).await,
            "support_agent_workload" => workload(ctx, &p, &mut tx).await,
            "support_customer_satisfaction" => satisfaction(ctx, &p, &mut tx).await,
            "support_backlog" => backlog(ctx, &p, &mut tx).await,
            "support_volume" => volume(ctx, &p, &mut tx).await,
            "support_reopens" => reopens(ctx, &p, &mut tx).await,
            "support_articles" => articles(ctx, &p, &mut tx).await,
            "support_time_spent" => time_spent(ctx, &p, &mut tx).await,
            _ => Err(Error::NotFound(format!("report {name}"))),
        };
        tx.rollback().await?;
        out
    }
}
