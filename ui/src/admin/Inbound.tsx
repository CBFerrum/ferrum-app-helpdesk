// The inbound mail log: what became of each mail to a support address (a ticket, a message, or why it was neither)
// and the mail held for review, which staff open a ticket from or discard. A held mail waits here until one of the two.

import { useInfiniteQuery, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";
import { Link } from "react-router";
import { api, docPath, errorMessage, formatMoment, isolate, newKey, t } from "@ferrum/client";
import { Badge, Button, confirmDialog, ListSkeleton, StatePanel, type Tone, toast } from "@ferrum/kit";
import { LoadFailed } from "../states";
import { useRights } from "./Parts";

export interface LogRow {
  id: string;
  received_at?: string | null;
  from_address?: string | null;
  to_address?: string | null;
  subject?: string | null;
  outcome?: string | null;
  reason?: string | null;
  detail?: string | null;
  /** The held mail's text: absent for a person outside the field group `held_mail`. */
  body?: string | null;
  ticket?: string | null;
  _titles?: Record<string, string>;
}

export const OUTCOME_TONES: Readonly<Record<string, Tone>> = { "Ticket Created": "green", "Added to Ticket": "green", Held: "amber", Ignored: "neutral", Failed: "red" };

/** What the log can be narrowed to. */
export const VIEWS = [
  { id: "Held", get label() { return t("Held for review"); } },
  { id: "", get label() { return t("Everything"); } },
  { id: "Ticket Created,Added to Ticket", get label() { return t("Became tickets or messages"); } },
  { id: "Ignored,Failed", get label() { return t("Ignored or failed"); } },
] as const;

const PAGE = 50;

export function viewFilters(view: string): unknown[] | undefined {
  return view ? [["outcome", "in", view.split(",")]] : undefined;
}

/** What a held mail said, for the roles allowed to read it, as plain text (never drawn as markup). */
function HeldBody({ body }: { body?: string | null }) {
  if (body === undefined || body === null) return <span className="faint">{t("What the mail said is shown only to people given access to held mail.")}</span>;
  return (
    <details className="hd-held-body">
      <summary>{t("Read the mail")}</summary>
      {body.trim() ? <pre>{isolate(body)}</pre> : <p className="faint">{t("The mail has no text.")}</p>}
    </details>
  );
}

export function Inbound() {
  const qc = useQueryClient();
  const rights = useRights("InboundLog");
  const [view, setView] = useState<string>("Held");
  const [busy, setBusy] = useState<string | null>(null);
  const list = useInfiniteQuery({
    queryKey: ["helpdesk", "inbound", view],
    initialPageParam: 0,
    queryFn: ({ pageParam, signal }) =>
      api.get<{ rows: LogRow[]; total?: number }>(
        docPath("InboundLog"),
        { fields: "received_at,from_address,to_address,subject,outcome,reason,detail,body,ticket", sort: "received_at desc", limit: PAGE, offset: pageParam, count: true, titles: true, filters: viewFilters(view) ? JSON.stringify(viewFilters(view)) : undefined },
        signal,
      ),
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.rows.length, 0);
      return (last.total ?? 0) > loaded ? loaded : undefined;
    },
  });
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  const rows = list.data?.pages.flatMap((p) => p.rows) ?? [];
  const total = list.data?.pages[0]?.total ?? rows.length;
  const done = () => {
    void qc.invalidateQueries({ queryKey: ["helpdesk", "inbound"] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "held"] });
  };

  const release = async (r: LogRow) => {
    if (busy) return;
    setBusy(r.id);
    try {
      await api.post<{ ticket?: string }>(docPath("InboundLog", r.id, "actions", "release"), {}, { idempotencyKey: newKey() });
      toast(t("A ticket was opened from the mail. The log row now links to it."));
      done();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(null);
    }
  };
  const discard = async (r: LogRow) => {
    if (busy) return;
    if (!(await confirmDialog({ title: t("Discard this mail?"), message: t("The mail and its files are removed and no ticket is made. This cannot be undone."), confirmLabel: t("Discard it"), danger: true }))) return;
    setBusy(r.id);
    try {
      await api.post(docPath("InboundLog", r.id, "actions", "discard"), {}, { idempotencyKey: newKey() });
      toast(t("Discarded"));
      done();
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="stack">
      <p className="faint">{t("Mail is held here until staff open a ticket from it or discard it. It is never released by itself, and a mail that staff released is not held again.")}</p>
      <div className="segmented" role="group" aria-label={t("Show")}>
        {VIEWS.map((v) => (
          <button key={v.id} type="button" aria-pressed={view === v.id} onClick={() => setView(v.id)}>
            {isolate(v.label)}
          </button>
        ))}
      </div>
      {list.error ? (
        <LoadFailed error={list.error} onRetry={() => void list.refetch()} />
      ) : list.isLoading ? (
        <ListSkeleton rows={5} />
      ) : !rows.length ? (
        <StatePanel kind="empty" title={view === "Held" ? t("No mail is waiting for review.") : t("Nothing in the log for this view.")} />
      ) : (
        <>
          <p className="faint" aria-live="polite">
            {t("{n} mails", { n: total })}
          </p>
          <ul className="hd-rows" aria-label={t("Inbound mail")}>
            {rows.map((r) => (
              <li key={r.id} className="hd-log">
                <div className="hd-log-main">
                  <strong>{isolate(r.subject || t("No subject"))}</strong>
                  <span className="faint">
                    {isolate(r.from_address ?? "")} → {isolate(r.to_address ?? "")}
                    {r.received_at ? ` · ${formatMoment(r.received_at, { dateStyle: "medium", timeStyle: "short" })}` : ""}
                  </span>
                  {r.detail && <span className="faint">{isolate(r.detail)}</span>}
                  {r.reason === "Unverified sender" && <span className="faint">{t("The sender's mail server did not vouch for this mail, so it waits for staff.")}</span>}
                  {r.outcome === "Held" && <HeldBody body={r.body} />}
                </div>
                <div className="hd-log-state">
                  <Badge tone={OUTCOME_TONES[r.outcome ?? ""] ?? "neutral"}>{isolate(r.outcome ?? "")}</Badge>
                  {r.reason && <span className="faint">{isolate(r.reason)}</span>}
                  {r.ticket && <Link to={`/t/${r.ticket}`}>{isolate(r._titles?.ticket ?? t("The ticket"))}</Link>}
                </div>
                {r.outcome === "Held" && rights.write && (
                  <div className="hd-log-actions">
                    <Button size="sm" variant="primary" loading={busy === r.id} disabled={!!busy} onClick={() => void release(r)}>
                      {t("Open a ticket from it")}
                    </Button>
                    <Button size="sm" disabled={!!busy} onClick={() => void discard(r)}>
                      {t("Discard")}
                    </Button>
                  </div>
                )}
              </li>
            ))}
          </ul>
          {list.hasNextPage && (
            <div className="load-more">
              <Button loading={list.isFetchingNextPage} onClick={() => void list.fetchNextPage()}>
                {t("Show more ({n} left)", { n: total - rows.length })}
              </Button>
            </div>
          )}
        </>
      )}
    </div>
  );
}
