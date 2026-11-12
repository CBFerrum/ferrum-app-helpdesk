// Ticket tags: the platform's tags on a record (lowercase words) and the colour a `TicketTag` record
// gives each. Pure helpers, so what a chip shows and what may be added are tested without a browser.

import { t as tr } from "@ferrum/client";

export interface TagDef {
  id?: string;
  tag_name: string;
  colour?: string | null;
  description?: string | null;
}

const COLOUR = /^#[0-9a-f]{6}$/i;

/** A colour the server's rule allows (#rrggbb); anything else shows as no colour. */
export const colourOf = (c?: string | null): string | null => (c && COLOUR.test(c.trim()) ? c.trim() : null);

/** The same rule the server applies (Helpdesk `tags::clean`): lowercase letters, digits, dashes, underscores. */
export function cleanTag(raw: string): string | null {
  const t = raw.trim().toLowerCase();
  return t && [...t].length <= 40 && /^[\p{L}\p{N}_-]+$/u.test(t) ? t : null;
}

/** What is wrong with a tag typed, in words; null when it may be added. */
export function tagProblem(raw: string, have: string[]): string | null {
  if (!raw.trim()) return null;
  const t = cleanTag(raw);
  if (!t) return tr("A tag is one word: letters, digits, dashes and underscores, at most 40.");
  if (have.some((h) => h.toLowerCase() === t)) return tr("It is on the ticket already.");
  return null;
}

/** Colours by tag name. */
export function colours(defs: TagDef[]): Map<string, string> {
  return new Map(defs.flatMap((d) => (colourOf(d.colour) ? [[d.tag_name.toLowerCase(), colourOf(d.colour)!] as [string, string]] : [])));
}
