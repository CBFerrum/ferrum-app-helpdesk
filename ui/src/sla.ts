// The two service-level cards of a ticket, "First response" and "Resolution". They rest in plain
// grey ("Due in 3 h") and take a colour only when something has happened: met (green), missed (red),
// paused (blue). Pure, so the wording is tested without a browser.

import { formatMoment } from "@ferrum/client";
import { span } from "./inbox";

export interface SlaTicket {
  status?: string | null;
  service_level_agreement?: string | null;
  response_by?: string | null;
  resolution_by?: string | null;
  first_responded_at?: string | null;
  resolved_at?: string | null;
  response_breached?: boolean | null;
  resolution_breached?: boolean | null;
  /** Business hours the first response and the resolution took, as the server counts them. */
  first_response_hours?: string | number | null;
  resolution_hours?: string | number | null;
  paused_hours?: string | number | null;
}

/** `due` and `none` are plain; the others are told apart by colour. */
export type SlaState = "none" | "due" | "paused" | "fulfilled" | "late" | "overdue";

export interface SlaCard {
  id: "response" | "resolution";
  title: string;
  state: SlaState;
  text: string;
  /** One more line for those who want to know when, in words. */
  detail: string | null;
}

const at = (s?: string | null) => {
  const t = s ? Date.parse(s) : Number.NaN;
  return Number.isNaN(t) ? null : t;
};

/** Business hours as a person says them: 3.33 is "3 h 20 min", 0.5 is "30 min". */
export function hoursText(hours: string | number | null | undefined): string | null {
  const h = typeof hours === "number" ? hours : Number(hours);
  if (hours === null || hours === undefined || hours === "" || !Number.isFinite(h) || h < 0) return null;
  const mins = Math.round(h * 60);
  if (mins < 60) return `${mins} min`;
  const [hh, mm] = [Math.floor(mins / 60), mins % 60];
  return mm ? `${hh} h ${mm} min` : `${hh} h`;
}

const when = (t: number) => formatMoment(t, { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });

function card(
  id: SlaCard["id"],
  title: string,
  o: { done: string | null | undefined; due: string | null | undefined; breached: boolean | null | undefined; took: string | number | null | undefined; sla: boolean; status: string | null | undefined; doneWord: string },
  now: number,
): SlaCard {
  const done = at(o.done);
  const due = at(o.due);
  if (done !== null) {
    if (o.breached) {
      const late = due !== null && done > due ? `Failed by ${span(done - due)}` : "Failed";
      return { id, title, state: "late", text: late, detail: `${o.doneWord} ${when(done)}${due !== null ? `, due ${when(due)}` : ""}` };
    }
    const took = hoursText(o.took);
    return { id, title, state: "fulfilled", text: took ? `Fulfilled in ${took}` : "Fulfilled", detail: `${o.doneWord} ${when(done)}` };
  }
  if (!o.sla) return { id, title, state: "none", text: "No target", detail: null };
  if (due === null) {
    // The clock stops while the ticket waits on the customer or is on hold, and its deadline with it.
    if (o.status === "On Hold") return { id, title, state: "paused", text: "On hold", detail: "The clock is stopped" };
    if (o.status === "Replied") return { id, title, state: "paused", text: "Waiting for the customer", detail: "The clock is stopped" };
    return { id, title, state: "none", text: "No deadline", detail: null };
  }
  return due < now
    ? { id, title, state: "overdue", text: `Overdue by ${span(now - due)}`, detail: `Was due ${when(due)}` }
    : { id, title, state: "due", text: `Due in ${span(due - now)}`, detail: `Due ${when(due)}` };
}

/** The ticket's two cards as of `now`. */
export function slaCards(t: SlaTicket, now = Date.now()): SlaCard[] {
  const sla = !!t.service_level_agreement;
  const paused = hoursText(t.paused_hours);
  const resolution = card("resolution", "Resolution", { done: t.resolved_at, due: t.resolution_by, breached: t.resolution_breached, took: t.resolution_hours, sla, status: t.status, doneWord: "Resolved" }, now);
  // What the clock was stopped for is worth knowing once it is not running any more.
  if (paused && Number(t.paused_hours) > 0 && resolution.state !== "paused") resolution.detail = [resolution.detail, `Stopped for ${paused} in all`].filter(Boolean).join(" · ");
  return [card("response", "First response", { done: t.first_responded_at, due: t.response_by, breached: t.response_breached, took: t.first_response_hours, sla, status: t.status, doneWord: "Answered" }, now), resolution];
}
