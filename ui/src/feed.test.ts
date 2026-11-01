import { describe, expect, it } from "vitest";
import { buildFeed, counts, dayKey, dayLabel, type EventItem, fileSize, filterFeed, type MessageItem, type Story } from "./feed";

const name = (email: string) => ({ "priya@acme.test": "Priya" })[email] ?? email;

const story = (extra: Partial<Story> = {}): Story => ({
  subject: "Printer jam",
  description: "It jams on every page.",
  opened_at: "2026-11-01T09:00:00Z",
  contact_name: "Ada",
  contact_email: "ada@shop.test",
  conversation: [],
  status_history: [{ status: "Open", at: "2026-11-01T09:00:00Z" }],
  ...extra,
});

const events = (feed: ReturnType<typeof buildFeed>) => feed.filter((i): i is EventItem => i.kind === "event");
const messages = (feed: ReturnType<typeof buildFeed>) => feed.filter((i): i is MessageItem => i.kind === "message");

describe("a ticket's feed", () => {
  it("starts with the request as the customer wrote it, then replies and notes in the order they were sent", () => {
    const feed = buildFeed(
      story({
        conversation: [
          { id: "2", author_kind: "Agent", author: "priya@acme.test", body: "Try the tray.", sent_at: "2026-11-01T10:00:00Z" },
          { id: "1", author_kind: "Agent", author: "priya@acme.test", body: "Checking with the vendor.", is_internal: true, sent_at: "2026-11-01T09:30:00Z" },
          { id: "3", author_kind: "Customer", body: "Still jams.", sent_at: "2026-11-01T11:00:00Z" },
        ],
      }),
      [],
      name,
    );
    expect(messages(feed).map((m) => [m.side, m.internal, m.author, m.body])).toEqual([
      ["customer", false, "Ada", "It jams on every page."],
      ["agent", true, "Priya", "Checking with the vendor."],
      ["agent", false, "Priya", "Try the tray."],
      ["customer", false, "Ada", "Still jams."],
    ]);
  });

  it("falls back to the subject when a ticket has no description, and to the address when nobody is named", () => {
    const feed = buildFeed(story({ description: null, contact_name: null, customer_name: null }), [], name);
    expect(messages(feed)[0]).toMatchObject({ author: "ada@shop.test", body: "Printer jam" });
  });

  it("shows files and whether a reply went out by email", () => {
    const feed = buildFeed(
      story({
        conversation: [
          { id: "1", author_kind: "Agent", author: "priya@acme.test", body: "See attached.", sent_at: "2026-11-01T10:00:00Z", emailed_to: "ada@shop.test", attachments: [{ id: "f1", file_name: "fix.pdf", size: 2048 }] },
        ],
      }),
      [],
      name,
    );
    expect(messages(feed)[1]).toMatchObject({ emailedTo: "ada@shop.test", attachments: [{ file_name: "fix.pdf" }] });
  });

  it("tells status changes from the ticket's own history, with who made them when the trail knows", () => {
    const feed = buildFeed(
      story({
        status_history: [
          { status: "Open", at: "2026-11-01T09:00:00Z" },
          { status: "On Hold", at: "2026-11-01T12:00:00Z" },
          { status: "Open", at: "2026-11-01T13:00:00Z" },
        ],
      }),
      [{ id: 1, at: "2026-11-01T12:00:01Z", actor_name: "Priya", action: "update", diff: { status: ["Open", "On Hold"] } }],
      name,
    );
    expect(events(feed).map((e) => [e.text, e.who])).toEqual([
      ["Status: Open → On Hold", "Priya"],
      ["Status: On Hold → Open", null],
    ]);
  });

  it("leaves out a status change that only follows from a reply in the same save", () => {
    const feed = buildFeed(
      story({
        conversation: [
          { id: "1", author_kind: "Agent", author: "priya@acme.test", body: "On it.", sent_at: "2026-11-01T10:00:00.400Z" },
          { id: "2", author_kind: "Customer", body: "Thanks!", sent_at: "2026-11-01T11:00:00Z" },
          { id: "3", author_kind: "Agent", author: "priya@acme.test", body: "Looking.", is_internal: true, sent_at: "2026-11-01T13:00:00Z" },
        ],
        status_history: [
          { status: "Open", at: "2026-11-01T09:00:00Z" },
          { status: "Replied", at: "2026-11-01T10:00:00.500Z" },
          { status: "Open", at: "2026-11-01T11:00:00.300Z" },
          { status: "Replied", at: "2026-11-01T13:00:00.300Z" },
        ],
      }),
      [],
      name,
    );
    // The last one was not the reply's doing: an internal note does not answer the customer.
    expect(events(feed).map((e) => e.text)).toEqual(["Status: Open → Replied"]);
  });

  it("puts how a ticket was resolved on its resolution", () => {
    const feed = buildFeed(
      story({
        resolution_details: "Replaced the roller.",
        status_history: [
          { status: "Open", at: "2026-11-01T09:00:00Z" },
          { status: "Resolved", at: "2026-11-01T15:00:00Z" },
          { status: "Closed", at: "2026-11-08T15:00:00Z" },
        ],
      }),
      [],
      name,
    );
    const [resolved, closed] = events(feed);
    expect([resolved.tone, resolved.detail]).toEqual(["good", "Replaced the roller."]);
    expect(closed.detail).toBeNull();
  });

  it("tells who has the ticket and when its priority moved, but not the priority it was created with", () => {
    const feed = buildFeed(
      story(),
      [
        { id: 1, at: "2026-11-01T09:00:00Z", action: "create", actor_name: "System", diff: { priority: [null, "Medium"], assigned_to: [null, "priya@acme.test"] } },
        { id: 2, at: "2026-11-01T10:00:00Z", action: "update", actor_name: "Priya", diff: { assigned_to: ["priya@acme.test", "raj@acme.test"], priority: ["Medium", "Urgent"] } },
        { id: 3, at: "2026-11-01T11:00:00Z", action: "update", actor_name: "Raj", diff: { assigned_to: { old: "raj@acme.test", new: null } } },
      ],
      name,
    );
    expect(events(feed).map((e) => [e.text, e.tone, e.who])).toEqual([
      ["Assigned to Priya", "plain", "System"],
      ["Reassigned from Priya to raj@acme.test", "plain", "Priya"],
      ["Priority: Medium → Urgent", "warn", "Priya"],
      ["Unassigned from raj@acme.test", "plain", "Raj"],
    ]);
  });

  it("tells the SLA: targets missed at the time they were due, and an escalation", () => {
    const feed = buildFeed(
      story({
        response_breached: true,
        response_by: "2026-11-01T13:00:00Z",
        resolution_breached: true,
        resolution_by: "2026-11-02T09:00:00Z",
        escalation_level: 2,
        escalated_at: "2026-11-01T14:00:00Z",
      }),
      [],
      name,
    );
    expect(events(feed).map((e) => [e.at, e.text, e.tone])).toEqual([
      ["2026-11-01T13:00:00Z", "The first response target was missed", "bad"],
      ["2026-11-01T14:00:00Z", "Escalated to level 2", "warn"],
      ["2026-11-02T09:00:00Z", "The resolution target was missed", "bad"],
    ]);
  });

  it("takes each escalation from the trail when it can, and does not tell the latest twice", () => {
    const feed = buildFeed(
      story({ escalation_level: 2, escalated_at: "2026-11-01T14:00:00Z" }),
      [
        { id: 1, at: "2026-11-01T12:00:00Z", action: "escalate", diff: { level: 1 } },
        { id: 2, at: "2026-11-01T14:00:00Z", action: "escalate", diff: { level: 2 } },
      ],
      name,
    );
    expect(events(feed).map((e) => e.text)).toEqual(["Escalated to level 1", "Escalated to level 2"]);
  });

  it("tells the customer's rating with what they wrote", () => {
    const feed = buildFeed(story({ feedback: "Quick and kind." }), [{ id: 1, at: "2026-11-02T10:00:00Z", action: "update", diff: { satisfaction_rating: [null, 5] } }], name);
    expect(events(feed)[0]).toMatchObject({ text: "The customer rated the help 5 of 5", tone: "good", detail: "Quick and kind." });
  });

  it("keeps messages ahead of events that fall at the same moment", () => {
    const feed = buildFeed(
      story({ response_breached: true, response_by: "2026-11-01T10:00:00Z", conversation: [{ id: "1", author_kind: "Customer", body: "Hello?", sent_at: "2026-11-01T10:00:00Z" }] }),
      [],
      name,
    );
    expect(feed.map((i) => i.kind)).toEqual(["message", "message", "event"]);
  });
});

