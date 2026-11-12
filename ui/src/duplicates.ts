// The rules of merging tickets that can be told before asking the server (which decides again): only
// one customer's tickets, none closed, none into itself. Pure, so they are tested.

import { t } from "@ferrum/client";

export interface MergeTicket {
  id: string;
  number?: string | null;
  subject?: string | null;
  status?: string | null;
  customer_name?: string | null;
  contact_email?: string | null;
  opened_at?: string | null;
}

/** The same customer: by name where both have one, else by the contact's address. */
export function sameCustomer(a: MergeTicket, b: MergeTicket): boolean {
  if (a.customer_name && b.customer_name) return a.customer_name === b.customer_name;
  if (a.customer_name || b.customer_name) return false;
  return !!a.contact_email && !!b.contact_email && a.contact_email.toLowerCase() === b.contact_email.toLowerCase();
}

/** The one that stays by default: the oldest, which holds the longest story. */
export function oldest(tickets: MergeTicket[]): MergeTicket | undefined {
  const at = (x: MergeTicket) => (x.opened_at ? Date.parse(x.opened_at) : Number.POSITIVE_INFINITY);
  return [...tickets].sort((a, b) => at(a) - at(b))[0];
}

/** Why `other` cannot be merged into `survivor`, in the server's plain words; null when it can. */
export function refusal(survivor: MergeTicket, other: MergeTicket): string | null {
  const n = other.number ?? t("A ticket");
  if (other.id === survivor.id) return t("{n} is the ticket that stays: a ticket is not merged into itself", { n });
  if (other.status === "Closed") return t("{n} is closed, and a closed ticket is final", { n });
  if (!sameCustomer(survivor, other)) return t("{n} is another customer's: only tickets of one customer are merged", { n });
  return null;
}

/** The numbers the action takes: the duplicates, separated by commas. */
export const numbersOf = (others: MergeTicket[]): string => others.map((o) => o.number ?? "").filter(Boolean).join(", ");
