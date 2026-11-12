import { t as tr } from "@ferrum/client";
// The agent inbox's rules (kept apart from the page, so they are tested): the queues and what each
// asks the server, which tickets need someone first, how long each has waited, what the inbox keeps
// in its address so a link opens the same view, and the filters a person saves.

/** What the platform says of a service target (Ticket `targets`: respond, resolve). */
export type TargetState = "running" | "paused" | "met" | "met_late" | "breached";

export interface Ticket {
  id: string;
  version?: number;
  number?: string | null;
  subject?: string | null;
  status?: string | null;
  priority?: string | null;
  channel?: string | null;
  team?: string | null;
  customer_name?: string | null;
  contact_name?: string | null;
  contact_email?: string | null;
  assigned_to?: string | null;
  sla_status?: string | null;
  response_by?: string | null;
  resolution_by?: string | null;
  first_responded_at?: string | null;
  opened_at?: string | null;
  last_customer_message_at?: string | null;
  last_agent_message_at?: string | null;
  last_message_preview?: string | null;
  /** The assignment rule that placed it, and when (`routed_by` is the rule's id; its name is in `_titles`). */
  routed_by?: string | null;
  routed_at?: string | null;
  response_state?: TargetState | null;
  resolution_state?: TargetState | null;
  _titles?: Record<string, string>;
}

/** The list an agent came from, carried by the link that opens a ticket, so next and previous walk it. */
export interface Carried {
  order?: string[];
  /** The inbox's address (its query string) when the ticket was opened, to come back to the same view. */
  search?: string;
}

/** Where "Inbox" leads back to: the view the ticket was opened from, else the inbox as it starts. */
export function inboxLink(c: Carried): string {
  return c.search ? `/?${c.search}` : "/";
}

/** Statuses that still need someone: Open waits for an agent, Replied for the customer. */
export const ACTIVE = ["Open", "Replied", "On Hold"];
/** SLA states that need attention, the worse first. */
export const SLA_ALERTS = ["Breached", "At Risk"];
const PRIORITY = ["Urgent", "High", "Medium", "Low"];
export const PRIORITIES = ["Low", "Medium", "High", "Urgent"];
export const CHANNELS = ["Email", "Portal", "Phone", "Chat", "Web Form"];

const rank = (list: string[], v?: string | null) => {
  const i = v ? list.indexOf(v) : -1;
  return i < 0 ? list.length : i;
};
const time = (s?: string | null) => {
  const t = s ? Date.parse(s) : Number.NaN;
  return Number.isNaN(t) ? Number.POSITIVE_INFINITY : t;
};

/** When it is due next: its first response until someone has answered, then its resolution. */
export function dueAt(t: Ticket): string | null {
  return (t.first_responded_at ? t.resolution_by : t.response_by) ?? null;
}

/** Most pressing first: a breached SLA, then one at risk; tickets waiting for an agent before
 *  those waiting for the customer; then priority, the earliest due, the oldest. */
export function byUrgency(a: Ticket, b: Ticket): number {
  const due = time(dueAt(a)) - time(dueAt(b));
  const opened = time(a.opened_at) - time(b.opened_at);
  return (
    rank(SLA_ALERTS, a.sla_status) - rank(SLA_ALERTS, b.sla_status) ||
    Number(a.status !== "Open") - Number(b.status !== "Open") ||
    rank(PRIORITY, a.priority) - rank(PRIORITY, b.priority) ||
    (Number.isNaN(due) ? 0 : due) ||
    (Number.isNaN(opened) ? 0 : opened)
  );
}

/** "35 min", "3 h", "4 d": a stretch of time as a person says it. */
export function span(ms: number): string {
  const mins = Math.max(0, Math.round(Math.abs(ms) / 60_000));
  return mins < 60 ? tr("{mins} min", { mins }) : mins < 48 * 60 ? `${Math.round(mins / 60)} h` : `${Math.round(mins / 1440)} d`;
}

/** "overdue by 2 h", "due in 35 min", "due in 3 d"; null without a due time. */
export function dueLabel(due: string | null | undefined, now = Date.now()): string | null {
  const at = due ? Date.parse(due) : Number.NaN;
  if (Number.isNaN(at)) return null;
  const ms = at - now;
  return ms < 0 ? tr("overdue by {ms}", { ms: span(ms) }) : tr("due in {ms}", { ms: span(ms) });
}