describe("what the feed shows", () => {
  const feed = buildFeed(
    story({
      response_breached: true,
      response_by: "2026-11-01T13:00:00Z",
      conversation: [
        { id: "1", author_kind: "Agent", author: "priya@acme.test", body: "note", is_internal: true, sent_at: "2026-11-01T10:00:00Z" },
        { id: "2", author_kind: "Agent", author: "priya@acme.test", body: "reply", sent_at: "2026-11-01T11:00:00Z" },
      ],
    }),
    [],
    name,
  );

  it("can be narrowed to what the customer could read, or to the team's notes", () => {
    expect(filterFeed(feed, "all")).toHaveLength(4);
    expect(filterFeed(feed, "replies").map((i) => (i as MessageItem).body)).toEqual(["It jams on every page.", "reply"]);
    expect(filterFeed(feed, "notes").map((i) => (i as MessageItem).body)).toEqual(["note"]);
  });

  it("counts replies and notes for the filter's labels", () => {
    expect(counts(feed)).toEqual({ replies: 2, notes: 1 });
  });
});

describe("days and sizes", () => {
  const now = new Date(2026, 10, 10, 15, 0);
  it("names a day in the viewer's calendar", () => {
    expect(dayLabel(new Date(2026, 10, 10, 0, 5).toISOString(), now)).toBe("Today");
    expect(dayLabel(new Date(2026, 10, 9, 23, 55).toISOString(), now)).toBe("Yesterday");
    expect(dayLabel(new Date(2026, 10, 2, 12, 0).toISOString(), now)).toMatch(/2/);
    expect(dayLabel("not a date", now)).toBe("");
  });

  it("gives the same key to moments on one day and another to the next", () => {
    expect(dayKey(new Date(2026, 10, 9, 1, 0).toISOString())).toBe(dayKey(new Date(2026, 10, 9, 23, 0).toISOString()));
    expect(dayKey(new Date(2026, 10, 9, 23, 0).toISOString())).not.toBe(dayKey(new Date(2026, 10, 10, 1, 0).toISOString()));
  });

  it("says a file's size the way a person would", () => {
    expect([fileSize(500), fileSize(2048), fileSize(3 * 1024 * 1024), fileSize(null), fileSize(0)]).toEqual(["500 B", "2 KB", "3.0 MB", "", ""]);
  });
});
