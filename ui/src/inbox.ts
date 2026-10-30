// The agent inbox's order (kept apart from the page, so it is tested): which tickets need someone
// first, and when each is due.

export interface Ticket {
  id: string;
  number?: string | null;
  subject?: string | null;
  status?: string | null;
  priority?: string | null;
  customer_name?: string | null;
  contact_name?: string | null;
  contact_email?: string | null;
  assigned_to?: string | null;
  sla_status?: string | null;
  response_by?: string | null;
  resolution_by?: string | null;
  first_responded_at?: string | null;
  opened_at?: string | null;
}

/** Statuses that still need someone: Open waits for an agent, Replied for the customer. */
export const ACTIVE = ["Open", "Replied", "On Hold"];
/** SLA states that need attention, the worse first. */
export const SLA_ALERTS = ["Breached", "At Risk"];
const PRIORITY = ["Urgent", "High", "Medium", "Low"];

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

/** "overdue by 2 h", "due in 35 min", "due in 3 d"; null without a due time. */
export function dueLabel(due: string | null | undefined, now = Date.now()): string | null {
  const at = due ? Date.parse(due) : Number.NaN;
  if (Number.isNaN(at)) return null;
  const ms = at - now;
  const mins = Math.round(Math.abs(ms) / 60_000);
  const span = mins < 60 ? `${mins} min` : mins < 48 * 60 ? `${Math.round(mins / 60)} h` : `${Math.round(mins / 1440)} d`;
  return ms < 0 ? `overdue by ${span}` : `due in ${span}`;
}

/** Who a ticket is from, as an agent reads it. */
export function requester(t: Ticket): string {
  return t.customer_name || t.contact_name || t.contact_email || "";
}
