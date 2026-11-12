//! Assignment: which team and which agent a ticket goes to.
//!
//! The rules are records (`TicketAssignmentRule`): the enabled one with the highest priority whose condition
//! holds for the ticket decides. It gives the team, and an agent of that team by taking turns (Round Robin) or by
//! the fewest open tickets (Load Balancing), among the agents who can take it: active members under their limit
//! and, when the rule says so, not away (the platform's away periods) and on shift (their profile's working calendar
//! is open now), with the skill the rule asks for (a fixed one, or the ticket's language) on their profile. When
//! nobody can, the rule's fallback agent gets it, else it stays with the team unassigned.
//!
//! It runs for a new ticket, for a reopened one whose agent cannot take it, and for a ticket nobody answered for
//! the time Support Settings say (`reassign_unanswered`, from the upkeep), which the agent who had it does not get
//! again. The rule's row is locked for the rest of the transaction, so two tickets at once take turns and never
//! go to the same agent twice by the same turn.

use crate::ticket::{Member, OPEN, pick_member};
use crate::{int, linked};
use chrono::Utc;
use ferrum_domain::util::{b, get, id, rows, s, set, sys};
use ferrum_kernel::{Ctx, Doc, Filter, HookCtx, Kernel, Result, Tx};
use ferrum_meta::{EffEntity, Expr, Scope};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

/// Where a ticket was sent.
#[derive(Debug, Clone, PartialEq)]
pub struct Routed {
    pub team: Uuid,
    pub agent: Option<String>,
    pub rule: Uuid,
}

/// An agent's profile, as far as routing reads it.
#[derive(Default)]
struct Profile {
    skills: Vec<String>,
    /// Open now (or no calendar to be closed by).
    on_shift: bool,
    away: bool,
}

/// Comma-separated tags, lowercase.
pub fn skills(list: &str) -> Vec<String> {
    list.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()).collect()
}

/// Whether a skill list covers `want`: the tag itself, or for a language its base (`hi` covers `hi-IN`).
pub fn covers(have: &[String], want: &str) -> bool {
    let want = want.trim().to_lowercase();
    have.iter().any(|h| *h == want || want.strip_prefix(h.as_str()).is_some_and(|rest| rest.starts_with('-')))
}

async fn profiles(tx: &mut Tx, tenant: Uuid, agents: &[String]) -> Result<HashMap<String, Profile>> {
    let now = Utc::now();
    let rows_: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("select agent::text, skills, shift_calendar from e_agent_profile where tenant_id = $1 and agent = any($2)")
            .bind(tenant)
            .bind(agents)
            .fetch_all(&mut **tx)
            .await?;
    let away: HashSet<String> = sqlx::query_scalar::<_, String>(
        "select u.email::text from person_away a join app_user u on u.tenant_id = a.tenant_id and u.id = a.user_id \
         where a.tenant_id = $1 and u.email = any($2) and current_date between a.starts_on and a.ends_on",
    )
    .bind(tenant)
    .bind(agents)
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect();
    let mut open: HashMap<String, bool> = HashMap::new();
    let mut out: HashMap<String, Profile> =
        agents.iter().map(|a| (a.clone(), Profile { on_shift: true, away: away.contains(a), ..Default::default() })).collect();
    for (agent, list, calendar) in rows_ {
        let p = out.entry(agent).or_default();
        p.skills = skills(list.as_deref().unwrap_or_default());
        if let Some(name) = calendar.filter(|n| !n.trim().is_empty()) {
            if !open.contains_key(&name) {
                // A calendar that is not there is no reason to turn work away.
                let is = ferrum_kernel::load_calendar(tx, tenant, &name).await?.is_none_or(|c| c.is_open(now));
                open.insert(name.clone(), is);
            }
            p.on_shift = open[&name];
        }
    }
    Ok(out)
}

/// The open tickets each agent holds.
async fn load(tx: &mut Tx, tenant: Uuid, agents: &[String]) -> Result<HashMap<String, i64>> {
    Ok(sqlx::query_as::<_, (String, i64)>(
        "select assigned_to, count(*) from e_ticket where tenant_id = $1 and assigned_to = any($2) and status = any($3) group by assigned_to",
    )
    .bind(tenant)
    .bind(agents)
    .bind(&OPEN[..])
    .fetch_all(&mut **tx)
    .await?
    .into_iter()
    .collect())
}

