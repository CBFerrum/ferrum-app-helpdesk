// A ticket's workspace as the route draws it, with the ticket already in the query cache: what the
// header offers, that the conversation, the box and the details are all there, and what changes when
// the ticket is closed or the person may only read.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { TicketRoute } from "./Ticket";

const ME = "priya@acme.test";

const ticket = {
  id: "t1",
  version: 4,
  number: "TKT-2026-000123",
  subject: "Printer jam",
  description: "It jams on every page.",
  status: "Open",
  priority: "High",
  channel: "Email",
  customer: "c1",
  customer_name: "Acme Corp",
  contact_name: "Ada",
  contact_email: "ada@acme.test",
  assigned_to: "raj@acme.test",
  opened_at: "2026-11-01T09:00:00Z",
  service_level_agreement: "sla",
  response_by: "2026-11-01T10:00:00Z",
  response_state: "breached",
  resolution_state: "running",
  resolution_by: "2026-11-05T10:00:00Z",
  response_breached: true,
  sla_status: "Breached",
  status_history: [{ status: "Open", at: "2026-11-01T09:00:00Z" }],
  conversation: [
    { id: "m1", author_kind: "Agent", author: "raj@acme.test", body: "Try the tray.", sent_at: "2026-11-01T11:00:00Z", emailed: true },
    { id: "m2", author_kind: "Agent", author: "raj@acme.test", body: "Might be the roller.", is_internal: true, sent_at: "2026-11-01T11:05:00Z" },
  ],
};

function draw(opts: { doc?: Record<string, unknown>; write?: boolean; replies?: boolean } = {}) {
  const qc = new QueryClient();
  const perms = { read: "all", write: opts.write === false ? "none" : "all", create: "all" };
  qc.setQueryData(["me"], { tenant: "t", user: { id: "u1", email: ME, full_name: "Priya Nair", is_admin: false, roles: [] } });
  qc.setQueryData(["meta"], { entities: [{ name: "Ticket", permissions: perms }, ...(opts.replies === false ? [] : [{ name: "SavedReply", permissions: perms }])] });
  qc.setQueryData(["meta", "Ticket"], { name: "Ticket", permissions: perms, actions: [{ name: "make_warranty_claim" }], linked_from: [], fields: [] });
  qc.setQueryData(["people"], { users: [{ id: "u1", full_name: "Priya Nair", email: ME }, { id: "u2", full_name: "Raj Rao", email: "raj@acme.test" }] });
  qc.setQueryData(["peek", "Ticket", "t1"], { ...ticket, ...opts.doc });
  qc.setQueryData(["helpdesk", "audit", "t1"], { events: [] });
  return renderToStaticMarkup(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/t/t1"]}>
        <Routes>
          <Route path="/t/:id" element={<TicketRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  (globalThis as { matchMedia?: unknown }).matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
});
afterEach(() => {
  delete (globalThis as { matchMedia?: unknown }).matchMedia;
});

describe("a ticket's workspace", () => {
  it("shows the ticket, who has it, and the actions an agent reaches for", () => {
    const html = draw();
    expect(html).toContain("Printer jam");
    expect(html).toContain("TKT-2026-000123");
    expect(html).toContain("Raj Rao");
    expect(html).toContain(">Resolve<");
    expect(html).toContain("Details");
    expect(html).toContain("Inbox");
  });

  it("puts the conversation, the box to answer in and the details side by side", () => {
    const html = draw();
    expect(html).toContain("It jams on every page.");
    expect(html).toContain("Try the tray.");
    expect(html).toContain("Might be the roller.");
    expect(html).toContain("Emailed to the customer");
    expect(html).toContain("Write to the ticket");
    expect(html).toContain("Send reply by email");
    expect(html).toContain("Service levels");
    expect(html).toContain("Breached: overdue by");
    expect(html).toContain("Customer");
  });

  it("tells the agent a colleague has the ticket before they answer over them", () => {
    expect(draw()).toContain("Raj Rao has this ticket. Sending takes it over.");
  });

  it("says that sending takes a ticket nobody has", () => {
    expect(draw({ doc: { assigned_to: null } })).toContain("Nobody has this ticket yet. Sending takes it over.");
  });

  it("offers saved replies only where the workspace has them", () => {
    expect(draw()).toContain("Saved replies");
    expect(draw({ replies: false })).not.toContain("Saved replies");
  });

  it("offers to reopen a resolved ticket, not to resolve it again", () => {
    const html = draw({ doc: { status: "Resolved", resolution_details: "Replaced the roller.", status_history: [{ status: "Open", at: "2026-11-01T09:00:00Z" }, { status: "Resolved", at: "2026-11-01T15:00:00Z" }] } });
    expect(html).toContain("Reopen");
    expect(html).not.toContain(">Resolve<");
    expect(html).toContain("Replaced the roller.");
  });

  it("leaves a closed ticket to be read, with no box to write in", () => {
    const html = draw({ doc: { status: "Closed" } });
    expect(html).toContain("This ticket is closed for good");
    expect(html).not.toContain("Write to the ticket");
    expect(html).not.toContain(">Resolve<");
  });

  it("says a merged ticket was merged, links the one that stayed, and has no box to reply in", () => {
    const html = draw({ doc: { status: "Closed", merged_into: "t9", merged_at: "2026-11-10T09:00:00Z", _titles: { merged_into: "TKT-2026-000099" } } });
    expect(html).toContain("Merged into");
    expect(html).toContain('href="/t/t9"');
    expect(html).toContain("TKT-2026-000099");
    expect(html).toContain("cannot be replied to");
    expect(html).not.toContain("closed for good");
    expect(html).not.toContain("Write to the ticket");
  });

  it("marks the messages that came from a merged ticket with where they came from", () => {
    const html = draw({ doc: { conversation: [{ id: "m9", author_kind: "Customer", body: "Still jammed.", sent_at: "2026-11-02T09:00:00Z", merged_from: "TKT-2026-000100" }] } });
    expect(html).toContain("Still jammed.");
    expect(html).toContain("From TKT-2026-000100, merged");
  });

  it("marks a message whose sender the mail server did not verify, and no other", () => {
    const html = draw({ doc: { conversation: [{ id: "m8", author_kind: "Customer", body: "Pay me.", sent_at: "2026-11-02T09:00:00Z", sender_unverified: true }, { id: "m7", author_kind: "Customer", body: "Fine.", sent_at: "2026-11-02T10:00:00Z" }] } });
    expect(html.match(/Sender not verified/g)).toHaveLength(1);
  });

  it("tells public replies from internal notes in the conversation", () => {
    const html = draw();
    expect(html).toContain("Emailed to the customer");
    expect(html).toContain("Internal note");
    expect(html).toContain("Public reply");
  });

  it("shows the copies on a message and that a ticket follows up an earlier one", () => {
    const html = draw({
      doc: {
        related_ticket: "t8",
        _titles: { related_ticket: "TKT-2026-000090" },
        conversation: [{ id: "m1", author_kind: "Customer", body: "Hello again", sent_at: "2026-11-02T09:00:00Z", cc: "bob@acme.test" }],
      },
    });
    expect(html).toContain("follows up");
    expect(html).toContain('href="/t/t8"');
    expect(html).toContain("TKT-2026-000090");
    expect(html).toContain("Cc");
    expect(html).toContain("bob@acme.test");
  });

  it("leaves a ticket to be read by someone who may not change it", () => {
    const html = draw({ write: false });
    expect(html).toContain("You can read this ticket but not answer it.");
    expect(html).not.toContain("Write to the ticket");
  });
});
