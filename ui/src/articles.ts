// Help centre articles: who can read an article given its status and visibility (said in words before it is saved),
// the words the portal asks the server to suggest articles for, and the helpful share. Pure, so it is tested.

import { t } from "@ferrum/client";

export const STATUSES = ["Draft", "Published", "Archived"] as const;
export const VISIBILITIES = ["Internal", "Customers Only", "Public"] as const;

export const STATUS_TONES = { Draft: "neutral", Published: "green", Archived: "amber" } as const;

/** Who will read an article, by its status and visibility: the sentence shown beside the controls. */
export function reach(status: string, visibility: string): string {
  if (status === "Draft") return t("A draft: only agents see it, whoever it is meant for.");
  if (status === "Archived") return t("Archived: only agents see it. Customers no longer find it.");
  switch (visibility) {
    case "Internal":
      return t("Published for agents only: it is in the knowledge base, never shown to a customer.");
    case "Customers Only":
      return t("Published for customers: people signed in to the portal find it by search, are offered it as they start a request, and can say whether it helped.");
    case "Public":
      return t("Published for everyone: it is on the help centre's public list, and customers in the portal find it too. The platform's public page takes it from there.");
    default:
      return "";
  }
}

/** The words to look articles up for while a customer writes: the subject once it has a word the server would use (three letters or more), else nothing. */
export function suggestQuery(subject: string): string {
  const s = subject.trim().replace(/\s+/g, " ");
  return s.split(" ").some((w) => [...w.replace(/[^\p{L}\p{N}]/gu, "")].length >= 3) ? s.slice(0, 200) : "";
}

/** "7 of 10 found it helpful", or null while nobody has said. */
export function helpfulShare(helpful?: number | null, notHelpful?: number | null): string | null {
  const [h, n] = [helpful ?? 0, notHelpful ?? 0];
  return h + n === 0 ? null : t("{h} of {all} found it helpful", { h, all: h + n });
}