/// Chooses the team and agent for a ticket under the first rule that fits, as the module's notes say. `exclude` is an agent
/// who must not get it (the one who had it). `None` when no rule fits. Locks the rule it uses.
pub async fn route(k: &Kernel, ctx: &Ctx, tx: &mut Tx, ent: &EffEntity, doc: &Doc, exclude: Option<&str>) -> Result<Option<Routed>> {
    let sctx = sys(ctx);
    let rules = k.find_tx(&sctx, tx, "TicketAssignmentRule", &[Filter::eq("enabled", true)], Some("priority desc"), 1000).await?;
    let view = ent.cel_view(&Value::Object(doc.clone()), &ctx.schema);
    let user = ctx.user_value();
    // Agents limited to some teams route only into those teams (anything else would be refused).
    let allowed = if ctx.is_admin || ctx.is_system { None } else { ctx.scopes.get("SupportTeam").cloned() };
    for rule in rules {
        let (Some(rule_id), Some(team)) = (id(&rule, "id"), id(&rule, "team")) else { continue };
        if id(doc, "team").is_some_and(|t| t != team) || allowed.as_ref().is_some_and(|a| !a.contains(&team)) {
            continue;
        }
        if let Some(cond) = s(&rule, "condition") {
            match Expr::compile(cond).and_then(|e| e.eval_bool(&Scope { doc: &view, old: None, parent: None, user: &user })) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(e) => {
                    tracing::warn!(rule = %rule_id, "assignment rule condition failed: {e}");
                    continue;
                }
            }
        }
        let rule = k.get_tx(&sctx, tx, "TicketAssignmentRule", rule_id, true).await?;
        let members: Vec<(String, bool, i64)> = rows(&get(k, ctx, tx, "SupportTeam", team).await?, "members")
            .iter()
            .filter_map(|m| Some((s(m, "agent")?.to_string(), b(m, "active"), int(m, "max_open_tickets"))))
            .collect();
        let agents: Vec<String> = members.iter().map(|m| m.0.clone()).collect();
        let (open, who) = (load(tx, ctx.tenant, &agents).await?, profiles(tx, ctx.tenant, &agents).await?);
        let available = b(&rule, "only_available");
        let want: Option<String> = match s(&rule, "needs_skill") {
            Some("Ticket Language") => s(doc, "language").map(str::to_string),
            Some("Fixed Skill") => s(&rule, "skill").map(str::to_string),
            _ => None,
        };
        let candidates: Vec<Member> = members
            .iter()
            .map(|(agent, active, max_open)| {
                let p = who.get(agent);
                let fits = (!available || p.is_none_or(|p| !p.away && p.on_shift)) && want.as_deref().is_none_or(|w| p.is_some_and(|p| covers(&p.skills, w)));
                Member { agent: agent.clone(), active: *active && fits && exclude != Some(agent.as_str()), max_open: *max_open }
            })
            .collect();
        let chosen = pick_member(&candidates, s(&rule, "last_assigned_to"), &open, s(&rule, "rule_type") == Some("Load Balancing"));
        if let Some(agent) = &chosen {
            let mut v = Doc::new();
            v.insert("last_assigned_to".into(), json!(agent));
            k.set_values_tx(&sctx, tx, "TicketAssignmentRule", rule_id, v).await?;
        }
        let agent = chosen.or_else(|| s(&rule, "fallback_agent").filter(|f| exclude != Some(*f)).map(str::to_string));
        return Ok(Some(Routed { team, agent, rule: rule_id }));
    }
    Ok(None)
}

/// A new ticket with no agent: routed by the rules (the module's notes).
pub async fn assign(hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
    if s(doc, "assigned_to").is_some() {
        return Ok(());
    }
    if let Some(r) = route(hc.kernel, hc.ctx, hc.tx, hc.entity, doc, None).await? {
        apply(doc, &r);
    }
    Ok(())
}

pub fn apply(doc: &mut Doc, r: &Routed) {
    set(doc, "team", r.team.to_string());
    if let Some(a) = &r.agent {
        set(doc, "assigned_to", a.clone());
    }
    set(doc, "routed_by", r.rule.to_string());
    set(doc, "routed_at", Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true));
}

/// Whether `agent` can take a ticket of `team` now: an active member, not away, on shift.
pub async fn can_take(k: &Kernel, ctx: &Ctx, tx: &mut Tx, team: Option<Uuid>, agent: &str) -> Result<bool> {
    if let Some(t) = team
        && let Some(team_doc) = linked(k, ctx, tx, "SupportTeam", t).await?
        && !rows(&team_doc, "members").iter().any(|m| s(m, "agent") == Some(agent) && b(m, "active"))
    {
        return Ok(false);
    }
    let who = profiles(tx, ctx.tenant, &[agent.to_string()]).await?;
    Ok(who.get(agent).is_none_or(|p| !p.away && p.on_shift))
}

/// A reopened ticket whose agent cannot take it goes through the rules again, to someone else.
pub async fn reroute_reopened(hc: &mut HookCtx<'_>, doc: &mut Doc) -> Result<()> {
    let on: Option<bool> = sqlx::query_scalar("select route_reopened from e_support_settings where tenant_id = $1 limit 1")
        .bind(hc.ctx.tenant)
        .fetch_optional(&mut **hc.tx)
        .await?
        .flatten();
    if on == Some(false) {
        return Ok(());
    }
    let current = s(doc, "assigned_to").map(str::to_string);
    if let Some(agent) = &current
        && can_take(hc.kernel, hc.ctx, hc.tx, id(doc, "team"), agent).await?
    {
        return Ok(());
    }
    let mut asked = doc.clone();
    asked.remove("team");
    if let Some(r) = route(hc.kernel, hc.ctx, hc.tx, hc.entity, &asked, current.as_deref()).await? {
        if r.agent.is_some() {
            apply(doc, &r);
        } else {
            set(doc, "team", r.team.to_string());
        }
    }
    Ok(())
}

