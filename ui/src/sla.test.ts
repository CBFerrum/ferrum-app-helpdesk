import { describe, expect, it } from "vitest";
import { hoursText, slaCards, type SlaTicket } from "./sla";

const now = Date.parse("2026-11-02T12:00:00Z");
const base: SlaTicket = { status: "Open", service_level_agreement: "sla-1" };
const cards = (t: SlaTicket) => slaCards(t, now);

describe("the target cards", () => {
  it("rest in plain words while a target is running", () => {
    const [r, z] = cards({ ...base, response_state: "running", response_by: "2026-11-02T15:00:00Z", resolution_state: "running", resolution_by: "2026-11-04T12:00:00Z" });
    expect([r.title, r.state, r.text]).toEqual(["First response", "due", "Due in 3 h"]);
    expect([z.title, z.state, z.text]).toEqual(["Resolution", "due", "Due in 2 d"]);
  });

  it("turn red once the platform calls a target breached", () => {
    const [r] = cards({ ...base, response_state: "breached", response_by: "2026-11-02T11:01:00Z" });
    expect([r.state, r.text]).toEqual(["overdue", "Breached: overdue by 59 min"]);
  });

  it("turn green with the working time it took", () => {
    const [r, z] = cards({ ...base, response_state: "met", first_responded_at: "2026-11-02T10:00:00Z", first_response_hours: "3.33", resolution_state: "met", resolved_at: "2026-11-02T11:00:00Z", resolution_hours: "6.00" });
    expect([r.state, r.text]).toEqual(["fulfilled", "Met in 3 h 20 min"]);
    expect([z.state, z.text]).toEqual(["fulfilled", "Met in 6 h"]);
  });

  it("say how late a target was met when it was met late", () => {
    const [r] = cards({ ...base, response_state: "met_late", response_by: "2026-11-02T09:00:00Z", first_responded_at: "2026-11-02T09:20:00Z" });
    expect([r.state, r.text]).toEqual(["late", "Met late, by 20 min"]);
    const [bare] = cards({ ...base, response_state: "met_late" });
    expect(bare.text).toBe("Met late");
  });

  it("turn blue while the clock is stopped, and say why", () => {
    const [r, z] = cards({ ...base, status: "On Hold", response_state: "paused", resolution_state: "paused" });
    expect([r.state, r.text, z.state]).toEqual(["paused", "Paused: on hold", "paused"]);
    const [waiting] = cards({ ...base, status: "Replied", response_state: "paused" });
    expect(waiting.text).toBe("Paused: waiting for the customer");
  });

  it("say what the target is in working time", () => {
    const [r, z] = cards({ ...base, response_state: "running", response_by: "2026-11-02T15:00:00Z", response_secs: 3600, resolution_secs: 14400, resolution_state: "paused" });
    expect(r.detail).toContain("Target 1 h of working time");
    expect(z.detail).toContain("Target 4 h of working time");
  });

  it("have nothing to say without a target", () => {
    const [r] = cards({ status: "Open" });
    expect([r.state, r.text]).toEqual(["none", "No target"]);
    const [open] = cards(base);
    expect([open.state, open.text]).toEqual(["none", "No deadline"]);
  });

  it("no longer read the hours the clock was stopped for, which the server stopped working out", () => {
    const [, z] = cards({ ...base, resolution_state: "met", resolved_at: "2026-11-02T11:00:00Z", resolution_hours: "5", paused_hours: 2.5 } as SlaTicket);
    expect(z.detail).not.toContain("Stopped for");
  });
});

describe("working hours in words", () => {
  it("says minutes under an hour and hours and minutes above", () => {
    expect([hoursText("0.5"), hoursText(1), hoursText("3.33"), hoursText("41.25"), hoursText(null), hoursText("x"), hoursText("")]).toEqual(["30 min", "1 h", "3 h 20 min", "41 h 15 min", null, null, null]);
  });
});
