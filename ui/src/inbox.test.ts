import { describe, expect, it } from "vitest";
import {
  byUrgency,
  dueAt,
  dueLabel,
  filterNameError,
  initials,
  isFilter,
  isRefined,
  queueFilters,
  readState,
  refineFilters,
  requester,
  saveFilter,
  sortTickets,
  span,
  START,
  step,
  stateOf,
  type Ticket,
  toggle,
  waitLabel,
  waiting,
  workload,
  writeState,
} from "./inbox";

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
    expect([span(59 * 60_000), span(90 * 60_000), span(47 * 3_600_000), span(72 * 3_600_000), span(-5 * 60_000)]).toEqual(["59 min", "2 h", "47 h", "3 d", "5 min"]);
  });

  it("names who it is from", () => {
    expect(requester(t("a", { customer_name: "Acme", contact_name: "Ada" }))).toBe("Acme");
    expect(requester(t("b", { contact_email: "ada@acme.test" }))).toBe("ada@acme.test");
  });

  it("gives an avatar its letters from a name or an address", () => {
    expect([initials("priya.nair@acme.test"), initials("Priya Nair"), initials("raj@acme.test"), initials(null)]).toEqual(["PN", "PN", "RA", "?"]);
  });
});

describe("how long a ticket has waited", () => {
  const now = Date.parse("2026-11-01T12:00:00Z");
  it("waits on an agent from the customer's last message, else from when it was opened", () => {
    expect(waiting(t("a", { last_customer_message_at: "2026-11-01T09:00:00Z", opened_at: "2026-10-30T00:00:00Z" }))).toEqual({ on: "agent", since: "2026-11-01T09:00:00Z" });
    expect(waiting(t("b", { opened_at: "2026-10-30T00:00:00Z" }))).toEqual({ on: "agent", since: "2026-10-30T00:00:00Z" });
    expect(waitLabel(t("a", { last_customer_message_at: "2026-11-01T09:00:00Z" }), now)).toBe("Waiting 3 h");
  });

  it("waits on the customer once the team has answered, and on no one while on hold", () => {
    expect(waitLabel(t("a", { status: "Replied", last_agent_message_at: "2026-10-31T12:00:00Z" }), now)).toBe("Customer's turn 24 h");
    expect(waiting(t("b", { status: "On Hold" }))).toBeNull();
    expect(waitLabel(t("c", { status: "Resolved" }), now)).toBeNull();
  });
});

describe("the queues", () => {
  const who = { me: "Priya@Acme.test", teams: ["team-1", "team-2"] };
  it("ask the server for what still needs someone, narrowed as each says", () => {
    const active = ["status", "in", ["Open", "Replied", "On Hold"]];
    expect(queueFilters("mine", who)).toEqual([active, ["assigned_to", "=", "priya@acme.test"]]);
    expect(queueFilters("unassigned", who)).toEqual([active, ["assigned_to", "is", "not set"]]);
    expect(queueFilters("alerts", who)).toEqual([active, ["sla_status", "in", ["Breached", "At Risk"]]]);
    expect(queueFilters("teams", who)).toEqual([active, ["team", "in", ["team-1", "team-2"]]]);
    expect(queueFilters("open", who)).toEqual([active]);
  });

  it("show nothing for My teams to someone in no team, rather than everything", () => {
    const [, team] = queueFilters("teams", { me: "x@y.test", teams: [] }) as [unknown, [string, string, string[]]];
    expect(team[2]).toHaveLength(1);
    expect(team[2][0]).toMatch(/^0+-0+-0+-0+-0+$/);
  });
});

describe("the inbox in its address", () => {
  it("opens on the start when the address says nothing, and ignores what it does not know", () => {
    expect(readState(new URLSearchParams(""))).toEqual(START);
    expect(readState(new URLSearchParams("queue=nonsense&sort=nonsense"))).toEqual(START);
  });

  it("round-trips a view and leaves out what is the start", () => {
    const state = { ...START, queue: "unassigned" as const, sort: "waiting" as const, priority: "High", q: "printer jam" };
    const sp = writeState(state);
    expect(sp.toString()).toBe("queue=unassigned&sort=waiting&q=printer+jam&priority=High");
    expect(readState(sp)).toEqual(state);
    expect(writeState(START).toString()).toBe("");
  });

  it("turns the quick filters into what the server is asked, and knows when any is on", () => {
    expect(refineFilters({ q: "x", priority: "High", team: "t1", channel: "Email", assignee: "Raj@Acme.test" })).toEqual([
      ["priority", "=", "High"],
      ["team", "=", "t1"],
      ["channel", "=", "Email"],
      ["assigned_to", "=", "raj@acme.test"],
    ]);
    expect(refineFilters(START)).toEqual([]);
    expect(isRefined(START)).toBe(false);
    expect(isRefined({ ...START, q: " " })).toBe(false);
    expect(isRefined({ ...START, channel: "Chat" })).toBe(true);
  });
});