/** Who a ticket is from, as an agent reads it. */
export function requester(t: Ticket): string {
  return t.customer_name || t.contact_name || t.contact_email || "";
}

/** The letters in a person's avatar, from their name or their address. */
export function initials(who?: string | null): string {
  const base = (who ?? "").includes("@") ? (who ?? "").split("@")[0] : (who ?? "");
  const parts = base.split(/[\s._-]+/).filter(Boolean);
  return (parts.length > 1 ? parts[0][0] + parts[1][0] : (parts[0] ?? "?").slice(0, 2)).toUpperCase();
}

// ── how long a ticket has waited ──

/** Who a ticket waits for, and since when: an open ticket for an agent, from the customer's last
 *  message; a replied one for the customer, from the team's. A ticket on hold waits for no one. */
export function waiting(t: Ticket): { on: "agent" | "customer"; since: string } | null {
  if (t.status === "Open") {
    const since = t.last_customer_message_at ?? t.opened_at;
    return since ? { on: "agent", since } : null;
  }
  if (t.status === "Replied") {
    const since = t.last_agent_message_at ?? t.first_responded_at ?? t.opened_at;
    return since ? { on: "customer", since } : null;
  }
  return null;
}

/** "Waiting 3 h" on an agent, "Customer's turn 1 d" on the customer; null when no one is waited for. */
export function waitLabel(t: Ticket, now = Date.now()): string | null {
  const w = waiting(t);
  const since = w ? Date.parse(w.since) : Number.NaN;
  if (!w || Number.isNaN(since)) return null;
  return `${w.on === "agent" ? "Waiting" : "Customer's turn"} ${span(now - since)}`;
}

// ── queues ──

export type QueueId = "mine" | "unassigned" | "alerts" | "teams" | "open";

export const QUEUES: { id: QueueId; label: string; empty: string }[] = [
  { id: "mine", get label() { return tr("Mine"); }, get empty() { return tr("Nothing assigned to you is open. Well done."); } },
  { id: "unassigned", get label() { return tr("Unassigned"); }, get empty() { return tr("Every open ticket has someone on it."); } },
  { id: "alerts", get label() { return tr("SLA alerts"); }, get empty() { return tr("No ticket is at risk of missing its SLA."); } },
  { id: "teams", get label() { return tr("My teams"); }, get empty() { return tr("Your teams have no open tickets."); } },
  { id: "open", get label() { return tr("All open"); }, get empty() { return tr("No open tickets."); } },
];

export interface Who {
  /** The signed-in agent's address. */
  me: string;
  /** The teams they belong to (ids). */
  teams: string[];
}

/** What the server is asked for a queue: tickets still being worked on, narrowed as the queue says. */
export function queueFilters(queue: QueueId, who: Who): unknown[] {
  const active = ["status", "in", ACTIVE];
  switch (queue) {
    case "mine":
      return [active, ["assigned_to", "=", who.me.toLowerCase()]];
    case "unassigned":
      return [active, ["assigned_to", "is", "not set"]];
    case "alerts":
      return [active, ["sla_status", "in", SLA_ALERTS]];
    case "teams":
      // A person in no team has no such queue; an impossible filter keeps the list honest if asked.
      return [active, ["team", "in", who.teams.length ? who.teams : ["00000000-0000-0000-0000-000000000000"]]];
    case "open":
      return [active];
  }
}

// ── the inbox in its address ──

export type SortId = "urgent" | "waiting" | "newest";

export const SORTS: { id: SortId; label: string }[] = [
  { id: "urgent", get label() { return tr("Most pressing"); } },
  { id: "waiting", get label() { return tr("Longest waiting"); } },
  { id: "newest", get label() { return tr("Newest"); } },
];

/** The target states a queue can be narrowed to, as the platform keeps them on a ticket. */
export const TARGET_STATES = ["running", "paused", "met", "met_late", "breached"] as const;

export interface TargetPick {
  /** `response:breached`: which target and the state it is in. */
  id: string;
  label: string;
}

/** The choices of the "By target" filter: a target and a state. */
export const TARGET_PICKS: TargetPick[] = (["response", "resolution"] as const).flatMap((k) =>
  TARGET_STATES.map((st) => ({
    id: `${k}:${st}`,
    get label() {
      const target = k === "response" ? tr("First response") : tr("Resolution");
      const state = { running: tr("running"), paused: tr("paused"), met: tr("met"), met_late: tr("met late"), breached: tr("breached") }[st];
      return `${target} ${state}`;
    },
  })),
);