/// Open tickets nobody has answered for `hours` go to the next agent their rule picks, up to `max` times each. The hours
/// are working hours: counted on the calendar of the ticket's agreement (else `helpdesk_hours`, else the clock), so a
/// ticket that came in on Friday evening is not given away on Saturday. Returns how many moved.
pub async fn reassign_unanswered(k: &Kernel, ctx: &Ctx) -> Result<usize> {
    let (hours, max): (i64, i64) = {
        let mut tx = k.db.tenant_tx(ctx.tenant).await?;
        let row: Option<(Option<i64>, Option<i64>)> =
            sqlx::query_as("select reassign_unanswered_hours, max_reassignments from e_support_settings where tenant_id = $1 limit 1")
                .bind(ctx.tenant)
                .fetch_optional(&mut *tx)
                .await?;
        tx.commit().await?;
        row.map_or((0, 2), |(h, m)| (h.unwrap_or(0), m.unwrap_or(2)))
    };
    if hours <= 0 {
        return Ok(0);
    }
    let mut tx = k.db.tenant_tx(ctx.tenant).await?;
    // Waiting no longer than the clock says is a first cut: working time is never more than the clock's.
    let waiting: Vec<(Uuid, Option<String>, chrono::DateTime<Utc>)> = sqlx::query_as(
        "select id, sla_calendar, coalesce(last_customer_message_at, opened_at) from e_ticket where tenant_id = $1 and status = 'Open' and assigned_to is not null \
         and coalesce(reassigned_count, 0) < $3 and (last_agent_message_at is null or last_customer_message_at > last_agent_message_at) \
         and coalesce(last_customer_message_at, opened_at) < now() - make_interval(hours => $2) order by opened_at limit 1000",
    )
    .bind(ctx.tenant)
    .bind(hours as i32)
    .bind(max)
    .fetch_all(&mut *tx)
    .await?;
    let now = Utc::now();
    let mut calendars: HashMap<String, Option<ferrum_kernel::Calendar>> = HashMap::new();
    let mut due: Vec<Uuid> = Vec::new();
    for (ticket, calendar, since) in waiting {
        let name = calendar.filter(|c| !c.is_empty()).unwrap_or_else(|| crate::hours::CALENDAR.to_string());
        if !calendars.contains_key(&name) {
            let loaded = ferrum_kernel::load_calendar(&mut tx, ctx.tenant, &name).await?;
            calendars.insert(name.clone(), loaded);
        }
        let waited = match calendars.get(&name).and_then(Option::as_ref) {
            // A calendar the workspace lacks, or a wait too long to count in hours, is the clock's.
            Some(c) => c.working_between(since, now, &[]).map_or((now - since).num_seconds(), |d| d.num_seconds()),
            None => (now - since).num_seconds(),
        };
        if waited >= hours * 3600 && due.len() < 200 {
            due.push(ticket);
        }
    }
    tx.commit().await?;
    let sctx = sys(ctx);
    let ent = k.entity(ctx, "Ticket")?;
    let mut moved = 0;
    for ticket in due {
        let mut tx = k.db.tenant_tx(ctx.tenant).await?;
        let doc = k.get_tx(&sctx, &mut tx, "Ticket", ticket, true).await?;
        let (Some(current), true) = (s(&doc, "assigned_to").map(str::to_string), s(&doc, "status") == Some("Open")) else { continue };
        let mut asked = doc.clone();
        asked.remove("team");
        let Some(r) = route(k, &sctx, &mut tx, &ent, &asked, Some(&current)).await? else { continue };
        let Some(agent) = r.agent.filter(|a| *a != current) else { continue };
        let mut patch = Doc::new();
        patch.insert("assigned_to".into(), json!(agent));
        patch.insert("team".into(), json!(r.team.to_string()));
        patch.insert("routed_by".into(), json!(r.rule.to_string()));
        patch.insert("routed_at".into(), json!(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)));
        patch.insert("reassigned_count".into(), json!(int(&doc, "reassigned_count") + 1));
        k.update_tx(&sctx, &mut tx, "Ticket", ticket, patch, None).await?;
        tx.commit().await?;
        moved += 1;
    }
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skills_are_lowercase_tags_and_a_language_covers_its_variants() {
        assert_eq!(skills(" Billing, HI ,,en-GB"), ["billing", "hi", "en-gb"]);
        let have = skills("billing, hi");
        assert!(covers(&have, "Billing") && covers(&have, "hi-IN") && covers(&have, "HI"));
        assert!(!covers(&have, "his") && !covers(&have, "en") && !covers(&[], "hi"));
    }
}
