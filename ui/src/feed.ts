// One ticket's whole story in a single feed: the request as the customer wrote it, every reply and
// internal note, and between them what happened to the ticket (status, who has it, priority, the
// SLA, escalation). Pure, so it is tested without a browser.

import { formatMoment, shownTimeZone, t as tr } from "@ferrum/client";

export interface Attachment {
  id: string;
  file_name: string;
  content_type?: string | null;
  size?: number | null;
}

export interface Message {
  id?: string;
  author_kind?: string | null;
  author?: string | null;
  body?: string | null;
  is_internal?: boolean | null;
  sent_at?: string | null;
  attachments?: Attachment[] | null;
  /** A reply that went out by email to the ticket's contact. */
  emailed?: boolean | null;
  /** The number of the ticket this message came from, when that ticket was merged into this one. */
  merged_from?: string | null;
  /** The mail server did not verify the sender (a failed check, or no result): not proof of forgery. */
  sender_unverified?: boolean | null;
  /** The addresses copied on the message, a text of addresses the server read from the mail or put on the reply. */
  cc?: string | null;
}

export interface AuditEvent {
  id?: number;
  at: string;
  actor?: string | null;
  actor_name?: string | null;
  action: string;
  diff?: Record<string, unknown> | null;
}

/** What the feed reads of a ticket. */
export interface Story {
  subject?: string | null;
  description?: string | null;
  opened_at?: string | null;
  channel?: string | null;
  customer_name?: string | null;
  contact_name?: string | null;
  contact_email?: string | null;
  conversation?: Message[] | null;
  status_history?: { status: string; at: string }[] | null;
  response_by?: string | null;
  resolution_by?: string | null;
  response_breached?: boolean | null;
  resolution_breached?: boolean | null;
  escalation_level?: number | null;
  escalated_at?: string | null;
  resolution_details?: string | null;
  satisfaction_rating?: number | null;
  feedback?: string | null;
}

export interface MessageItem {
  kind: "message";
  key: string;
  at: string;
  side: "customer" | "agent";
  internal: boolean;
  author: string;
  /** The address behind `author`, for the avatar's initials. */
  email: string | null;
  body: string;
  attachments: Attachment[];
  emailed: boolean;
  /** The ticket it came from, when it was merged into this one. */
  mergedFrom: string | null;
  /** The mail server did not verify who sent it. */
  unverified: boolean;
  /** The addresses copied on it, if any. */
  cc: string | null;
}

export interface EventItem {
  kind: "event";
  key: string;
  at: string;
  /** How loudly it speaks: `bad` for a missed target, `warn` for an escalation. */
  tone: "plain" | "warn" | "bad" | "good";
  icon: "status" | "person" | "priority" | "clock" | "alert" | "star";
  text: string;
  who: string | null;
  detail: string | null;
}

export type FeedItem = MessageItem | EventItem;
export type FeedFilter = "all" | "replies" | "notes";

/** How close in time two things must be to have happened in one save. */
const SAME_SAVE_MS = 5_000;

const time = (s?: string | null) => {
  const t = s ? Date.parse(s) : Number.NaN;
  return Number.isNaN(t) ? null : t;
};

function changed(diff: unknown): [unknown, unknown] | null {
  if (Array.isArray(diff) && diff.length === 2) return [diff[0], diff[1]];
  if (diff && typeof diff === "object" && "new" in diff) return [(diff as { old?: unknown }).old, (diff as { new?: unknown }).new];
  return null;
}

const text = (v: unknown) => (typeof v === "string" && v ? v : null);

/** Who a ticket is from, as the feed names them. */
function requesterOf(t: Story): string {
  return t.contact_name || t.customer_name || t.contact_email || "Requester";
}

/** When a status moved only because of a message in the same save, the message says it: an agent's
 *  first reply marks a ticket replied, a customer's reply reopens it. */
function impliedByMessage(from: string, to: string, at: number, messages: Message[]): boolean {
  const near = (m: Message, customer: boolean) => {
    const sent = time(m.sent_at);
    return !m.is_internal && (m.author_kind === "Customer") === customer && sent !== null && Math.abs(sent - at) <= SAME_SAVE_MS;
  };
  if (from === "Open" && to === "Replied") return messages.some((m) => near(m, false));
  if (to === "Open") return messages.some((m) => near(m, true));
  return false;
}

