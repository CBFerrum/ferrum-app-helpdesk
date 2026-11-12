// The administration screens' records as small forms: which fields, what is wrong with a draft before it is
// sent, and what body to send (a new record whole, an edited one only what changed). The server checks
// again and its refusal is shown as it words it. Pure, so it is tested without a browser.

import { t as tr } from "@ferrum/client";

export type Kind = "text" | "email" | "textarea" | "markdown" | "code" | "select" | "check" | "int" | "link" | "colour";

export interface FieldSpec {
  name: string;
  label: string;
  kind: Kind;
  /** For `select`: the values stored. */
  options?: readonly string[];
  /** For `select`: what each is called, where it differs from what is stored. */
  optionLabels?: Record<string, string>;
  /** For `link`: the entity linked to. */
  to?: string;
  hint?: string;
  required?: boolean;
  min?: number;
  max?: number;
  rows?: number;
  /** Shown only while this holds of the draft. */
  when?: (draft: Draft) => boolean;
}

export type Draft = Record<string, string | boolean>;
type Doc = Record<string, unknown>;

export function draftOf(spec: FieldSpec[], record?: Doc | null, defaults: Doc = {}): Draft {
  const out: Draft = {};
  for (const f of spec) {
    const v = record ? record[f.name] : defaults[f.name];
    out[f.name] = f.kind === "check" ? !!v : v === null || v === undefined ? "" : String(v);
  }
  return out;
}

const visible = (spec: FieldSpec[], draft: Draft) => spec.filter((f) => !f.when || f.when(draft));

const value = (f: FieldSpec, v: string | boolean | undefined): unknown => {
  if (f.kind === "check") return !!v;
  const s = typeof v === "string" ? v : "";
  if (f.kind === "int") return s.trim() === "" ? null : Number(s);
  const t = f.kind === "code" || f.kind === "textarea" || f.kind === "markdown" ? s.trim() : s.trim();
  if (t === "") return null;
  return f.kind === "email" ? t.toLowerCase() : f.kind === "colour" ? t.toLowerCase() : t;
};

/** What is wrong with the draft, by field name, in words; empty when it may be sent. */
export function problems(spec: FieldSpec[], draft: Draft): Record<string, string> {
  const out: Record<string, string> = {};
  for (const f of visible(spec, draft)) {
    const v = value(f, draft[f.name]);
    if (f.required && (v === null || v === "")) out[f.name] = tr("This is needed.");
    else if (v === null || f.kind === "check") continue;
    else if (f.kind === "int") {
      const n = v as number;
      if (!Number.isInteger(n)) out[f.name] = tr("A whole number.");
      else if (f.min !== undefined && n < f.min) out[f.name] = tr("At least {min}.", { min: f.min });
      else if (f.max !== undefined && n > f.max) out[f.name] = tr("At most {max}.", { max: f.max });
    } else if (f.kind === "email" && !/^[^\s@,<>]+@[^\s@,<>]+\.[^\s@,<>]+$/.test(v as string)) out[f.name] = tr("That does not look like an email address.");
    else if (f.kind === "colour" && !/^#[0-9a-f]{6}$/.test(v as string)) out[f.name] = tr("Use #rrggbb, for example #2563eb.");
    else if (f.max !== undefined && typeof v === "string" && [...v].length > f.max) out[f.name] = tr("At most {max} characters.", { max: f.max });
  }
  return out;
}

/** The body to send: a new record's filled fields, or an edited record's changed ones. */
export function bodyOf(spec: FieldSpec[], draft: Draft, record?: Doc | null): Doc {
  const body: Doc = {};
  for (const f of visible(spec, draft)) {
    const now = value(f, draft[f.name]);
    if (record) {
      const was = value(f, draftOf([f], record)[f.name]);
      if (now !== was) body[f.name] = now;
    } else if (now !== null && (f.kind === "check" || now !== "")) body[f.name] = now;
  }
  // A field hidden by the draft is cleared on a record that had it, so a rule's old skill does not outlive its kind.
  if (record) for (const f of spec) if (f.when && !f.when(draft) && record[f.name] !== null && record[f.name] !== undefined && record[f.name] !== "" && f.kind !== "check") body[f.name] = null;
  return body;
}
