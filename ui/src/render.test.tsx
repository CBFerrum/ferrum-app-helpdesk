// What the screens draw, checked without a browser by rendering their parts to markup: that they draw
// without failing, and say what a person needs to read (who spoke, what was emailed, what is wrong).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import type { EntityMeta } from "@ferrum/records";
import { Composer } from "./Composer";
import { Conversation } from "./Conversation";
import { buildFeed, type Story } from "./feed";
import { MergeDialog } from "./Merge";
import { Queues } from "./Queues";
import { Rail, type RailTicket } from "./Rail";
import { ResolveDialog } from "./Resolve";
import { Row } from "./Row";
import { START, type Ticket } from "./inbox";

const page = (node: React.ReactNode) =>
  renderToStaticMarkup(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>{node}</MemoryRouter>
    </QueryClientProvider>,
  );
const name = (email: string) => ({ "priya@acme.test": "Priya Nair" })[email] ?? email;

const story: Story = {
  subject: "Printer jam",
  description: "It jams on <every> page.",
  opened_at: "2026-11-01T09:00:00Z",
  contact_name: "Ada",
  contact_email: "ada@shop.test",
  response_breached: true,
  response_by: "2026-11-01T13:00:00Z",
  status_history: [
    { status: "Open", at: "2026-11-01T09:00:00Z" },
    { status: "On Hold", at: "2026-11-01T12:00:00Z" },
  ],
  conversation: [
    { id: "1", author_kind: "Agent", author: "priya@acme.test", body: "Try the **tray**.", sent_at: "2026-11-01T10:00:00Z", emailed: true, attachments: [{ id: "f1", file_name: "fix.pdf", size: 2048 }] },
    { id: "2", author_kind: "Agent", author: "priya@acme.test", body: "Probably the roller.", is_internal: true, sent_at: "2026-11-01T11:00:00Z" },
    { id: "3", author_kind: "Agent", author: "priya@acme.test", body: "Called her.", sent_at: "2026-11-01T11:30:00Z" },
  ],
};

describe("the conversation", () => {
  const feed = buildFeed(story, [], name);
  const show = (filter: "all" | "replies" | "notes") => page(<Conversation items={feed} filter={filter} onFilter={() => {}} address="ada@shop.test" endRef={null} />);

  it("shows who spoke, what was emailed and what was not, files, and what happened between", () => {
    const html = show("all");
    expect(html).toContain("Ada");
    expect(html).toContain("Priya Nair");
    expect(html).toContain("Emailed to the customer");
    expect(html).toContain('title="Emailed to ada@shop.test"');
    expect(html).toContain("Not sent by email");
    expect(html).toContain("fix.pdf");
    expect(html).toContain("Internal note");
    expect(html).toContain("Status: Open → On Hold");
    expect(html).toContain("The first response target was missed");
  });

  it("writes the customer's words as text, not as markup", () => {
    expect(show("all")).toContain("It jams on &lt;every&gt; page.");
  });

  it("narrows to replies or to notes, and says so when there are no notes", () => {
    expect(show("notes")).toContain("Probably the roller.");
    expect(show("notes")).not.toContain("Try the");
    expect(show("replies")).not.toContain("Probably the roller.");
    const none = page(<Conversation items={buildFeed({ ...story, conversation: [] }, [], name)} filter="notes" onFilter={() => {}} address={null} endRef={null} />);
    expect(none).toContain("No internal notes yet");
  });
});

const ticket = (extra: Partial<Ticket> = {}): Ticket => ({
  id: "t1",
  number: "TKT-2026-000123",
  subject: "Printer jam",
  status: "Open",
  priority: "Urgent",
  channel: "Portal",
  contact_name: "Ada",
  opened_at: "2026-11-01T09:00:00Z",
  last_message_preview: "It jams on every page.",
  response_by: "2026-11-01T10:00:00Z",
  sla_status: "Breached",
  ...extra,
});

const row = (t: Ticket, o: Partial<React.ComponentProps<typeof Row>> = {}) =>
  page(
    <ul>
      <Row t={t} now={Date.parse("2026-11-01T12:00:00Z")} me="priya@acme.test" who={name} picked={false} current={false} writable carried={{}} onPick={() => {}} onTake={() => {}} onHold={() => {}} onResolve={() => {}} {...o} />
    </ul>,
  );