/** The ticket's story, oldest first. `name` turns an address into the name people know them by. */
export function buildFeed(t: Story, events: AuditEvent[], name: (email: string) => string): FeedItem[] {
  const items: FeedItem[] = [];
  const messages = t.conversation ?? [];
  const who = (e: AuditEvent) => e.actor_name || null;

  const requester = requesterOf(t);
  items.push({
    kind: "message",
    key: "request",
    at: t.opened_at ?? "",
    side: "customer",
    internal: false,
    author: requester,
    email: t.contact_email ?? null,
    body: t.description || t.subject || "",
    attachments: [],
    emailed: false,
    mergedFrom: null,
    unverified: false,
    cc: null,
  });
  messages.forEach((m, i) => {
    const customer = m.author_kind === "Customer";
    items.push({
      kind: "message",
      key: `m-${m.id ?? i}`,
      at: m.sent_at ?? "",
      side: customer ? "customer" : "agent",
      internal: !!m.is_internal,
      author: customer ? requester : m.author ? name(m.author) : "Agent",
      email: customer ? (t.contact_email ?? null) : (m.author ?? null),
      body: m.body ?? "",
      attachments: m.attachments ?? [],
      emailed: !!m.emailed,
      mergedFrom: m.merged_from || null,
      unverified: !!m.sender_unverified,
      cc: m.cc?.trim() || null,
    });
  });

  // The last time the ticket was resolved (else closed) carries how it was resolved.
  const history = [...(t.status_history ?? [])].sort((a, b) => (time(a.at) ?? 0) - (time(b.at) ?? 0));
  const statuses = history.map((h) => h.status);
  const lastDone = statuses.includes("Resolved") ? statuses.lastIndexOf("Resolved") : statuses.lastIndexOf("Closed");
  history.forEach((h, i) => {
    const at = time(h.at);
    if (i === 0 || at === null) return;
    const from = history[i - 1].status;
    if (impliedByMessage(from, h.status, at, messages)) return;
    const by = events.find((e) => {
      const d = changed(e.diff?.status);
      const when = time(e.at);
      return d !== null && d[1] === h.status && when !== null && Math.abs(when - at) <= SAME_SAVE_MS;
    });
    items.push({
      kind: "event",
      key: `s-${i}`,
      at: h.at,
      tone: h.status === "Resolved" || h.status === "Closed" ? "good" : "plain",
      icon: "status",
      text: `Status: ${from} → ${h.status}`,
      who: by ? who(by) : null,
      detail: i === lastDone ? text(t.resolution_details) : null,
    });
  });

  for (const e of events) {
    const assigned = changed(e.diff?.assigned_to);
    if (assigned) {
      const [was, now] = [text(assigned[0]), text(assigned[1])];
      items.push({
        kind: "event",
        key: `e-${e.id ?? e.at}-owner`,
        at: e.at,
        tone: "plain",
        icon: "person",
        text: now ? (was ? tr("Reassigned from {was} to {now}", { was: name(was), now: name(now) }) : tr("Assigned to {now}", { now: name(now) })) : (was ? tr("Unassigned from {name}", { name: name(was) }) : tr("Unassigned from the agent")),
        who: who(e),
        detail: null,
      });
    }
    const priority = changed(e.diff?.priority);
    if (priority && e.action !== "create") {
      items.push({
        kind: "event",
        key: `e-${e.id ?? e.at}-priority`,
        at: e.at,
        tone: priority[1] === "Urgent" ? "warn" : "plain",
        icon: "priority",
        text: `Priority: ${text(priority[0]) ?? "none"} → ${text(priority[1]) ?? "none"}`,
        who: who(e),
        detail: null,
      });
    }
    const rating = changed(e.diff?.satisfaction_rating);
    if (rating && typeof rating[1] === "number") {
      items.push({
        kind: "event",
        key: `e-${e.id ?? e.at}-rating`,
        at: e.at,
        tone: rating[1] >= 4 ? "good" : rating[1] <= 2 ? "warn" : "plain",
        icon: "star",
        text: tr("The customer rated the help {value} of 5", { value: rating[1] }),
        who: null,
        detail: text(t.feedback),
      });
    }
    if (e.action === "escalate") {
      const level = (e.diff as { level?: unknown } | null)?.level;
      items.push({
        kind: "event",
        key: `e-${e.id ?? e.at}-escalated`,
        at: e.at,
        tone: "warn",
        icon: "alert",
        text: typeof level === "number" ? tr("Escalated to level {level}", { level }) : tr("Escalated"),
        who: null,
        detail: null,
      });
    }
  }

  // An escalation the audit trail could not show (it may be closed to this reader) is still on the ticket.
  if (t.escalated_at && !events.some((e) => e.action === "escalate")) {
    items.push({ kind: "event", key: "escalated", at: t.escalated_at, tone: "warn", icon: "alert", text: tr("Escalated to level {value}", { value: t.escalation_level ?? 1 }), who: null, detail: null });
  }
  if (t.response_breached && t.response_by) {
    items.push({ kind: "event", key: "response-missed", at: t.response_by, tone: "bad", icon: "clock", text: tr("The first response target was missed"), who: null, detail: null });
  }
  if (t.resolution_breached && t.resolution_by) {
    items.push({ kind: "event", key: "resolution-missed", at: t.resolution_by, tone: "bad", icon: "clock", text: tr("The resolution target was missed"), who: null, detail: null });
  }

  // Equal times keep the order things were added in: messages, then what happened around them.
  return items.map((item, i) => ({ item, i })).sort((a, b) => (time(a.item.at) ?? 0) - (time(b.item.at) ?? 0) || a.i - b.i).map((x) => x.item);
}

