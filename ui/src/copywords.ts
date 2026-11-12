// The copies (CC) on a ticket: one text field of addresses separated by commas, which the server copies on
// every emailed reply (Helpdesk `inbound::addresses` reads it). Pure, so it is tested.

import { t as tr } from "@ferrum/client";

/** The addresses in the field: `Name <a@b.c>` counts as `a@b.c`; the same address once; lowercase. */
export function copyList(text?: string | null): string[] {
  const out: string[] = [];
  for (const part of (text ?? "").split(",")) {
    const m = /<([^<>]+)>\s*$/.exec(part);
    const a = (m ? m[1] : part).trim().toLowerCase();
    if (a.includes("@") && !/\s/.test(a) && !out.includes(a)) out.push(a);
  }
  return out;
}

const ADDRESS = /^[^\s@,<>]+@[^\s@,<>]+\.[^\s@,<>]+$/;

/** What is wrong with an address typed to copy, in words; null when it may be added. */
export function copyProblem(raw: string, have: string[], contact?: string | null): string | null {
  const a = raw.trim().toLowerCase();
  if (!a) return null;
  if (!ADDRESS.test(a)) return tr("That does not look like an email address.");
  if (have.includes(a)) return tr("They are copied already.");
  if (contact && a === contact.toLowerCase()) return tr("They are the customer, who gets the reply anyway.");
  return null;
}

/** The field's text for a list. Nothing left is empty, not an empty string. */
export const copyText = (list: string[]): string | null => (list.length ? list.join(", ") : null);
