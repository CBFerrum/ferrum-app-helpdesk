// The administration screens as they draw once their records are in the query cache, in the shape the routes answer
// them (Helpdesk's entities: SupportAddress, InboundLog, AgentProfile, TicketTag, TagRule, TicketAssignmentRule,
// SupportMacro, SupportSettings), and what a person with no right to them is told.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { AdminRoute } from "./Admin";

const ENTITIES = ["SupportAddress", "InboundLog", "AgentProfile", "TicketTag", "TagRule", "TicketAssignmentRule", "SupportMacro", "SupportSettings", "SupportTeam", "TicketType"];
const ALL = { read: "all", write: "all", create: "all", delete: "all" };

function draw(path: string, seed: (qc: QueryClient) => void = () => {}, perms: Record<string, string> = ALL) {
  const qc = new QueryClient();
  qc.setQueryData(["me"], { tenant: "t", user: { id: "u1", email: "mgr@acme.test", full_name: "Mia", is_admin: false, roles: ["Support Manager"] } });
  qc.setQueryData(["meta"], { entities: ENTITIES.map((name) => ({ name, permissions: perms })) });
  qc.setQueryData(["people"], { users: [{ id: "u1", full_name: "Mia Manager", email: "mgr@acme.test" }] });
  qc.setQueryData(["helpdesk", "held"], 2);
  seed(qc);
  return renderToStaticMarkup(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/admin/:section?" element={<AdminRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
const rows = (entity: string, sort: string, list: Record<string, unknown>[]) => (qc: QueryClient) => qc.setQueryData(["helpdesk", "admin", entity, sort, null], list.map((r, i) => ({ id: `r${i}`, version: 1, ...r })));

beforeEach(() => {
  (globalThis as { matchMedia?: unknown }).matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
});
afterEach(() => {
  delete (globalThis as { matchMedia?: unknown }).matchMedia;
});

describe("administration", () => {
  it("lists its sections and says how much mail waits for review", () => {
    const html = draw("/admin");
    for (const s of ["Support addresses", "Inbound mail", "Agent profiles", "Tags and tag rules", "Assignment rules", "Macros", "Satisfaction survey"]) expect(html).toContain(s);
    expect(html).toContain(">2<");
    expect(html).toContain('aria-current="page"');
  });

  it("says when there is no such section", () => {
    expect(draw("/admin/nothing")).toContain("There is no such section");
  });
});

describe("support addresses", () => {
  const seed = (qc: QueryClient) => {
    rows("SupportAddress", "address asc", [{ address: "support@acme.test", label: "Acme", team: "t1", default_priority: "High", enabled: true, _titles: { team: "Front line" } }, { address: "billing@acme.test", enabled: false }])(qc);
    qc.setQueryData(["helpdesk", "settings"], { id: "s1", version: 2, unknown_senders: "Hold for Review", inbound_per_hour: 20, inbound_log_days: 90 });
  };
  it("shows the address, its brand, its team and what to do with unknown senders", () => {
    const html = draw("/admin/addresses", seed);
    expect(html).toContain("support@acme.test");
    expect(html).toContain("Acme");
    expect(html).toContain("Front line");
    expect(html).toContain("Add support address");
    expect(html).toContain("Hold it in the inbound log for staff to open or discard");
    expect(html).toContain("Mail from a sender who is nobody&#x27;s customer");
    expect(html).toContain("Off");
  });

  it("tells a person with no right to the addresses, instead of an empty table", () => {
    expect(draw("/admin/addresses", seed, { read: "none", write: "none", create: "none", delete: "none" })).toContain("denied");
  });
});

describe("the inbound log", () => {
  const log = [
    { id: "l1", received_at: "2026-11-05T09:00:00Z", from_address: "stranger@x.test", to_address: "support@acme.test", subject: "Help me", outcome: "Held", reason: "Unknown sender" },
    { id: "l2", received_at: "2026-11-05T08:00:00Z", from_address: "ada@acme.test", to_address: "support@acme.test", subject: "Jam", outcome: "Ticket Created", ticket: "t1", _titles: { ticket: "TKT-2026-000001" } },
  ];
  const seed = (qc: QueryClient) => qc.setQueryData(["helpdesk", "inbound", "Held"], { pages: [{ rows: log, total: 2 }], pageParams: [0] });
  it("offers release and discard for held mail only, and links the ticket a mail became", () => {
    const html = draw("/admin/inbound", seed);
    expect(html).toContain("Help me");
    expect(html).toContain("Open a ticket from it");
    expect(html.match(/Open a ticket from it/g)).toHaveLength(1);
    expect(html).toContain("Discard");
    expect(html).toContain('href="/t/t1"');
    expect(html).toContain("TKT-2026-000001");
    expect(html).toContain("Unknown sender");
  });

  it("offers neither to someone who may only read", () => {
    const html = draw("/admin/inbound", seed, { read: "all", write: "none", create: "none", delete: "none" });
    expect(html).toContain("Help me");
    expect(html).not.toContain("Open a ticket from it");
  });

  it("says in words when nothing waits", () => {
    expect(draw("/admin/inbound", (qc) => qc.setQueryData(["helpdesk", "inbound", "Held"], { pages: [{ rows: [], total: 0 }], pageParams: [0] }))).toContain("No mail is waiting for review.");
  });
});

describe("agent profiles", () => {
  it("shows skills, shift calendar and whether there is a signature", () => {
    const html = draw("/admin/agents", rows("AgentProfile", "agent asc", [{ agent: "raj@acme.test", display_name: "Raj Rao", skills: "billing, hi", shift_calendar: "day-shift", signature: "Raj" }]));
    expect(html).toContain("Raj Rao");
    expect(html).toContain("billing");
    expect(html).toContain(">hi<");
    expect(html).toContain("day-shift");
  });
});

describe("tags and tag rules", () => {
  it("shows each tag with its colour and each rule with its condition", () => {
    const html = draw("/admin/tags", (qc) => {
      rows("TicketTag", "tag_name asc", [{ tag_name: "refund", colour: "#2563eb" }])(qc);
      rows("TagRule", "rule_name asc", [{ rule_name: "Urgent ones", tag: "x", condition: "doc.priority == 'Urgent'", enabled: true, _titles: { tag: "urgent" } }])(qc);
    });
    expect(html).toContain("refund");
    expect(html).toContain("--tag:#2563eb");
    expect(html).toContain("Urgent ones");
    expect(html).toContain("doc.priority == &#x27;Urgent&#x27;");
  });
});

describe("assignment rules", () => {
  const seed = (qc: QueryClient) => {
    rows("TicketAssignmentRule", "priority desc", [
      { rule_name: "Hindi billing", team: "t1", rule_type: "Load Balancing", priority: 20, needs_skill: "Fixed Skill", skill: "billing", fallback_agent: "lead@acme.test", enabled: true, _titles: { team: "Billing" } },
      { rule_name: "Everything else", team: "t2", rule_type: "Round Robin", priority: 10, needs_skill: "No", enabled: true, _titles: { team: "Front line" } },
    ])(qc);
    qc.setQueryData(["helpdesk", "settings"], { id: "s1", version: 1, route_reopened: true, reassign_unanswered_hours: 0, max_reassignments: 2 });
  };
  it("shows them in the order they are tried, with skill and fallback, and moves them earlier or later", () => {
    const html = draw("/admin/routing", seed);
    expect(html.indexOf("Hindi billing")).toBeLessThan(html.indexOf("Everything else"));
    expect(html).toContain("billing");
    expect(html).toContain("lead@acme.test");
    expect(html).toContain("Try Everything else earlier");
    expect(html).toContain("Try Hindi billing later");
    expect(html).toContain("The first enabled rule whose condition fits");
  });
});

describe("macros", () => {
  it("lists macros with who can use them and how often they were used", () => {
    const html = draw("/admin/macros", rows("SupportMacro", "macro_name asc", [{ macro_name: "Refund approved", scope: "Team", uses: 7, _titles: { team: "Billing" } }, { macro_name: "Mine", scope: "Personal" }]));
    expect(html).toContain("Refund approved");
    expect(html).toContain("Billing");
    expect(html).toContain("Only its author");
    expect(html).toContain("Add macro");
    expect(html).toContain("every step applies, or none does");
  });
});

describe("the satisfaction survey", () => {
  const seed = (qc: QueryClient) => qc.setQueryData(["helpdesk", "settings"], { id: "s1", version: 1, survey_enabled: true, survey_delay_hours: 2, survey_link_days: 30, survey_reopen_at_or_below: 2, survey_alert_at_or_below: 1, survey_alert_to: "lead@acme.test" });
  it("says what the settings add up to and previews the rating page", () => {
    const html = draw("/admin/survey", seed);
    expect(html).toContain("mailed a link 2 hours after the ticket is solved");
    expect(html).toContain("A rating of 2 or below reopens a ticket");
    expect(html).toContain("tells the agent and lead@acme.test");
    expect(html).toContain("The rating page the link opens");
    expect(html).toContain("How did we do? (1 to 5)");
    expect(html).toContain("nothing here is sent");
  });

  it("previews the message the workspace has, with an example ticket", () => {
    const html = draw("/admin/survey", (qc) => {
      seed(qc);
      qc.setQueryData(["helpdesk", "survey-template"], { template: { languages: { en: { email: { subject: "How did we do? {{ doc.number }}", body: "<p>Hello {{ doc.contact_name }}</p>" } } } } });
    });
    expect(html).toContain("How did we do? TKT-2026-000123");
    expect(html).toContain("sandbox");
    expect(html).toContain("Hello Ada");
  });
});