/** All of it, only what the customer could read (replies), or only the team's own notes. */
export function filterFeed(items: FeedItem[], filter: FeedFilter): FeedItem[] {
  if (filter === "replies") return items.filter((i) => i.kind === "message" && !i.internal);
  if (filter === "notes") return items.filter((i) => i.kind === "message" && i.internal);
  return items;
}

/** How many replies and notes the ticket has, for the filter's labels. */
export function counts(items: FeedItem[]): { replies: number; notes: number } {
  const messages = items.filter((i): i is MessageItem => i.kind === "message");
  return { replies: messages.filter((m) => !m.internal).length, notes: messages.filter((m) => m.internal).length };
}

/** The day instant `t` falls on in the workspace's calendar, the one its times are shown on: `YYYY-MM-DD`. */
const calendarDay = (t: number | Date) => new Intl.DateTimeFormat("en-CA", { timeZone: shownTimeZone(), year: "numeric", month: "2-digit", day: "2-digit" }).format(t);

/** "Today", "Yesterday", or the date, in the workspace's calendar. */
export function dayLabel(at: string, now = new Date()): string {
  const t = time(at);
  if (t === null) return "";
  const [day, today] = [calendarDay(t), calendarDay(now)];
  const diff = Math.round((Date.parse(today) - Date.parse(day)) / 86_400_000);
  if (diff === 0) return tr("Today");
  if (diff === 1) return tr("Yesterday");
  return formatMoment(t, { weekday: "short", day: "numeric", month: "short", ...(day.slice(0, 4) === today.slice(0, 4) ? {} : { year: "numeric" }) });
}

/** The day a moment falls on in the workspace's calendar, to put a divider where it changes. */
export function dayKey(at: string): string {
  const t = time(at);
  return t === null ? "" : calendarDay(t);
}

/** Names a file's size the way a person says it. */
export function fileSize(bytes?: number | null): string {
  if (!bytes || bytes < 0) return "";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return tr("{n} KB", { n: Math.max(1, Math.round(bytes / 1024)) });
  return tr("{value} MB", { value: (bytes / (1024 * 1024)).toFixed(1) });
}

/** The part of a ticket a colleague's change shows in. */
export interface Snapshot {
  status?: string | null;
  priority?: string | null;
  assigned_to?: string | null;
  conversation?: Message[] | null;
}

/** What a colleague changed while this ticket was open, in a line: new messages first, then who has it,
 *  then status and priority. Null when nothing changed that is worth a notice. */
export function whatChanged(before: Snapshot, after: Snapshot, name: (email: string) => string): string | null {
  const seen = new Set((before.conversation ?? []).map((m) => m.id));
  const fresh = (after.conversation ?? []).filter((m) => !seen.has(m.id));
  const parts: string[] = [];
  const last = fresh[fresh.length - 1];
  if (last) {
    const who = last.author_kind === "Customer" ? tr("The customer") : last.author ? name(last.author) : tr("A colleague");
    const what = last.is_internal ? tr("added a note") : "replied";
    parts.push(fresh.length > 1 ? tr("{length} new messages, the latest from {who}", { length: fresh.length, who }) : `${who} ${what}`);
  }
  if ((before.assigned_to ?? null) !== (after.assigned_to ?? null)) parts.push(after.assigned_to ? tr("Now with {value}", { value: name(after.assigned_to) }) : tr("No one has it now"));
  if (before.status !== after.status && after.status) parts.push(tr("Status is now {status}", { status: after.status }));
  if (before.priority !== after.priority && after.priority) parts.push(tr("Priority is now {priority}", { priority: after.priority }));
  return parts.length ? parts.join(". ") : null;
}
