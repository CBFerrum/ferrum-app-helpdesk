// Merging duplicate tickets (Helpdesk 0.9.0, `merge_tickets` on the ticket that stays). From the
// inbox, the selected tickets are merged into the one the agent chooses to keep; from a ticket, the
// customer's other open tickets are merged into it. The dialog says what moves and that the others
// close for good, and tells the server's refusals in its words.

import { type FormEvent, useEffect, useMemo, useState } from "react";
import { api, ApiError, docPath, errorMessage, isolate, t } from "@ferrum/client";
import { Alert, Button, Dialog } from "@ferrum/kit";
import { type MergeTicket, numbersOf, oldest, refusal } from "./duplicates";

export interface Merged {
  merged: string[];
  messages_moved: number;
  message?: string;
}

/** What the server refused, as it said it (a field's message, else the error's). */
export function refusedBy(e: unknown): string {
  return e instanceof ApiError && e.fields.length ? e.fields.map((f) => f.message).join(" ") : errorMessage(e);
}

/** Who stays and who merges when the dialog opens: from the inbox every other selected ticket merges, from a ticket the agent ticks them. */
function start(tickets: MergeTicket[], keep?: string): { survivor: string; moved: Set<string> } {
  const survivor = keep ?? oldest(tickets)?.id ?? "";
  return { survivor, moved: new Set(keep ? [] : tickets.filter((x) => x.id !== survivor).map((x) => x.id)) };
}

export function MergeDialog({
  open,
  tickets,
  keep,
  loading,
  failed,
  onClose,
  onDone,
}: {
  open: boolean;
  /** The tickets on offer; in a ticket's own dialog, that ticket and the customer's others. */
  tickets: MergeTicket[];
  /** The ticket that must stay, when it is fixed (the one the dialog was opened on). */
  keep?: string;
  /** The tickets on offer are still being fetched, or could not be. */
  loading?: boolean;
  failed?: boolean;
  onClose: () => void;
  onDone: (r: Merged, survivor: MergeTicket) => void;
}) {
  const [survivorId, setSurvivorId] = useState(() => start(tickets, keep).survivor);
  const [moved, setMoved] = useState<Set<string>>(() => start(tickets, keep).moved);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  // Starts over when it opens or the tickets on offer change, not on every draw of the page behind.
  const offered = tickets.map((x) => x.id).join(",");
  useEffect(() => {
    if (!open) return;
    const begun = start(tickets, keep);
    setSurvivorId(begun.survivor);
    setMoved(begun.moved);
    setBusy(false);
    setProblem(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, keep, offered]);

  const survivor = tickets.find((x) => x.id === survivorId);
  const others = useMemo(() => tickets.filter((x) => x.id !== survivorId && (keep ? moved.has(x.id) : true)), [tickets, survivorId, moved, keep]);
  const refused = survivor ? others.map((o) => refusal(survivor, o)).filter((r): r is string => !!r) : [];
  const ready = !!survivor && others.length > 0 && refused.length === 0 && survivor.status !== "Closed";
  const label = (x: MergeTicket) => [x.number, x.subject].filter(Boolean).join(" · ");

  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!survivor || !ready || busy) return;
    setBusy(true);
    setProblem(null);
    try {
      const r = await api.post<Merged>(docPath("Ticket", survivor.id, "actions", "merge_tickets"), { numbers: numbersOf(others) });
      onDone(r, survivor);
      onClose();
    } catch (err) {
      setProblem(refusedBy(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={keep ? t("Merge tickets into {n}", { n: survivor?.number ?? t("this ticket") }) : t("Merge tickets")}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-merge" loading={busy} disabled={!ready}>
            {others.length === 1 ? t("Merge 1 ticket") : t("Merge {n} tickets", { n: others.length })}
          </Button>
        </>
      }
    >
      <form id="hd-merge" className="stack" onSubmit={submit}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        {refused.length > 0 && (
          <Alert tone="warning">
            <ul>
              {refused.map((r) => (
                <li key={r}>{isolate(r)}</li>
              ))}
            </ul>
          </Alert>
        )}
        {keep ? (
          <fieldset className="hd-merge-set">
            <legend>{t("Tickets to merge into this one")}</legend>
            {loading ? (
              <p className="faint" role="status">{t("Loading the customer's tickets")}</p>
            ) : failed ? (
              <p className="faint">{t("The customer's other tickets could not be loaded.")}</p>
            ) : tickets.filter((x) => x.id !== keep).length === 0 ? (
              <p className="faint">{t("This customer has no other tickets.")}</p>
            ) : (
              tickets
                .filter((x) => x.id !== keep)
                .map((x) => (
                  <label key={x.id} className="hd-check">
                    <input
                      type="checkbox"
                      checked={moved.has(x.id)}
                      disabled={x.status === "Closed"}
                      onChange={(e) => setMoved((s) => new Set(e.target.checked ? [...s, x.id] : [...s].filter((i) => i !== x.id)))}
                    />
                    {isolate(label(x))}
                    {x.status === "Closed" && <span className="faint"> {t("(closed)")}</span>}
                  </label>
                ))
            )}
          </fieldset>
        ) : (
          <fieldset className="hd-merge-set">
            <legend>{t("The ticket that stays")}</legend>
            {tickets.map((x) => (
              <label key={x.id} className="hd-check">
                <input type="radio" name="hd-merge-keep" checked={survivorId === x.id} onChange={() => setSurvivorId(x.id)} />
                {isolate(label(x))}
              </label>
            ))}
          </fieldset>
        )}
        {survivor && others.length > 0 && (
          <p>
            {t("The conversation, files, comments, watchers and tags of {from} move to {to}, in time order, each message marked with the ticket it came from. {from} then closes as merged: it cannot be replied to or reopened, and a customer's reply to it lands on {to}. This cannot be undone.", {
              from: others.map((o) => o.number ?? t("a ticket")).join(", "),
              to: survivor.number ?? t("this ticket"),
            })}
          </p>
        )}
      </form>
    </Dialog>
  );
}