describe("a row of the inbox", () => {
  it("says what the ticket is, how long it has waited, and that its service level is missed", () => {
    const html = row(ticket());
    expect(html).toContain("Printer jam");
    expect(html).toContain("TKT-2026-000123");
    expect(html).toContain("Urgent");
    expect(html).toContain("Breached");
    expect(html).toContain("overdue by 2 h");
    expect(html).toContain("Waiting 3 h");
    expect(html).toContain("It jams on every page.");
  });

  it("offers Take for a ticket nobody has, and says who has one that someone does", () => {
    expect(row(ticket())).toContain("Take");
    const owned = row(ticket({ assigned_to: "priya@acme.test" }));
    expect(owned).toContain("You have this ticket");
    expect(owned).not.toContain(">Take<");
    expect(row(ticket({ assigned_to: "raj@acme.test" }))).toContain("raj@acme.test has this ticket");
  });

  it("does not offer changes to someone who may only read", () => {
    const html = row(ticket(), { writable: false });
    expect(html).not.toContain(">Take<");
    expect(html).toContain("Nobody");
  });

  it("marks the keyboard's place and the selection", () => {
    const html = row(ticket(), { current: true, picked: true });
    expect(html).toContain("data-current");
    expect(html).toContain("data-picked");
  });
});

describe("the queues", () => {
  const queues = [
    { id: "mine" as const, label: "Mine" },
    { id: "unassigned" as const, label: "Unassigned" },
  ];
  const draw = (state = START, more = true) =>
    page(
      <Queues
        state={state}
        queues={queues}
        counts={{ mine: 12, unassigned: 0 }}
        saved={[{ id: "s1", name: "Urgent email", query: { priority: "Urgent", channel: "Email" } }]}
        onRemove={() => {}}
        people={[{ who: "priya@acme.test", count: 7 }, { who: "raj@acme.test", count: 3 }]}
        name={name}
        onPerson={() => {}}
        showMore={more}
      />,
    );

  it("count each queue, mark the one shown, and list the saved filters and who has what", () => {
    const html = draw();
    expect(html).toContain("Mine");
    expect(html).toContain(">12<");
    expect(html).toContain('aria-current="page"');
    expect(html).toContain("Urgent email");
    expect(html).toContain("Priya Nair");
    expect(html).toContain(">7<");
  });

  it("fold the saved filters and the people away on a narrow screen", () => {
    const html = draw(START, false);
    expect(html).toContain("Mine");
    expect(html).not.toContain("Urgent email");
    expect(html).not.toContain("Who has what");
  });
});

describe("the composer", () => {
  const draw = (o: Partial<React.ComponentProps<typeof Composer>> = {}) =>
    page(
      <Composer
        ticket="t1"
        mode="reply"
        onMode={() => {}}
        inputRef={{ current: null }}
        rootRef={null}
        to={{ name: "Ada", email: "ada@shop.test" }}
        assignee={null}
        about={{ ticket: {}, contact: {}, agent: {} }}
        replies
        conflict={null}
        onSend={async () => true}
        {...o}
      />,
    );

  it("writes to the customer, says where it goes, and offers saved replies", () => {
    const html = draw();
    expect(html).toContain("To Ada &lt;ada@shop.test&gt;");
    expect(html).toContain("Saved replies");
    expect(html).toContain("Send reply");
    expect(html).toContain("Internal note");
  });

  it("says plainly when there is no address to email, and who will lose the ticket by being answered over", () => {
    expect(draw({ to: { name: "Ada", email: null } })).toContain("No email address on this ticket");
    expect(draw({ assignee: "Priya has this ticket." })).toContain("Priya has this ticket. Sending takes it over.");
  });

  it("is a note for the team in note mode, without saved replies", () => {
    const html = draw({ mode: "note" });
    expect(html).toContain("Only your team sees this");
    expect(html).toContain("Add note");
    expect(html).not.toContain("Saved replies");
  });

  it("tells the agent when the ticket changed under them", () => {
    expect(draw({ conflict: "This ticket changed while you were writing." })).toContain("This ticket changed while you were writing.");
  });
});

