// The two target cards of a ticket, "First response" and "Resolution", read from the platform's own
// target states (`response_state`, `resolution_state`: running, paused, met, met_late, breached). They
// rest in plain grey ("Due in 3 h") and take a colour only when something has happened: met (green),
// missed (red), paused (blue). Pure, so the wording is tested without a browser.

import { formatMoment, t as tr } from "@ferrum/client";
import { span } from "./inbox";

/** What the platform says of a target (Ticket `targets`: respond, resolve). */
export type TargetState = "running" | "paused" | "met" | "met_late" | "breached";

export interface SlaTicket {
  status?: string | null;
  service_level_agreement?: string | null;
  response_by?: string | null;
  resolution_by?: string | null;
  first_responded_at?: string | null;
  resolved_at?: string | null;
  response_state?: TargetState | null;
  resolution_state?: TargetState | null;
  /** The target, in working seconds, at the ticket's priority. */
  response_secs?: number | null;
  resolution_secs?: number | null;
  /** Working hours the first response and the resolution took, as the platform counted them. */
  first_response_hours?: string | number | null;
  resolution_hours?: string | number | null;
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

/** Working hours as a person says them: 3.33 is "3 h 20 min", 0.5 is "30 min". */
export function hoursText(hours: string | number | null | undefined): string | null {
  const h = typeof hours === "number" ? hours : Number(hours);
  if (hours === null || hours === undefined || hours === "" || !Number.isFinite(h) || h < 0) return null;
  const mins = Math.round(h * 60);
  if (mins < 60) return tr("{mins} min", { mins });
  const [hh, mm] = [Math.floor(mins / 60), mins % 60];
  return mm ? tr("{hh} h {mm} min", { hh, mm }) : `${hh} h`;
}

const when = (t: number) => formatMoment(t, { weekday: "short", day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });

interface Target {
  state: TargetState | null | undefined;
  done: string | null | undefined;
  due: string | null | undefined;
  took: string | number | null | undefined;
  secs: number | null | undefined;
  doneWord: string;
  status: string | null | undefined;
}

function card(id: SlaCard["id"], title: string, o: Target, sla: boolean, now: number): SlaCard {
  const done = at(o.done);
  const due = at(o.due);
  const size = o.secs ? hoursText(o.secs / 3600) : null;
  const goal = size ? tr("Target {size} of working time", { size }) : null;
  const lines = (...l: (string | null)[]) => l.filter(Boolean).join(" · ") || null;
  switch (o.state) {
    case "met": {
      const took = hoursText(o.took);
      return { id, title, state: "fulfilled", text: took ? tr("Met in {took}", { took }) : tr("Met"), detail: lines(done !== null ? `${o.doneWord} ${when(done)}` : null, goal) };
    }
    case "met_late": {
      const late = done !== null && due !== null && done > due ? tr("Met late, by {value}", { value: span(done - due) }) : tr("Met late");
      return { id, title, state: "late", text: late, detail: lines(done !== null ? `${o.doneWord} ${when(done)}` : null, due !== null ? tr("Was due {due}", { due: when(due) }) : null) };
    }
    case "breached":
      return {
        id,
        title,
        state: "overdue",
        text: due !== null ? tr("Breached: overdue by {value}", { value: span(Math.max(0, now - due)) }) : tr("Breached"),
        detail: lines(due !== null ? tr("Was due {due}", { due: when(due) }) : null, goal),
      };
    case "paused":
      // The clock stops while the ticket waits on the customer or is on hold, as the agreement says.
      return {
        id,
        title,
        state: "paused",
        text: o.status === "Replied" ? tr("Paused: waiting for the customer") : o.status === "On Hold" ? tr("Paused: on hold") : tr("Paused"),
        detail: lines(tr("The clock is stopped"), goal),
      };
    case "running":
      if (due === null) return { id, title, state: "none", text: tr("Running"), detail: goal };
      return due < now
        ? { id, title, state: "overdue", text: tr("Overdue by {value}", { value: span(now - due) }), detail: lines(tr("Was due {due}", { due: when(due) }), goal) }
        : { id, title, state: "due", text: tr("Due in {value}", { value: span(due - now) }), detail: lines(tr("Due {due}", { due: when(due) }), goal) };
    default:
      return { id, title, state: "none", text: sla ? tr("No deadline") : tr("No target"), detail: null };
  }
}

/** The ticket's two cards as of `now`. */
export function slaCards(t: SlaTicket, now = Date.now()): SlaCard[] {
  const sla = !!t.service_level_agreement;
  return [
    card("response", "First response", { state: t.response_state, done: t.first_responded_at, due: t.response_by, took: t.first_response_hours, secs: t.response_secs, doneWord: "Answered", status: t.status }, sla, now),
    card("resolution", "Resolution", { state: t.resolution_state, done: t.resolved_at, due: t.resolution_by, took: t.resolution_hours, secs: t.resolution_secs, doneWord: "Resolved", status: t.status }, sla, now),
  ];
}
