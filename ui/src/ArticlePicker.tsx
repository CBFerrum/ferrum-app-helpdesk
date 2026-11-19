// Articles for the reply being written (Helpdesk 0.20): the published articles that fit the ticket, Internal ones
// included, from the ticket's own operation `suggest_articles`; search with other words, open one to read, or insert a
// link to one the customer may read. An Internal article cannot be linked: it would not open for the customer.

import { useQuery } from "@tanstack/react-query";
import { useRef, useState } from "react";
import { Link } from "react-router";
import { ApiError, api, bootstrap, docPath, errorMessage, isolate, t } from "@ferrum/client";
import { Button, Spinner, useDebounced, useDismiss } from "@ferrum/kit";
import { articleLink, type SuggestedArticle } from "./articles";

const WHO: Record<string, () => string> = {
  Internal: () => t("Agents only"),
  "Customers Only": () => t("Customers signed in"),
  Public: () => t("Everyone"),
};

export const suggestKey = (ticket: string, q: string) => ["helpdesk", "suggest", ticket, q];

export function ArticlePicker({ ticket, onPick, onClose }: { ticket: string; onPick: (text: string) => void; onClose: () => void }) {
  const [typed, setTyped] = useState("");
  const q = useDebounced(typed.trim(), 300);
  const box = useRef<HTMLDivElement>(null);
  useDismiss(box, true, onClose);
  const found = useQuery({
    queryKey: suggestKey(ticket, q),
    retry: false,
    queryFn: () => api.post<{ articles: SuggestedArticle[] }>(docPath("Ticket", ticket, "actions", "suggest_articles"), { q }).then((r) => r.articles),
  });
  const here = globalThis.location;
  const at = { protocol: here?.protocol ?? "https:", host: bootstrap()?.workspace.host ?? here?.host ?? "", slug: bootstrap()?.workspace.slug ?? "" };
  const refused = found.error instanceof ApiError && (found.error.status === 403 || found.error.status === 404);
  return (
    <div ref={box} className="hd-picker" role="dialog" aria-label={t("Articles")}>
      <div className="hd-picker-top">
        <input className="input" type="search" aria-label={t("Search articles")} placeholder={t("Fits this ticket")} autoFocus value={typed} onChange={(e) => setTyped(e.target.value)} />
      </div>
      <div className="hd-picker-body">
        {refused ? (
          <p className="muted hd-picker-empty">{t("You cannot search the articles.")}</p>
        ) : found.error ? (
          <p className="muted hd-picker-empty">
            {isolate(errorMessage(found.error))}{" "}
            <Button size="sm" onClick={() => void found.refetch()}>
              {t("Try again")}
            </Button>
          </p>
        ) : !found.data ? (
          <Spinner label={t("Looking for articles")} />
        ) : !found.data.length ? (
          <p className="muted hd-picker-empty">{t("No article fits. Try other words.")}</p>
        ) : (
          <ul className="hd-picker-list" aria-label={t("Articles")}>
            {found.data.map((a) => {
              const url = articleLink(a, at);
              return (
                <li key={a.id} className="hd-picker-opt">
                  <span className="hd-picker-title">{isolate(a.title)}</span>
                  {a.summary && <span className="faint">{isolate(a.summary)}</span>}
                  <span className="faint">{isolate((WHO[a.visibility] ?? (() => a.visibility))())}</span>
                  <span className="row">
                    <Link className="btn btn-ghost btn-sm" to={`/help/${a.id}`} target="_blank" rel="noreferrer">
                      {t("Open")}
                    </Link>
                    <Button size="sm" disabled={!url} onClick={() => url && onPick(`[${a.title}](${url})`)}>
                      {t("Insert link")}
                    </Button>
                  </span>
                  {!url && <span className="faint">{t("Agents only: it cannot be sent to a customer")}</span>}
                </li>
              );
            })}
          </ul>
        )}
      </div>
    </div>
  );
}
