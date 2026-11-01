// The inbox as a whole, drawn from what the server would have said: the screen is rendered to markup
// with its data already in the query cache, so what its wiring does with queues, counts, rows,
// saved filters and an empty or filtered list is read without a browser.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { HelpdeskHome } from "./Home";
import type { Ticket } from "./inbox";

const ME = "priya@acme.test";

function draw(opts: { url?: string; rows?: Ticket[]; permissions?: Record<string, string> } = {}) {
  const qc = new QueryClient();
  qc.setQueryData(["me"], { tenant: "t", user: { id: "u1", email: ME, full_name: "Priya Nair", is_admin: false, roles: ["Support Agent"] } });
  qc.setQueryData(["meta"], { entities: [{ name: "Ticket", permissions: opts.permissions ?? { read: "all", write: "all", create: "all" } }] });
  qc.setQueryData(["people"], { users: [{ id: "u1", full_name: "Priya Nair", email: ME }, { id: "u2", full_name: "Raj Rao", email: "raj@acme.test" }] });
  qc.setQueryData(["helpdesk", "teams"], [{ id: "team-1", team_name: "Front line" }]);
  qc.setQueryData(["helpdesk", "my-teams", ME], ["team-1"]);
  qc.setQueryData(["helpdesk", "prefs"], { value: { filters: [{ id: "s1", name: "Urgent email", query: { priority: "Urgent", channel: "Email" } }] }, version: 1 });
  qc.setQueryData(["helpdesk", "queue", "load"], [{ who: ME, count: 4 }, { who: "raj@acme.test", count: 2 }]);
  const mineFor = (queue: string) => ["helpdesk", "queue", "count", queue, ME, "team-1"];
  for (const [q, n] of [["mine", 3], ["unassigned", 1], ["alerts", 2], ["teams", 5], ["open", 9]] as const) qc.setQueryData(mineFor(q), n);
  const rows = opts.rows ?? [];
  const url = new URL(opts.url ?? "/", "http://x");
  const key = ["helpdesk", "queue", "list", url.searchParams.get("queue") ?? "mine", url.searchParams.get("sort") === "newest", url.searchParams.get("q") ?? "", url.searchParams.get("priority") ?? "", url.searchParams.get("team") ?? "", url.searchParams.get("channel") ?? "", url.searchParams.get("assignee") ?? "", ME, "team-1"];
  qc.setQueryData(key, { pages: [{ rows, total: rows.length }], pageParams: [0] });
  return renderToStaticMarkup(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[url.pathname + url.search]}>
        <HelpdeskHome />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

const ticket = (id: string, extra: Partial<Ticket> = {}): Ticket => ({ id, number: `TKT-${id}`, subject: `Subject ${id}`, status: "Open", priority: "Medium", opened_at: "2026-11-01T09:00:00Z", version: 1, ...extra });

beforeEach(() => {
  // A wide screen, so that the whole of the left side is drawn.
  (globalThis as { matchMedia?: unknown }).matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
});
afterEach(() => {
  delete (globalThis as { matchMedia?: unknown }).matchMedia;
});

describe("the inbox", () => {
  it("lists the queues with how many tickets each holds, and marks the one shown", () => {
    const html = draw({ rows: [ticket("1"), ticket("2")] });
    for (const label of ["Mine", "Unassigned", "SLA alerts", "My teams", "All open"]) expect(html).toContain(label);
    expect(html).toContain(">9<");
    expect(html).toContain('aria-current="page"');
  });

  it("shows each ticket, and says how many there are", () => {
    const html = draw({ rows: [ticket("1", { priority: "Urgent", sla_status: "Breached" }), ticket("2", { assigned_to: ME })] });
    expect(html).toContain("Subject 1");
    expect(html).toContain("Subject 2");
    expect(html).toContain("2 tickets");
    expect(html).toContain("Breached");
    expect(html).toContain("You have this ticket");
  });

  it("lists the saved filters and who has what", () => {
    const html = draw({ rows: [ticket("1")] });
    expect(html).toContain("Urgent email");
    expect(html).toContain("Who has what");
    expect(html).toContain("Raj Rao");
  });

  it("says a queue is empty in words, not with a blank page", () => {
    expect(draw()).toContain("Nothing assigned to you is open. Well done.");
    expect(draw({ url: "/?queue=alerts" })).toContain("No ticket is at risk of missing its SLA.");
  });

  it("says nothing fits when filters are on, and offers to clear them", () => {
    const html = draw({ url: "/?priority=High" });
    expect(html).toContain("No ticket matches");
    expect(html).toContain("Clear the filters");
  });

  it("offers a view to save when the filters are new, but not when they are a saved one", () => {
    expect(draw({ url: "/?priority=High" })).toContain("Save this view");
    expect(draw({ url: "/?priority=Urgent&channel=Email", rows: [ticket("1")] })).not.toContain("Save this view");
  });

  it("does not offer changes to someone who may only read", () => {
    const html = draw({ rows: [ticket("1")], permissions: { read: "all", write: "none", create: "none" } });
    expect(html).toContain("You can read tickets here but not change them.");
    expect(html).not.toContain("New ticket");
    expect(html).not.toContain(">Take<");
  });

  it("tells someone with no Support role, instead of showing an empty inbox", () => {
    expect(draw({ permissions: { read: "none", write: "none", create: "none" } })).toContain("No tickets for you");
  });
});
