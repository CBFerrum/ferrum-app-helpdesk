// Saved replies: finding one to insert, and filling its placeholders from the ticket and the agent
// answering it. A placeholder that cannot be filled stays in the text, marked, and holds back
// sending, so "Hello {{customer.first_name}}" never reaches a customer. Pure, so it is tested.

export type ReplyScope = "Personal" | "Team" | "Everyone";

export interface SavedReply {
  id: string;
  title: string;
  body: string;
  scope: ReplyScope;
  team?: string | null;
  uses?: number | null;
  created_by?: string | null;
}

/** What a reply may name, for the list that tells people. */
export const VARIABLES: { name: string; says: string }[] = [
  { name: "customer.first_name", says: "the first name of whoever wrote in" },
  { name: "customer.name", says: "their full name" },
  { name: "customer.company", says: "their company" },
  { name: "ticket.number", says: "the ticket's number" },
  { name: "ticket.subject", says: "its subject" },
  { name: "agent.first_name", says: "your first name" },
  { name: "agent.name", says: "your name" },
];

export interface FillWith {
  ticket: { number?: string | null; subject?: string | null };
  /** The person who wrote in, and the company they are with. */
  contact: { name?: string | null; company?: string | null; email?: string | null };
  /** The agent answering. */
  agent: { name?: string | null; email?: string | null };
}

const PLACEHOLDER = /\{\{\s*([a-z_]+(?:\.[a-z_]+)*)\s*\}\}/gi;

/** A person's first name: the first word of their name, or the start of their address. */
function firstName(name?: string | null, email?: string | null): string | null {
  const n = (name ?? "").trim();
  if (n && !n.includes("@")) return n.split(/\s+/)[0];
  const local = (email ?? "").split("@")[0];
  const word = local.split(/[._-]+/)[0];
  return word ? word.charAt(0).toUpperCase() + word.slice(1) : null;
}

/** The values a reply can name, by placeholder; an empty one is left out (it cannot be filled). */
export function values(w: FillWith): Record<string, string> {
  const out: Record<string, string | null | undefined> = {
    "ticket.number": w.ticket.number,
    "ticket.subject": w.ticket.subject,
    "customer.name": w.contact.name || w.contact.company,
    "customer.first_name": w.contact.name ? firstName(w.contact.name) : null,
    "customer.company": w.contact.company,
    "agent.name": w.agent.name || w.agent.email,
    "agent.first_name": firstName(w.agent.name, w.agent.email),
  };
  return Object.fromEntries(Object.entries(out).filter((e): e is [string, string] => !!e[1]?.trim()));
}

/** The reply with its placeholders filled; `missing` names those that could not be (they stay as
 *  written, so the agent sees where). */
export function fill(body: string, w: FillWith): { text: string; missing: string[] } {
  const known = values(w);
  const missing: string[] = [];
  const text = body.replace(PLACEHOLDER, (whole, name: string) => {
    const v = known[name.toLowerCase()];
    if (v !== undefined) return v;
    if (!missing.includes(name)) missing.push(name);
    return whole;
  });
  return { text, missing };
}

/** Placeholders still in a text: unfilled ones, or any the agent typed by hand. Sending waits for them. */
export function leftover(text: string): string[] {
  return [...new Set([...text.matchAll(PLACEHOLDER)].map((m) => m[1]))];
}

export type ScopeFilter = "all" | "mine" | "team" | "everyone";

export const SCOPE_FILTERS: { id: ScopeFilter; label: string }[] = [
  { id: "all", label: "All" },
  { id: "mine", label: "Personal" },
  { id: "team", label: "My teams" },
  { id: "everyone", label: "Everyone" },
];

const norm = (s: string) => s.toLowerCase().normalize("NFKD").replace(/\p{M}/gu, "");

/** The replies to offer: narrowed by who may use them and by the words typed, the most used first
 *  when nothing is typed, otherwise those with the words in their title before those with them in
 *  the text. `me` is the agent's user id. */
export function pick(all: SavedReply[], o: { query: string; scope: ScopeFilter; me?: string | null }): SavedReply[] {
  const q = norm(o.query.trim());
  const inScope = (r: SavedReply) => {
    switch (o.scope) {
      case "mine":
        return r.scope === "Personal" && (!r.created_by || r.created_by === o.me);
      case "team":
        return r.scope === "Team";
      case "everyone":
        return r.scope === "Everyone";
      default:
        return true;
    }
  };
  const rows = all.filter(inScope);
  const used = (r: SavedReply) => r.uses ?? 0;
  if (!q) return rows.sort((a, b) => used(b) - used(a) || a.title.localeCompare(b.title));
  const score = (r: SavedReply) => (norm(r.title).startsWith(q) ? 0 : norm(r.title).includes(q) ? 1 : norm(r.body).includes(q) ? 2 : 3);
  return rows
    .filter((r) => score(r) < 3)
    .sort((a, b) => score(a) - score(b) || used(b) - used(a) || a.title.localeCompare(b.title));
}

/** `insertion` put where the caret is (replacing what is selected); where the caret ends up. */
export function insertAt(text: string, from: number, to: number, insertion: string): { text: string; caret: number } {
  const clamp = (n: number) => Math.max(0, Math.min(text.length, n));
  const [a, b] = [clamp(Math.min(from, to)), clamp(Math.max(from, to))];
  return { text: text.slice(0, a) + insertion + text.slice(b), caret: a + insertion.length };
}
