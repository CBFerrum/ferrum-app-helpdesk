import { describe, expect, it } from "vitest";
import { hoursText, slaCards, type SlaTicket } from "./sla";

const now = Date.parse("2026-11-02T12:00:00Z");
const base: SlaTicket = { status: "Open", service_level_agreement: "sla-1" };
const cards = (t: SlaTicket) => slaCards(t, now);

describe("the service-level cards", () => {
  it("rest in plain words while a target is ahead", () => {
    const [r, z] = cards({ ...base, response_by: "2026-11-02T15:00:00Z", resolution_by: "2026-11-04T12:00:00Z" });
    expect([r.title, r.state, r.text]).toEqual(["First response", "due", "Due in 3 h"]);
    expect([z.title, z.state, z.text]).toEqual(["Resolution", "due", "Due in 2 d"]);
  });

  it("turn red when a target has passed and no one has met it", () => {
    const [r] = cards({ ...base, response_by: "2026-11-02T11:01:00Z", response_breached: true });
    expect([r.state, r.text]).toEqual(["overdue", "Overdue by 59 min"]);
  });

  it("turn green with the time it took, counted in business hours", () => {
    const [r, z] = cards({ ...base, first_responded_at: "2026-11-02T10:00:00Z", first_response_hours: "3.33", resolved_at: "2026-11-02T11:00:00Z", resolution_hours: "6.00" });
    expect([r.state, r.text]).toEqual(["fulfilled", "Fulfilled in 3 h 20 min"]);
    expect([z.state, z.text]).toEqual(["fulfilled", "Fulfilled in 6 h"]);
  });

  it("say how late a target was met when it was met late", () => {
    const [r] = cards({ ...base, response_by: "2026-11-02T09:00:00Z", first_responded_at: "2026-11-02T09:20:00Z", response_breached: true });
    expect([r.state, r.text]).toEqual(["late", "Failed by 20 min"]);
    const [late] = cards({ ...base, first_responded_at: "2026-11-02T09:20:00Z", response_breached: true });
    expect(late.text).toBe("Failed");
  });

  it("turn blue while the clock is stopped, because the deadline is blank then", () => {
    const [r, z] = cards({ ...base, status: "On Hold" });
    expect([r.state, r.text, z.state]).toEqual(["paused", "On hold", "paused"]);
    const [waiting] = cards({ ...base, status: "Replied" });
    expect([waiting.state, waiting.text]).toEqual(["paused", "Waiting for the customer"]);
  });

  it("have nothing to say without an agreement, and a plain word for an open ticket with no deadline", () => {
    const [r] = cards({ status: "Open" });
    expect([r.state, r.text]).toEqual(["none", "No target"]);
    const [open] = cards(base);
    expect([open.state, open.text]).toEqual(["none", "No deadline"]);
  });

  it("remember how long the clock was stopped once the ticket is resolved", () => {
    const [, z] = cards({ ...base, status: "Resolved", resolved_at: "2026-11-02T11:00:00Z", resolution_hours: "5", paused_hours: "2.5" });
    expect(z.detail).toContain("Stopped for 2 h 30 min in all");
  });

  it("put the answer and the deadline in a line for those who want it", () => {
    const [r] = cards({ ...base, response_by: "2026-11-02T15:00:00Z" });
    expect(r.detail).toMatch(/^Due /);
  });
});

describe("business hours in words", () => {
  it("says minutes under an hour and hours and minutes above", () => {
    expect([hoursText("0.5"), hoursText(1), hoursText("3.33"), hoursText("41.25"), hoursText(null), hoursText("x"), hoursText("")]).toEqual(["30 min", "1 h", "3 h 20 min", "41 h 15 min", null, null, null]);
  });
});