describe("the order of a queue", () => {
  const rows = [
    t("old-replied", { status: "Replied", opened_at: "2026-10-01T00:00:00Z", last_agent_message_at: "2026-10-30T00:00:00Z" }),
    t("new", { opened_at: "2026-11-01T08:00:00Z", last_customer_message_at: "2026-11-01T08:00:00Z" }),
    t("long-wait", { opened_at: "2026-10-20T00:00:00Z", last_customer_message_at: "2026-10-31T00:00:00Z" }),
    t("hold", { status: "On Hold", opened_at: "2026-10-05T00:00:00Z" }),
  ];
  it("can be the newest first", () => {
    expect(sortTickets(rows, "newest").map((x) => x.id)).toEqual(["new", "long-wait", "hold", "old-replied"]);
  });
  it("can be those who wait on an agent longest first, the rest after them", () => {
    expect(sortTickets(rows, "waiting").map((x) => x.id)).toEqual(["long-wait", "new", "old-replied", "hold"]);
  });
  it("does not change the list it is given", () => {
    const before = rows.map((x) => x.id);
    sortTickets(rows, "newest");
    expect(rows.map((x) => x.id)).toEqual(before);
  });
});

describe("who has what", () => {
  it("counts each agent's tickets, the most first, leaving out tickets nobody has", () => {
    expect(workload([{ assigned_to: "b@x.test" }, { assigned_to: "a@x.test" }, { assigned_to: "b@x.test" }, { assigned_to: null }, {}, { assigned_to: "c@x.test" }])).toEqual([
      { who: "b@x.test", count: 2 },
      { who: "a@x.test", count: 1 },
      { who: "c@x.test", count: 1 },
    ]);
  });
});

describe("selecting tickets", () => {
  const order = ["a", "b", "c", "d", "e"];
  it("toggles one, and a range from the one before when Shift is held", () => {
    const one = toggle(new Set(), order, "b", null, false);
    expect([...one]).toEqual(["b"]);
    expect([...toggle(one, order, "d", "b", true)].sort()).toEqual(["b", "c", "d"]);
    expect([...toggle(new Set(["b", "c", "d"]), order, "c", "b", false)].sort()).toEqual(["b", "d"]);
  });

  it("takes a range in either direction", () => {
    expect([...toggle(new Set(), order, "b", "d", true)].sort()).toEqual(["b", "c", "d"]);
  });
});

describe("the keyboard's place in the list", () => {
  const order = ["a", "b", "c"];
  it("starts at the first going down and at the last going up, and stops at the ends", () => {
    expect(step(order, null, 1)).toBe("a");
    expect(step(order, null, -1)).toBe("c");
    expect(step(order, "a", 1)).toBe("b");
    expect(step(order, "c", 1)).toBe("c");
    expect(step(order, "a", -1)).toBe("a");
    expect(step(order, "gone", 1)).toBe("a");
    expect(step([], "a", 1)).toBeNull();
  });
});

describe("saved filters", () => {
  const saved = saveFilter([], "  Urgent email ", { ...START, priority: "Urgent", channel: "Email" }, "f1");
  it("keep the view they saved under the name given", () => {
    expect(saved).toEqual([{ id: "f1", name: "Urgent email", query: { priority: "Urgent", channel: "Email" } }]);
    expect(stateOf(saved[0])).toEqual({ ...START, priority: "Urgent", channel: "Email" });
  });

  it("are recognised when the inbox shows them", () => {
    expect(isFilter(saved[0], { ...START, priority: "Urgent", channel: "Email" })).toBe(true);
    expect(isFilter(saved[0], { ...START, priority: "Urgent" })).toBe(false);
  });

  it("need a name that is not already taken", () => {
    expect(filterNameError("", saved)).toBe("Give it a name.");
    expect(filterNameError("x".repeat(61), saved)).toBe("Keep the name to 60 characters.");
    expect(filterNameError("URGENT EMAIL", saved)).toBe("You have a saved filter with that name.");
    expect(filterNameError("URGENT EMAIL", saved, "f1")).toBeNull();
    expect(filterNameError("Chat today", saved)).toBeNull();
  });
});
