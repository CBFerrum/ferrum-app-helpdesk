import { describe, expect, it } from "vitest";
import { fill, type FillWith, insertAt, leftover, pick, type SavedReply, values } from "./replies";

const w: FillWith = {
  ticket: { number: "TKT-2026-000123", subject: "Printer jam" },
  contact: { name: "Ada Lovelace", company: "Acme Corp", email: "ada@acme.test" },
  agent: { name: "Priya Nair", email: "priya@helpdesk.test" },
};

describe("filling a saved reply", () => {
  it("names the ticket, the customer and the agent", () => {
    const body = "Hi {{customer.first_name}}, about {{ticket.number}} ({{ticket.subject}}) at {{customer.company}}.\n\nThanks, {{agent.first_name}}";
    expect(fill(body, w)).toEqual({ text: "Hi Ada, about TKT-2026-000123 (Printer jam) at Acme Corp.\n\nThanks, Priya", missing: [] });
  });

  it("allows spaces inside the braces and any case", () => {
    expect(fill("{{ Customer.Name }} / {{agent.name}}", w).text).toBe("Ada Lovelace / Priya Nair");
  });

  it("leaves a placeholder it cannot fill as written, and says which", () => {
    const r = fill("Hi {{customer.first_name}}, {{customer.nickname}} {{customer.first_name}}", { ...w, contact: { email: "x@y.test" } });
    expect(r.text).toBe("Hi {{customer.first_name}}, {{customer.nickname}} {{customer.first_name}}");
    expect(r.missing).toEqual(["customer.first_name", "customer.nickname"]);
  });

  it("does not guess a customer's first name from a company", () => {
    expect(values({ ...w, contact: { company: "Acme Corp" } })["customer.first_name"]).toBeUndefined();
    expect(values({ ...w, contact: { company: "Acme Corp" } })["customer.name"]).toBe("Acme Corp");
  });

  it("takes the agent's first name from their address when they have no name", () => {
    expect(values({ ...w, agent: { email: "priya.nair@helpdesk.test" } })["agent.first_name"]).toBe("Priya");
  });

  it("keeps text that only looks like a placeholder", () => {
    expect(fill("Use {braces} or {{ }} freely, or {{not a name}}.", w)).toEqual({ text: "Use {braces} or {{ }} freely, or {{not a name}}.", missing: [] });
  });
});

describe("what holds back sending", () => {
  it("is any placeholder still in the text, once each", () => {
    expect(leftover("Hello {{customer.first_name}} {{customer.first_name}} and {{x}}")).toEqual(["customer.first_name", "x"]);
    expect(leftover("All filled in.")).toEqual([]);
  });
});

const r = (id: string, title: string, extra: Partial<SavedReply> = {}): SavedReply => ({ id, title, body: "", scope: "Everyone", ...extra });

describe("finding a saved reply", () => {
  const all = [
    r("1", "Refund policy", { body: "Refunds take 5 days.", uses: 2 }),
    r("2", "Thanks for waiting", { uses: 9 }),
    r("3", "Greeting", { scope: "Personal", created_by: "me", body: "Thanks for writing in." }),
    r("4", "Her greeting", { scope: "Personal", created_by: "her" }),
    r("5", "Billing hours", { scope: "Team", team: "t1", body: "Refunds are handled by billing.", uses: 4 }),
  ];

  it("lists the most used first when nothing is typed", () => {
    expect(pick(all, { query: "", scope: "all", me: "me" }).map((x) => x.id)).toEqual(["2", "5", "1", "3", "4"]);
  });

  it("narrows by who may use a reply, and Personal means the agent's own", () => {
    expect(pick(all, { query: "", scope: "mine", me: "me" }).map((x) => x.id)).toEqual(["3"]);
    expect(pick(all, { query: "", scope: "team", me: "me" }).map((x) => x.id)).toEqual(["5"]);
    expect(pick(all, { query: "", scope: "everyone", me: "me" }).map((x) => x.id)).toEqual(["2", "1"]);
  });

  it("finds by title before text, ignoring case and accents", () => {
    expect(pick(all, { query: "THANKS", scope: "all", me: "me" }).map((x) => x.id)).toEqual(["2", "3"]);
    expect(pick(all, { query: "refund", scope: "all", me: "me" }).map((x) => x.id)).toEqual(["1", "5"]);
    expect(pick([r("9", "Café hours")], { query: "cafe", scope: "all" }).map((x) => x.id)).toEqual(["9"]);
    expect(pick(all, { query: "zzz", scope: "all", me: "me" })).toEqual([]);
  });
});

describe("putting a reply in the text", () => {
  it("replaces what is selected and leaves the caret after it", () => {
    expect(insertAt("Hello world", 6, 11, "there")).toEqual({ text: "Hello there", caret: 11 });
    expect(insertAt("/", 0, 1, "Hi")).toEqual({ text: "Hi", caret: 2 });
    expect(insertAt("ab", 2, 2, "c")).toEqual({ text: "abc", caret: 3 });
    expect(insertAt("ab", 9, 9, "c")).toEqual({ text: "abc", caret: 3 });
  });
});