/** The server's filter for a target pick, or null for one that is not. */
export function targetFilter(id: string): unknown[] | null {
  const m = /^(response|resolution):(running|paused|met|met_late|breached)$/.exec(id);
  return m ? [`${m[1]}_state`, "=", m[2]] : null;
}

export interface Refine {
  /** Words to find in the subject, the requester and the customer. */
  q: string;
  priority: string;
  team: string;
  channel: string;
  /** An agent's address. */
  assignee: string;
  /** A tag the ticket carries. */
  tag: string;
  /** A target and its state: one of `TARGET_PICKS`. */
  target: string;
}

export interface InboxState extends Refine {
  queue: QueueId;
  sort: SortId;
}

export const NO_REFINE: Refine = { q: "", priority: "", team: "", channel: "", assignee: "", tag: "", target: "" };
export const START: InboxState = { queue: "mine", sort: "urgent", ...NO_REFINE };

const REFINE_KEYS = Object.keys(NO_REFINE) as (keyof Refine)[];

const oneOf = <T extends string>(v: string | null, all: readonly T[], otherwise: T): T => (all.includes(v as T) ? (v as T) : otherwise);

export function readState(sp: URLSearchParams): InboxState {
  const state: InboxState = {
    ...START,
    queue: oneOf(sp.get("queue"), QUEUES.map((q) => q.id), START.queue),
    sort: oneOf(sp.get("sort"), SORTS.map((s) => s.id), START.sort),
  };
  for (const k of REFINE_KEYS) state[k] = (sp.get(k) ?? "").trim();
  return state;
}

/** The address of a state, leaving out what is the start: `?queue=unassigned&priority=High`. */
export function writeState(state: InboxState): URLSearchParams {
  const sp = new URLSearchParams();
  if (state.queue !== START.queue) sp.set("queue", state.queue);
  if (state.sort !== START.sort) sp.set("sort", state.sort);
  for (const k of REFINE_KEYS) if (state[k]) sp.set(k, state[k]);
  return sp;
}

export const isRefined = (r: Refine) => REFINE_KEYS.some((k) => !!r[k].trim());

/** The quick filters as the server reads them. (The words go as `search`.) */
export function refineFilters(r: Refine): unknown[] {
  const out: unknown[] = [];
  if (r.priority) out.push(["priority", "=", r.priority]);
  if (r.team) out.push(["team", "=", r.team]);
  if (r.channel) out.push(["channel", "=", r.channel]);
  if (r.assignee) out.push(["assigned_to", "=", r.assignee.toLowerCase()]);
  if (r.tag) out.push(["_tag", "=", r.tag.toLowerCase()]);
  const target = r.target ? targetFilter(r.target) : null;
  if (target) out.push(target);
  return out;
}

/** The order a queue is shown in. The server sends the oldest first; the other orders are made here. */
export function sortTickets(rows: Ticket[], sort: SortId): Ticket[] {
  const list = [...rows];
  switch (sort) {
    case "urgent":
      return list.sort(byUrgency);
    case "newest":
      return list.sort((a, b) => time(b.opened_at) - time(a.opened_at));
    case "waiting": {
      // Whoever waits on an agent longest first; those waiting on the customer or on hold after them.
      const key = (t: Ticket) => {
        const w = waiting(t);
        return [w?.on === "agent" ? 0 : 1, time(w?.since)] as const;
      };
      return list.sort((a, b) => key(a)[0] - key(b)[0] || key(a)[1] - key(b)[1]);
    }
  }
}

// ── who has what ──

/** How many tickets each agent has, the most first; tickets nobody has are left out. */
export function workload(rows: { assigned_to?: string | null }[]): { who: string; count: number }[] {
  const by = new Map<string, number>();
  for (const r of rows) if (r.assigned_to) by.set(r.assigned_to, (by.get(r.assigned_to) ?? 0) + 1);
  return [...by.entries()].map(([who, count]) => ({ who, count })).sort((a, b) => b.count - a.count || a.who.localeCompare(b.who));
}

// ── selection and the keyboard's place ──

/** Toggles a ticket in the selection; with `range` (Shift held) every ticket between the one clicked
 *  last (`anchor`) and this one, in the order shown, takes its new state. */
