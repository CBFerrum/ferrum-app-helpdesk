import { describe, expect, it } from "vitest";
import { effects, findMacros, type Macro, sendsReply, stepsProblem } from "./macros";

const now = { status: "Open", priority: "Medium", assigned_to: "raj@acme.test", team: "Billing", ticket_type: "Question", language: null, tags: ["vip"], name: (e: string) => (e === "raj@acme.test" ? "Raj Rao" : e) };

describe("what a macro will change", () => {
  it("reads each step against the ticket as it stands", () => {
    const e = effects(
      [
        { step: "Set Status", value: "Resolved" },
        { step: "Set Priority", value: "Medium" },
        { step: "Assign To", value: "priya@acme.test" },
        { step: "Add Tag", value: "Refund" },
        { step: "Remove Tag", value: "vip" },
        { step: "Remove Tag", value: "never-had" },
      ],
      now,
    );
    expect(e.map((x) => [x.kind, x.text])).toEqual([
      ["change", "Status: Open becomes Resolved"],
      ["same", "Priority is already Medium: no change"],
      ["change", "Assigned to: Raj Rao becomes priya@acme.test"],
      ["change", "Adds the tag refund"],
      ["change", "Removes the tag vip"],
      ["same", "The tag never-had is not on it: no change"],
    ]);
  });

  it("lets a later step see what an earlier one left", () => {
    const e = effects([{ step: "Add Tag", value: "x" }, { step: "Add Tag", value: "X" }], now);
    expect(e.map((x) => x.kind)).toEqual(["change", "same"]);
  });

  it("says first when the customer is sent something, and that a note stays with the team", () => {
    const steps = [{ step: "Private Note", value: "Called them." }, { step: "Public Reply", value: "Sorted." }];
    expect(sendsReply(steps)).toBe(true);
    expect(sendsReply([{ step: "Private Note", value: "x" }])).toBe(false);
    expect(effects(steps, now).map((x) => x.kind)).toEqual(["note", "reply"]);
  });

  it("shortens a long text and names a step it does not know", () => {
    const [long, odd] = effects([{ step: "Public Reply", value: "w ".repeat(100) }, { step: "Escalate", value: "" }], now);
    expect(long.text.length).toBeLessThan(170);
    expect(odd.text).toContain("Escalate is not a step");
  });
});

describe("finding a macro", () => {
  const list: (Macro & { created_by?: string })[] = [
    { id: "1", macro_name: "Close as duplicate", scope: "Everyone", uses: 3 },
    { id: "2", macro_name: "Refund approved", scope: "Team", uses: 9 },
    { id: "3", macro_name: "Mine only", scope: "Personal", created_by: "u1" },
    { id: "4", macro_name: "Théo's", scope: "Personal", created_by: "u2" },
  ];
  it("puts the most used first and narrows by words and by whose", () => {
    expect(findMacros(list, { query: "", scope: "all" }).map((m) => m.id)).toEqual(["2", "1", "3", "4"]);
    expect(findMacros(list, { query: "theo", scope: "all" }).map((m) => m.id)).toEqual(["4"]);
    expect(findMacros(list, { query: "", scope: "mine", me: "u1" }).map((m) => m.id)).toEqual(["3"]);
    expect(findMacros(list, { query: "", scope: "team" }).map((m) => m.id)).toEqual(["2"]);
  });
});

describe("a macro's steps before saving", () => {
  it("needs a step, and a value where the step takes one", () => {
    expect(stepsProblem([])).toBe("A macro needs at least one step.");
    expect(stepsProblem([{ step: "Add Tag", value: " " }])).toBe("Step 1: Add Tag needs a value.");
    expect(stepsProblem([{ step: "Assign To", value: "" }, { step: "Add Tag", value: "x" }])).toBeNull();
  });
});
