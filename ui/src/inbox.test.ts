import { describe, expect, it } from "vitest";
import { byUrgency, dueAt, dueLabel, requester, type Ticket } from "./inbox";

const t = (id: string, extra: Partial<Ticket> = {}): Ticket => ({ id, status: "Open", priority: "Medium", ...extra });

describe("the agent inbox", () => {
  it("puts SLA trouble first, then tickets waiting for an agent, then priority, due and age", () => {
    const tickets = [
      t("replied-urgent", { status: "Replied", priority: "Urgent" }),
      t("low", { priority: "Low" }),
      t("at-risk", { sla_status: "At Risk", priority: "Low" }),
      t("high-later", { priority: "High", response_by: "2026-11-02T10:00:00Z" }),
      t("breached", { sla_status: "Breached", priority: "Low" }),
      t("high-sooner", { priority: "High", response_by: "2026-11-01T10:00:00Z" }),
      t("high-no-sla", { priority: "High", opened_at: "2026-10-01T00:00:00Z" }),
    ];
    expect([...tickets].sort(byUrgency).map((x) => x.id)).toEqual([
      "breached",
      "at-risk",
      "high-sooner",
      "high-later",
      "high-no-sla",
      "low",
      "replied-urgent",
    ]);
  });

  it("is due for a response until someone answers, then for the resolution", () => {
    expect(dueAt(t("a", { response_by: "R", resolution_by: "S" }))).toBe("R");
    expect(dueAt(t("b", { response_by: "R", resolution_by: "S", first_responded_at: "x" }))).toBe("S");
    expect(dueAt(t("c"))).toBeNull();
  });

  it("says when in words", () => {
    const now = Date.parse("2026-11-01T12:00:00Z");
    expect(dueLabel("2026-11-01T12:35:00Z", now)).toBe("due in 35 min");
    expect(dueLabel("2026-11-01T09:00:00Z", now)).toBe("overdue by 3 h");
    expect(dueLabel("2026-11-05T12:00:00Z", now)).toBe("due in 4 d");
    expect(dueLabel(null, now)).toBeNull();
    expect(dueLabel("soon", now)).toBeNull();
  });

  it("names who it is from", () => {
    expect(requester(t("a", { customer_name: "Acme", contact_name: "Ada" }))).toBe("Acme");
    expect(requester(t("b", { contact_email: "ada@acme.test" }))).toBe("ada@acme.test");
  });
});
