// Time on a ticket (Helpdesk 0.22): the total, the entries, and a small form to log minutes, typed or from a timer that
// lives in this browser. Entries are lines of the ticket, saved through its ordinary update; the server keeps them each
// to their author (or someone who may delete tickets) and works out the total.

import { type FormEvent, useState } from "react";
import { t as tr, isolate } from "@ferrum/client";
import { Badge, Button, confirmDialog } from "@ferrum/kit";
import { useNow, useRemembered } from "./hooks";
import { hoursText } from "./sla";

export interface TimeEntry {
  id: string;
  agent: string;
  spent_on: string;
  minutes: number;
  billable?: boolean | null;
  note?: string | null;
}

/** The whole minutes a timer started at `startedAt` has run by `now`: rounded up, at least 1, at most a day (1440). */
export function timerMinutes(startedAt: number, now: number): number {
  return Math.min(1440, Math.max(1, Math.ceil((now - startedAt) / 60_000)));
}

export function Time({
  ticket,
  entries,
  total,
  me,
  manager,
  editable,
  name,
  save,
}: {
  ticket: string;
  entries: TimeEntry[];
  total: number | null | undefined;
  /** The signed-in person's address. */
  me: string;
  /** Whether they may change anyone's entries (the right to delete tickets). */
  manager: boolean;
  editable: boolean;
  name: (email: string) => string;
  save: (field: string, value: unknown) => Promise<void>;
}) {
  const [minutes, setMinutes] = useState("");
  const [note, setNote] = useState("");
  const [billable, setBillable] = useState(false);
  const [started, setStarted] = useRemembered<string>(`helpdesk:timer:${ticket}`, "");
  const now = useNow(15_000);
  const kept = entries.map((r) => ({ id: r.id }));
  const n = Number(minutes);
  const ready = Number.isInteger(n) && n >= 1 && n <= 1440;
  const log = async (e: FormEvent) => {
    e.preventDefault();
    if (!ready) return;
    await save("time", [...kept, { minutes: n, note: note.trim() || null, billable }]);
    setMinutes("");
    setNote("");
    setBillable(false);
  };
  const remove = async (r: TimeEntry) => {
    const ok = await confirmDialog({ title: tr("Remove this time entry?"), message: tr("{minutes} on {day} goes from the ticket's total.", { minutes: hoursText(r.minutes / 60) ?? "", day: r.spent_on }), confirmLabel: tr("Remove it"), danger: true });
    if (ok) await save("time", kept.filter((k) => k.id !== r.id));
  };
  const stop = () => {
    setMinutes(String(timerMinutes(Number(started), Date.now())));
    setStarted("");
  };
  const sorted = [...entries].sort((a, b) => b.spent_on.localeCompare(a.spent_on));
  return (
    <section className="hd-card" aria-labelledby="hd-rail-time">
      <h3 id="hd-rail-time">{tr("Time")}</h3>
      <p>{total ? <strong>{isolate(hoursText(total / 60) ?? "")}</strong> : <span className="faint">{tr("No time logged yet.")}</span>}</p>
      {sorted.length > 0 && (
        <ul aria-labelledby="hd-rail-time">
          {sorted.map((r) => (
            <li key={r.id}>
              <span>{isolate([name(r.agent), r.spent_on, hoursText(r.minutes / 60)].filter(Boolean).join(" · "))}</span>
              {r.billable && <Badge tone="neutral">{tr("Billable")}</Badge>}
              {r.note && <span className="faint"> {isolate(r.note)}</span>}
              {editable && (manager || r.agent.toLowerCase() === me.toLowerCase()) && (
                <Button size="sm" onClick={() => void remove(r)}>
                  {tr("Remove")}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
      {editable && (
        <form className="stack" onSubmit={(e) => void log(e)}>
          <div className="row">
            <label className="field-label" htmlFor="hd-time-minutes">
              {tr("Minutes")}
            </label>
            <input id="hd-time-minutes" className="input" type="number" min={1} max={1440} step={1} value={minutes} onChange={(e) => setMinutes(e.target.value)} />
            {started ? (
              <Button size="sm" onClick={stop}>
                {tr("Stop ({minutes} min)", { minutes: timerMinutes(Number(started), now) })}
              </Button>
            ) : (
              <Button size="sm" onClick={() => setStarted(String(Date.now()))}>
                {tr("Start timer")}
              </Button>
            )}
          </div>
          <label className="field-label" htmlFor="hd-time-note">
            {tr("What for")}
          </label>
          <input id="hd-time-note" className="input" maxLength={300} value={note} onChange={(e) => setNote(e.target.value)} />
          <label className="hd-check">
            <input type="checkbox" checked={billable} onChange={(e) => setBillable(e.target.checked)} />
            {tr("Billable")}
          </label>
          <Button type="submit" disabled={!ready}>
            {tr("Log time")}
          </Button>
        </form>
      )}
    </section>
  );
}
