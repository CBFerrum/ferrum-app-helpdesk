import { describe, expect, it } from "vitest";
import { numbersOf, oldest, refusal, sameCustomer } from "./duplicates";

const a = { id: "a", number: "TKT-1", status: "Open", customer_name: "Acme", contact_email: "ada@acme.test", opened_at: "2026-11-02T09:00:00Z" };
const b = { id: "b", number: "TKT-2", status: "Replied", customer_name: "Acme", contact_email: "bo@acme.test", opened_at: "2026-11-01T09:00:00Z" };

describe("merging tickets", () => {
  it("keeps the oldest by default", () => {
    expect(oldest([a, b])?.id).toBe("b");
    expect(oldest([])).toBeUndefined();
  });

  it("knows a customer by name, else by the contact's address", () => {
    expect(sameCustomer(a, b)).toBe(true);
    expect(sameCustomer(a, { ...b, customer_name: "Globex" })).toBe(false);
    expect(sameCustomer({ ...a, customer_name: null }, { ...b, customer_name: null, contact_email: "ADA@acme.test" })).toBe(true);
    expect(sameCustomer({ ...a, customer_name: null }, b)).toBe(false);
  });

  it("tells why a ticket cannot be merged, as the server would", () => {
    expect(refusal(a, b)).toBeNull();
    expect(refusal(a, { ...b, status: "Closed" })).toBe("TKT-2 is closed, and a closed ticket is final");
    expect(refusal(a, { ...b, customer_name: "Globex" })).toBe("TKT-2 is another customer's: only tickets of one customer are merged");
    expect(refusal(a, a)).toContain("not merged into itself");
  });

  it("names the duplicates the way the action takes them", () => {
    expect(numbersOf([a, b])).toBe("TKT-1, TKT-2");
  });
});
