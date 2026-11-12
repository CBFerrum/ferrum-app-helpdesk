import { describe, expect, it } from "vitest";
import { moved, renumber } from "./order";

describe("putting assignment rules in order", () => {
  it("moves one rule and leaves the others", () => {
    expect(moved(["a", "b", "c"], 2, 1)).toEqual(["a", "c", "b"]);
    expect(moved(["a", "b", "c"], 0, 5)).toEqual(["a", "b", "c"]);
    expect(moved(["a", "b", "c"], -1, 1)).toEqual(["a", "b", "c"]);
  });

  it("gives the first rule the highest number, which the server tries first", () => {
    expect(renumber(["a", "b", "c"], {})).toEqual([
      { id: "a", priority: 30 },
      { id: "b", priority: 20 },
      { id: "c", priority: 10 },
    ]);
  });

  it("changes only the rules whose number is not right yet, even when two shared a number", () => {
    expect(renumber(["a", "b", "c"], { a: 30, b: 10, c: 10 })).toEqual([
      { id: "b", priority: 20 },
    ]);
  });

  it("stays within the server's 1 to 1000", () => {
    const ids = Array.from({ length: 200 }, (_, i) => `r${i}`);
    const all = renumber(ids, {});
    expect(Math.max(...all.map((x) => x.priority))).toBeLessThanOrEqual(1000);
    expect(Math.min(...all.map((x) => x.priority))).toBeGreaterThanOrEqual(1);
  });
});