describe("the details beside a ticket", () => {
  const t: RailTicket = {
    id: "t1",
    version: 3,
    number: "TKT-1",
    status: "Open",
    priority: "High",
    customer: "c1",
    customer_name: "Acme Corp",
    contact_name: "Ada",
    contact_email: "ada@acme.test",
    channel: "Email",
    assigned_to: "raj@acme.test",
    service_level_agreement: "sla",
    response_by: "2099-01-01T10:00:00Z",
    resolution_by: "2099-01-02T10:00:00Z",
    entitlement: "Exhausted",
    escalation_level: 2,
    support_contract: "k1",
    _titles: { support_contract: "Gold" },
  };
  const draw = (extra: Partial<RailTicket> = {}) =>
    page(<Rail meta={{ name: "Ticket", linked_from: [] } as unknown as EntityMeta} t={{ ...t, ...extra }} me="priya@acme.test" people={[{ id: "u1", full_name: "Priya Nair", email: "priya@acme.test" }]} name={name} editable save={async () => {}} />);

  it("shows the service levels, who has the ticket, the customer and what they are entitled to", () => {
    const html = draw();
    expect(html).toContain("First response");
    expect(html).toContain("Resolution");
    expect(html).toContain("Escalated to level 2");
    expect(html).toContain("Support contract has no tickets left");
    expect(html).toContain("ada@acme.test");
    expect(html).toContain("Assign to me");
  });

  it("keeps a former agent who still has a ticket among the choices", () => {
    expect(draw()).toContain("raj@acme.test");
  });

  it("changes nothing for someone who may not", () => {
    const html = page(<Rail meta={{ name: "Ticket" } as unknown as EntityMeta} t={t} me="priya@acme.test" people={[]} name={name} editable={false} save={async () => {}} />);
    expect(html).not.toContain("Assign to me");
    expect(html).toContain("disabled");
  });
});

describe("resolving", () => {
  const draw = (o: Partial<React.ComponentProps<typeof ResolveDialog>> = {}) =>
    page(<ResolveDialog open status="Resolved" what="TKT-1" onClose={() => {}} onSubmit={async () => true} {...o} />);

  it("asks how it was resolved, and offers to tell the customer when there is someone to tell", () => {
    expect(draw()).toContain("How it was resolved");
    expect(draw()).not.toContain("Also send this");
    expect(draw({ tell: "Ada" })).toContain("Also send this to Ada as a reply");
  });

  it("warns that closing is final", () => {
    expect(draw({ status: "Closed" })).toContain("A closed ticket is final");
    expect(draw()).not.toContain("A closed ticket is final");
  });
});

describe("merging", () => {
  const mk = (id: string, over: Partial<Ticket> = {}): Ticket => ({ id, number: `TKT-${id}`, subject: `Jam ${id}`, status: "Open", customer_name: "Acme", opened_at: `2026-11-0${id}T09:00:00Z`, ...over });
  const draw = (o: Partial<React.ComponentProps<typeof MergeDialog>> = {}) =>
    page(<MergeDialog open tickets={[mk("1"), mk("2")]} onClose={() => {}} onDone={() => {}} {...o} />);

  it("from the inbox, lets the agent choose the ticket that stays and says what happens to the others", () => {
    const html = draw();
    expect(html).toContain("The ticket that stays");
    expect(html).toContain('type="radio"');
    expect(html).toContain("Merge 1 ticket");
    expect(html).toContain("TKT-2 then closes as merged");
    expect(html).toContain("This cannot be undone");
  });

  it("says why tickets of two customers are not merged, and does not offer to", () => {
    const html = draw({ tickets: [mk("1"), mk("2", { customer_name: "Globex" })] });
    expect(html).toContain("is another customer&#x27;s: only tickets of one customer are merged");
    expect(html).toContain("disabled");
  });

  it("from a ticket, offers the customer's other tickets to tick", () => {
    const html = draw({ keep: "1", tickets: [mk("1"), mk("2"), mk("3", { status: "Closed" })] });
    expect(html).toContain("Merge tickets into TKT-1");
    expect(html).toContain('type="checkbox"');
    expect(html).toContain("(closed)");
    expect(html).toContain("Merge 0 tickets");
  });
});
