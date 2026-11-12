// Macros (`SupportMacro`): steps run in order on a ticket in one save, all or none. The preview says what
// each step will change on the ticket as it stands now, so nobody applies one blind; a step that sends
// a reply to the customer is said so first. Pure, so it is tested without a browser.

import { t as tr } from "@ferrum/client";

/** The step kinds the server knows (Helpdesk `MacroStep`), in the order a macro editor offers them. */
export const STEP_KINDS = [
  "Set Status",
  "Set Priority",
  "Set Team",
  "Assign To",
  "Set Type",
  "Set Language",
  "Set Resolution",
  "Add Tag",
  "Remove Tag",
  "Public Reply",
  "Private Note",
] as const;
export type StepKind = (typeof STEP_KINDS)[number];

export interface MacroStep {
  step: string;
  value?: string | null;
}

export interface Macro {
  id: string;
  macro_name: string;
  scope: "Personal" | "Team" | "Everyone";
  team?: string | null;
  uses?: number | null;
  steps?: MacroStep[];
}

/** What a ticket is now, for the steps to be told against. Names, not ids: steps name a team or type by its name. */
export interface Current {
  status?: string | null;
  priority?: string | null;
  assigned_to?: string | null;
  team?: string | null;
  ticket_type?: string | null;
  language?: string | null;
  resolution?: string | null;
  tags: string[];
  /** The name an address goes by. */
  name: (email: string) => string;
}

export interface Effect {
  /** `reply` reaches the customer; `note` stays with the team; `same` changes nothing. */
  kind: "change" | "same" | "reply" | "note";
  text: string;
}

const short = (s: string, n = 120) => (s.length > n ? `${s.slice(0, n).trimEnd()}…` : s);

/** Whether applying the macro sends the customer something. */
export const sendsReply = (steps: MacroStep[] = []) => steps.some((s) => s.step === "Public Reply");

/** What each step will do to the ticket as it is now, in the order they run. A later step is read against what the earlier ones leave. */
export function effects(steps: MacroStep[], now: Current): Effect[] {
  const cur = { ...now, tags: [...now.tags] };
  const field = (label: string, was: string | null | undefined, to: string): Effect => {
    const before = (was ?? "").trim();
    if (before === to.trim()) return { kind: "same", text: tr("{label} is already {to}: no change", { label, to }) };
    const out: Effect = { kind: "change", text: before ? tr("{label}: {was} becomes {to}", { label, was: before, to }) : tr("{label}: set to {to}", { label, to }) };
    return out;
  };
  return steps.map((s): Effect => {
    const v = (s.value ?? "").trim();
    switch (s.step) {
      case "Set Status": {
        const e = field(tr("Status"), cur.status, v);
        cur.status = v;
        return e;
      }
      case "Set Priority": {
        const e = field(tr("Priority"), cur.priority, v);
        cur.priority = v;
        return e;
      }
      case "Set Team": {
        const e = field(tr("Team"), cur.team, v);
        cur.team = v;
        return e;
      }
      case "Set Type": {
        const e = field(tr("Type"), cur.ticket_type, v);
        cur.ticket_type = v;
        return e;
      }
      case "Set Language": {
        const e = v ? field(tr("Language"), cur.language, v) : { kind: "change" as const, text: tr("Language: cleared") };
        cur.language = v || null;
        return e;
      }
      case "Assign To": {
        if (!v) {
          const e: Effect = cur.assigned_to ? { kind: "change", text: tr("Assigned to: {was} becomes nobody", { was: cur.name(cur.assigned_to) }) } : { kind: "same", text: tr("Nobody has the ticket: no change") };
          cur.assigned_to = null;
          return e;
        }
        const e = field(tr("Assigned to"), cur.assigned_to ? cur.name(cur.assigned_to) : null, cur.name(v));
        cur.assigned_to = v;
        return e;
      }
      case "Set Resolution":
        return { kind: "change", text: tr("How it was resolved: {text}", { text: short(v) }) };
      case "Add Tag": {
        const t = v.toLowerCase();
        if (cur.tags.includes(t)) return { kind: "same", text: tr("The tag {tag} is already on it: no change", { tag: t }) };
        cur.tags.push(t);
        return { kind: "change", text: tr("Adds the tag {tag}", { tag: t }) };
      }
      case "Remove Tag": {
        const t = v.toLowerCase();
        if (!cur.tags.includes(t)) return { kind: "same", text: tr("The tag {tag} is not on it: no change", { tag: t }) };
        cur.tags = cur.tags.filter((x) => x !== t);
        return { kind: "change", text: tr("Removes the tag {tag}", { tag: t }) };
      }
      case "Public Reply":
        return { kind: "reply", text: tr("Sends the customer this reply: {text}", { text: short(v) }) };
      case "Private Note":
        return { kind: "note", text: tr("Adds this internal note, only for the team: {text}", { text: short(v) }) };
      default:
        return { kind: "same", text: tr("{step} is not a step this version knows", { step: s.step }) };
    }
  });
}

export type MacroScope = "all" | "mine" | "team" | "everyone";

const norm = (s: string) => s.toLowerCase().normalize("NFKD").replace(/\p{M}/gu, "");

/** The macros to offer: narrowed by who may use them and the words typed, the most used first. `me` is the user id,
 *  which a record's `created_by` holds. */
export function findMacros(all: (Macro & { created_by?: string | null })[], o: { query: string; scope: MacroScope; me?: string | null }): Macro[] {
  const q = norm(o.query.trim());
  const inScope = (m: Macro & { created_by?: string | null }) => {
    switch (o.scope) {
      case "mine":
        return m.scope === "Personal" && (!m.created_by || m.created_by === o.me);
      case "team":
        return m.scope === "Team";
      case "everyone":
        return m.scope === "Everyone";
      default:
        return true;
    }
  };
  return all
    .filter(inScope)
    .filter((m) => !q || norm(m.macro_name).includes(q))
    .sort((a, b) => (b.uses ?? 0) - (a.uses ?? 0) || a.macro_name.localeCompare(b.macro_name));
}

/** What is wrong with a macro's steps before it is saved, in words; null when it is fine. The server checks again. */
export function stepsProblem(steps: MacroStep[]): string | null {
  if (!steps.length) return tr("A macro needs at least one step.");
  for (const [i, s] of steps.entries()) {
    const v = (s.value ?? "").trim();
    if (s.step === "Assign To" || s.step === "Set Language") continue;
    if (!v) return tr("Step {n}: {step} needs a value.", { n: i + 1, step: s.step });
  }
  return null;
}

/** One step as a line of words, for where it is not read against a ticket (a bulk confirmation): "Set Status: Resolved". */
export function stepLine(s: MacroStep): string {
  const v = (s.value ?? "").trim();
  return v ? `${s.step}: ${v.length > 80 ? `${v.slice(0, 80).trimEnd()}…` : v}` : s.step;
}
