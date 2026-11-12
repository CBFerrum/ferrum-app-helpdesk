import { describe, expect, it } from "vitest";
import { copyList, copyProblem, copyText } from "./copywords";

describe("the copies on a ticket", () => {
  it("reads the addresses as the server does", () => {
    expect(copyList('Bob <BOB@cust.test>, "Doe, Jane" <jane@cust.test>, bob@cust.test, nonsense, ')).toEqual(["bob@cust.test", "jane@cust.test"]);
    expect(copyList(null)).toEqual([]);
  });

  it("tells what is wrong with an address before it is added", () => {
    expect(copyProblem("", [])).toBeNull();
    expect(copyProblem("bob", [])).toBe("That does not look like an email address.");
    expect(copyProblem("Bob@cust.test", ["bob@cust.test"])).toBe("They are copied already.");
    expect(copyProblem("ada@cust.test", [], "Ada@cust.test")).toBe("They are the customer, who gets the reply anyway.");
    expect(copyProblem("new@cust.test", ["bob@cust.test"])).toBeNull();
  });

  it("writes the list back, and nothing as empty", () => {
    expect(copyText(["a@b.co", "c@d.co"])).toBe("a@b.co, c@d.co");
    expect(copyText([])).toBeNull();
  });
});
