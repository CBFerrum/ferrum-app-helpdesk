import { describe, expect, it } from "vitest";
import { bodyOf, draftOf, type FieldSpec, problems } from "./form";

const spec: FieldSpec[] = [
  { name: "address", label: "Address", kind: "email", required: true },
  { name: "label", label: "Brand", kind: "text", max: 10 },
  { name: "limit", label: "Limit", kind: "int", min: 0, max: 100 },
  { name: "enabled", label: "On", kind: "check" },
  { name: "colour", label: "Colour", kind: "colour" },
  { name: "skill", label: "Skill", kind: "text", when: (d) => d.kind === "fixed" },
  { name: "kind", label: "Kind", kind: "select", options: ["any", "fixed"] },
];

describe("an administration form", () => {
  it("starts from the record, or from the defaults for a new one", () => {
    expect(draftOf(spec, { address: "a@b.co", limit: 20, enabled: true, label: null })).toMatchObject({ address: "a@b.co", limit: "20", enabled: true, label: "" });
    expect(draftOf(spec, null, { enabled: true, kind: "any" })).toMatchObject({ enabled: true, kind: "any", address: "" });
  });

  it("says what is wrong, field by field, in words", () => {
    const bad = problems(spec, { ...draftOf(spec), address: "nonsense", label: "x".repeat(11), limit: "101", colour: "red" });
    expect(bad).toEqual({
      address: "That does not look like an email address.",
      label: "At most 10 characters.",
      limit: "At most 100.",
      colour: "Use #rrggbb, for example #2563eb.",
    });
    expect(problems(spec, draftOf(spec))).toEqual({ address: "This is needed." });
    expect(problems(spec, { ...draftOf(spec), address: "A@B.co", limit: "1.5" })).toEqual({ limit: "A whole number." });
  });

  it("does not hold a hidden field against the draft", () => {
    expect(problems([{ name: "skill", label: "S", kind: "text", required: true, when: (d) => d.kind === "fixed" }], { kind: "any", skill: "" })).toEqual({});
  });

  it("sends a new record's filled fields, in the shape the server keeps them", () => {
    expect(bodyOf(spec, { ...draftOf(spec), address: " Support@Acme.test ", limit: "20", enabled: true, kind: "any" })).toEqual({ address: "support@acme.test", limit: 20, enabled: true, kind: "any" });
  });

  it("sends an edited record's changes only, a cleared text as null, and clears what the draft no longer shows", () => {
    const record = { address: "a@b.co", label: "Acme", limit: 5, enabled: true, kind: "fixed", skill: "billing" };
    expect(bodyOf(spec, { ...draftOf(spec, record), label: "" }, record)).toEqual({ label: null });
    expect(bodyOf(spec, { ...draftOf(spec, record), kind: "any" }, record)).toEqual({ kind: "any", skill: null });
    expect(bodyOf(spec, draftOf(spec, record), record)).toEqual({});
  });
});