export function toggle(selected: ReadonlySet<string>, order: string[], id: string, anchor: string | null, range: boolean): Set<string> {
  const next = new Set(selected);
  const on = !selected.has(id);
  const a = anchor === null ? -1 : order.indexOf(anchor);
  const b = order.indexOf(id);
  const ids = range && a >= 0 && b >= 0 ? order.slice(Math.min(a, b), Math.max(a, b) + 1) : [id];
  for (const x of ids) {
    if (on) next.add(x);
    else next.delete(x);
  }
  return next;
}

/** The ticket `delta` places from `current` in the order shown, stopping at the ends; the first when
 *  nothing is current yet. */
export function step(order: string[], current: string | null, delta: 1 | -1): string | null {
  if (!order.length) return null;
  const i = current === null ? -1 : order.indexOf(current);
  if (i < 0) return order[delta === 1 ? 0 : order.length - 1];
  return order[Math.min(order.length - 1, Math.max(0, i + delta))];
}

// ── filters a person saves ──

export interface SavedFilter {
  id: string;
  name: string;
  /** The inbox's address for it, as `writeState` makes it. */
  query: Record<string, string>;
}

/** What is wrong with a name for a saved filter, in words a person can act on; null when it is fine. */
export function filterNameError(name: string, saved: SavedFilter[], except?: string): string | null {
  const n = name.trim();
  if (!n) return tr("Give it a name.");
  if (n.length > 60) return tr("Keep the name to 60 characters.");
  if (saved.some((f) => f.id !== except && f.name.toLowerCase() === n.toLowerCase())) return tr("You have a saved filter with that name.");
  return null;
}

export function stateOf(f: SavedFilter): InboxState {
  return readState(new URLSearchParams(f.query));
}

export function saveFilter(saved: SavedFilter[], name: string, state: InboxState, id: string): SavedFilter[] {
  return [...saved, { id, name: name.trim(), query: Object.fromEntries(writeState(state)) }];
}

/** Whether the inbox shows what a saved filter saved. */
export function isFilter(f: SavedFilter, state: InboxState): boolean {
  const a = writeState(stateOf(f)).toString();
  return a === writeState(state).toString();
}

/** Who wrote the last message the customer could read, so a preview is not mistaken for the other side's. */
export function lastSpeaker(t: Ticket): "Customer" | "Team" | null {
  const [c, a] = [Date.parse(t.last_customer_message_at ?? ""), Date.parse(t.last_agent_message_at ?? "")];
  if (Number.isNaN(c) && Number.isNaN(a)) return t.last_message_preview ? "Customer" : null;
  return Number.isNaN(a) || c >= a ? "Customer" : "Team";
}

/** Saved filters as stored in the person's preferences, which may be older, newer or damaged: only
 *  what is well formed is kept. */
export function savedFrom(value: unknown): SavedFilter[] {
  const list = (value as { filters?: unknown } | null)?.filters;
  if (!Array.isArray(list)) return [];
  return list.flatMap((f): SavedFilter[] => {
    const o = f as { id?: unknown; name?: unknown; query?: unknown } | null;
    if (!o || typeof o.id !== "string" || typeof o.name !== "string" || !o.query || typeof o.query !== "object") return [];
    const query = Object.fromEntries(Object.entries(o.query as Record<string, unknown>).filter((e): e is [string, string] => typeof e[1] === "string"));
    return [{ id: o.id, name: o.name, query }];
  });
}

export interface BulkResult {
  succeeded: number;
  failed: number;
  results: { id: string; ok: boolean; error?: { message?: string } }[];
}

/** What to tell the agent after a change to many tickets: a sentence, whether it went wrong, and which
 *  tickets still need doing (they stay selected). `label` names a ticket by its number. */
export function describeBulk(done: string, res: BulkResult, label: (id: string) => string): { message: string; bad: boolean; left: string[] } {
  const failed = res.results.filter((r) => !r.ok);
  if (!failed.length) return { message: `${done}: ${res.succeeded} ${res.succeeded === 1 ? "ticket" : "tickets"}`, bad: false, left: [] };
  const why = failed
    .slice(0, 3)
    .map((r) => `${label(r.id)}: ${r.error?.message ?? "not changed"}`)
    .join(". ");
  const more = failed.length > 3 ? tr(" And {value} more.", { value: failed.length - 3 }) : "";
  const lead = res.succeeded ? tr("{done}: {succeeded} of {value}. ", { done, succeeded: res.succeeded, value: res.succeeded + failed.length }) : tr("Nothing changed. ");
  return { message: `${lead}${why}.${more}`, bad: true, left: failed.map((r) => r.id) };
}
