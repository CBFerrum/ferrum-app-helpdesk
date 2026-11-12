import { describe, expect, it } from "vitest";
import { helpfulShare, reach, suggestQuery } from "./articles";

describe("who an article reaches", () => {
  it("is nobody but agents while it is a draft or archived, whatever its visibility", () => {
    expect(reach("Draft", "Public")).toContain("only agents see it");
    expect(reach("Archived", "Public")).toContain("Archived");
  });

  it("says where a published article shows up, by its visibility", () => {
    expect(reach("Published", "Internal")).toContain("agents only");
    expect(reach("Published", "Customers Only")).toContain("signed in to the portal");
    expect(reach("Published", "Public")).toContain("help centre's public list");
  });
});

describe("what to look articles up for", () => {
  it("waits for a word the server would use, and tidies the rest", () => {
    expect(suggestQuery("")).toBe("");
    expect(suggestQuery("a to I")).toBe("");
    expect(suggestQuery("  My   printer is jammed ")).toBe("My printer is jammed");
    expect(suggestQuery("x".repeat(300))).toHaveLength(200);
  });
});

describe("how helpful an article was", () => {
  it("says nothing until someone has voted", () => {
    expect(helpfulShare(0, 0)).toBeNull();
    expect(helpfulShare(null, undefined)).toBeNull();
    expect(helpfulShare(7, 3)).toBe("7 of 10 found it helpful");
  });
});
