// What the screens draw, checked without a browser by rendering their parts to markup: that they draw
// without failing, and say what a person needs to read (who spoke, what was emailed, what is wrong).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { setTimeZone } from "@ferrum/client";
import type { EntityMeta } from "@ferrum/records";
import { Composer } from "./Composer";
import { Conversation } from "./Conversation";
import { buildFeed, type Story } from "./feed";
import { day, when } from "./Portal";
import { MergeDialog } from "./Merge";
import { Queues } from "./Queues";
import { Rail, type RailTicket } from "./Rail";
import { relationKeys, Relations } from "./Relations";
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

describe("a row and the rule that placed it", () => {
  it("says which assignment rule put the ticket where it is", () => {
    expect(row(ticket({ routed_by: "r1", _titles: { routed_by: "Billing by skill" } }))).toContain("placed by Billing by skill");
    expect(row(ticket())).not.toContain("placed by");
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
        tags={[{ tag: "refund", colour: "#2563eb" }, { tag: "vip" }]}
        onTag={() => {}}
        targets={[{ id: "response:breached", label: "First response breached", count: 4 }]}
        onTarget={() => {}}
      />,
    );

  it("narrow by tag, with the tag's colour, and by target state, with how many are in it", () => {
    const html = draw({ ...START, queue: "open", tag: "refund", target: "response:breached" });
    expect(html).toContain("By tag");
    expect(html).toContain("refund");
    expect(html).toContain("--tag:#2563eb");
    expect(html).toContain("By target");
    expect(html).toContain("First response breached");
    expect(html).toContain('aria-pressed="true"');
    expect(draw(START, false)).not.toContain("By tag");
  });

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
        cc={[]}
        assignee={null}
        agent="priya@acme.test"
        onSignature={() => {}}
        replies
        conflict={null}
        onSend={async () => true}
        {...o}
      />,
    );

  it("writes to the customer, says where it goes, and offers saved replies", () => {
    const html = draw();
    expect(html).toContain("Public reply");
    expect(html).toContain("emailed to Ada");
    expect(html).toContain("&lt;ada@shop.test&gt;");
    expect(html).toContain("Saved replies");
    expect(html).toContain("Send reply by email");
    expect(html).toContain("Internal note");
  });

  it("names the copies a reply carries", () => {
    const html = draw({ cc: ["bob@shop.test", "eve@shop.test"] });
    expect(html).toContain("and copied to");
    expect(html).toContain("bob@shop.test, eve@shop.test");
  });

  it("says plainly when there is no address to email, and who will lose the ticket by being answered over", () => {
    const bare = draw({ to: { name: "Ada", email: null } });
    expect(bare).toContain("nothing is emailed");
    expect(bare).toContain("Add reply to the portal");
    expect(bare).not.toContain("Send reply by email");
    expect(draw({ assignee: "Priya has this ticket." })).toContain("Priya has this ticket. Sending takes it over.");
  });

  it("is a note for the team in note mode, without saved replies", () => {
    const html = draw({ mode: "note" });
    expect(html).toContain("It is never emailed, copied or shown to the customer");
    expect(html).toContain("Add internal note, not sent");
    expect(html).not.toContain("Send reply");
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
    response_state: "running",
    resolution_by: "2099-01-02T10:00:00Z",
    resolution_state: "running",
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

  it("shows who is copied, and the rating with its comment when there is one", () => {
    expect(draw({ cc: "bob@acme.test, eve@acme.test" })).toContain("eve@acme.test");
    expect(draw({ cc: "bob@acme.test" })).toContain("Copy another address");
    const rated = draw({ satisfaction_rating: 2, feedback: "Took too long.", rated_at: "2026-11-05T10:00:00Z" });
    expect(rated).toContain("2 of 5");
    expect(rated).toContain("Took too long.");
    expect(draw()).not.toContain("Satisfaction");
  });

  it("leaves the copies and tags unchanged for someone who may not", () => {
    const html = page(<Rail meta={{ name: "Ticket" } as unknown as EntityMeta} t={{ ...t, cc: "bob@acme.test" }} me="priya@acme.test" people={[]} name={name} editable={false} save={async () => {}} />);
    expect(html).toContain("bob@acme.test");
    expect(html).not.toContain("Copy another address");
    expect(html).not.toContain("Add a tag");
  });

  it("says which rule placed the ticket, and when, with a link to it", () => {
    const html = draw({ routed_by: "r1", routed_at: "2026-11-05T10:00:00Z", reassigned_count: 1, _titles: { routed_by: "Billing by skill" } });
    expect(html).toContain("Placed by the assignment rule");
    expect(html).toContain('href="/e/TicketAssignmentRule/r1"');
    expect(html).toContain("Billing by skill");
    expect(html).toContain("moved on 1 times");
    expect(draw()).not.toContain("Placed by the assignment rule");
  });

  it("keeps a former agent who still has a ticket among the choices", () => {
    expect(draw()).toContain("raj@acme.test");
  });

  it("offers the workspace's own statuses when there are any, and not when there are none", () => {
    const seeded = (statuses: unknown[], extra: Partial<RailTicket> = {}) => {
      const qc = new QueryClient();
      qc.setQueryData(["helpdesk", "statuses"], statuses);
      return renderToStaticMarkup(
        <QueryClientProvider client={qc}>
          <MemoryRouter>
            <Rail meta={{ name: "Ticket", linked_from: [] } as unknown as EntityMeta} t={{ ...t, ...extra }} me="priya@acme.test" people={[]} name={name} editable save={async () => {}} />
          </MemoryRouter>
        </QueryClientProvider>,
      );
    };
    const html = seeded([
      { id: "s1", status_name: "Waiting on supplier", category: "On Hold", enabled: true },
      { id: "s2", status_name: "Fixed in next release", category: "Resolved", enabled: true },
    ]);
    expect(html).toContain('id="hd-custom-status"');
    expect(html).toContain("Waiting on supplier");
    expect(html).not.toContain("Fixed in next release");
    expect(seeded([])).not.toContain('id="hd-custom-status"');
    expect(seeded([], { custom_status: "s9", _titles: { custom_status: "Old bucket" } })).toContain("Old bucket");
  });

  it("changes nothing for someone who may not", () => {
    const html = page(<Rail meta={{ name: "Ticket" } as unknown as EntityMeta} t={t} me="priya@acme.test" people={[]} name={name} editable={false} save={async () => {}} />);
    expect(html).not.toContain("Assign to me");
    expect(html).toContain("disabled");
  });
});

describe("related tickets", () => {
  const draw = (t: Parameters<typeof Relations>[0]["t"], seed: (qc: QueryClient) => void, editable = true) => {
    // A failed list stays failed when drawn (no fetch again on mount, as there is none on the server either).
    const qc = new QueryClient({ defaultOptions: { queries: { retryOnMount: false } } });
    seed(qc);
    return renderToStaticMarkup(
      <QueryClientProvider client={qc}>
        <MemoryRouter>
          <Relations t={t} editable={editable} save={async () => {}} />
        </MemoryRouter>
      </QueryClientProvider>,
    );
  };
  const seeded = (qc: QueryClient) => {
    qc.setQueryData(relationKeys.children("t1"), [{ id: "c1", number: "TKT-2", subject: "Pune office", status: "Open" }]);
    qc.setQueryData(relationKeys.linkedHere("t1"), [{ id: "h1", number: "TKT-3", subject: "Links here", status: "Replied", links: [{ id: "r9", ticket: "t1" }] }]);
    qc.setQueryData(relationKeys.linked(["l1"]), [{ id: "l1", number: "TKT-4", subject: "Linked from here", status: "Resolved" }]);
  };
  const parentOf = (qc: QueryClient) => {
    qc.setQueryData(relationKeys.children("t1"), []);
    qc.setQueryData(relationKeys.linkedHere("t1"), []);
    qc.setQueryData(relationKeys.parent("p1"), { id: "p1", number: "TKT-1", subject: "Outage", status: "Open" });
  };

  it("shows the parent, the children and the links both ways", () => {
    const html = draw({ id: "t1", links: [{ id: "r1", ticket: "l1" }] }, seeded);
    for (const words of ["Related tickets", "Child tickets", "TKT-2", "Pune office", "Linked", "TKT-3", "Links here", "TKT-4", "Linked from here", "Resolved"]) expect(html).toContain(words);
    const child = draw({ id: "t1", parent_ticket: "p1" }, parentOf);
    expect(child).toContain("Parent");
    expect(child).toContain("Outage");
  });

  it("offers no child ticket to a child", () => {
    const html = draw({ id: "t1", parent_ticket: "p1" }, parentOf);
    expect(html).not.toContain("New child ticket");
    expect(html).toContain("Remove from its parent");
    expect(draw({ id: "t1" }, seeded)).toContain("New child ticket");
  });

  it("shows nothing to change to someone who may only read", () => {
    const html = draw({ id: "t1", links: [{ id: "r1", ticket: "l1" }] }, seeded, false);
    expect(html).toContain("Pune office");
    for (const words of ["New child ticket", "Link a ticket", "Remove"]) expect(html).not.toContain(words);
  });

  it("says when the lists could not be loaded", () => {
    const html = draw({ id: "t1" }, (qc) => {
      qc.getQueryCache().build(qc, { queryKey: relationKeys.children("t1") }).setState({ status: "error", error: new Error("down"), fetchStatus: "idle" });
    });
    expect(html).toContain("They could not be loaded.");
  });

  it("offers to split a customer's message, once", () => {
    const feed = buildFeed(
      {
        ...story,
        conversation: [
          { id: "m1", author_kind: "Customer", body: "Also my invoice.", sent_at: "2026-11-01T10:00:00Z" },
          { id: "m2", author_kind: "Agent", author: "priya@acme.test", body: "On it.", sent_at: "2026-11-01T10:05:00Z" },
          { id: "m3", author_kind: "Agent", author: "priya@acme.test", body: "Note.", is_internal: true, sent_at: "2026-11-01T10:06:00Z" },
          { id: "m4", author_kind: "Customer", body: "Split already.", sent_at: "2026-11-01T10:07:00Z", split_into: "TKT-2026-000042" },
        ],
      },
      [],
      name,
    );
    const html = page(<Conversation items={feed} filter="all" onFilter={() => {}} address={null} endRef={null} onSplit={() => {}} />);
    expect(html.split("Split into a new ticket").length - 1).toBe(1);
    expect(html).toContain("Split into TKT-2026-000042");
    expect(page(<Conversation items={feed} filter="all" onFilter={() => {}} address={null} endRef={null} />)).not.toContain("Split into a new ticket");
  });
});

describe("resolving", () => {
  const draw = (o: Partial<React.ComponentProps<typeof ResolveDialog>> = {}) =>
    page(<ResolveDialog open status="Resolved" what="TKT-1" onClose={() => {}} onSubmit={async () => true} {...o} />);

  it("asks how it was resolved, and offers to tell the customer when there is someone to tell", () => {
    expect(draw()).toContain("How it was resolved");
    expect(draw()).not.toContain("Also send this");
    expect(draw({ tell: "Ada" })).toContain("Also send this to Ada as a reply");
    expect(draw()).not.toContain("Resolved as");
    expect(draw({ resolvedAs: [{ id: "s2", status_name: "Fixed in next release" }] })).toContain("Fixed in next release");
    expect(draw({ status: "Closed", resolvedAs: [{ id: "s2", status_name: "Fixed in next release" }] })).not.toContain("Resolved as");
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

describe("the portal's times", () => {
  it("are on the workspace's clock, not the browser's", () => {
    // 11:00 UTC on 5 Nov is 01:00 on 6 Nov in Kiritimati (UTC+14), and still 5 Nov in any browser west of UTC+13.
    const at = "2026-11-05T11:00:00Z";
    setTimeZone("Pacific/Kiritimati");
    try {
      expect(day(at)).toBe(new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeZone: "Pacific/Kiritimati" }).format(new Date(at)));
      expect(when(at)).toContain(new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short", timeZone: "Pacific/Kiritimati" }).format(new Date(at)));
    } finally {
      setTimeZone(undefined);
    }
  });
});
