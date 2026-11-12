import { describe, expect, it } from "vitest";
import { cleanTag, colourOf, colours, tagProblem } from "./tagwords";

describe("ticket tags", () => {
  it("are lowercase words as the server keeps them", () => {
    expect(cleanTag(" Billing-Q4 ")).toBe("billing-q4");
    expect(cleanTag("two words")).toBeNull();
    expect(cleanTag("")).toBeNull();
    expect(cleanTag("x".repeat(41))).toBeNull();
  });

  it("say what is wrong before a tag is sent", () => {
    expect(tagProblem("", [])).toBeNull();
    expect(tagProblem("two words", [])).toContain("one word");
    expect(tagProblem("Billing", ["billing"])).toBe("It is on the ticket already.");
    expect(tagProblem("vip", ["billing"])).toBeNull();
  });

  it("show only a colour the server would have accepted", () => {
    expect(colourOf("#2563eb")).toBe("#2563eb");
    expect(colourOf("red")).toBeNull();
    expect(colourOf("url(x)")).toBeNull();
    expect([...colours([{ tag_name: "VIP", colour: "#2563EB" }, { tag_name: "x", colour: "nope" }]).entries()]).toEqual([["vip", "#2563EB"]]);
  });
});
