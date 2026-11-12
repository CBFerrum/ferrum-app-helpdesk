// Macros, found from a ticket: pick one, read what it will change on this ticket (and whether it sends the
// customer a reply), then apply it. The server runs every step in one save as the person who presses the
// button, all or none, so a refusal leaves the ticket as it was and is shown as it is worded.

import { useQuery } from "@tanstack/react-query";
import { useId, useMemo, useState } from "react";
import { api, docPath, errorMessage, isolate, newKey, t } from "@ferrum/client";
import { Alert, Button, Dialog, ErrorState, Spinner, toast } from "@ferrum/kit";
import { useMe } from "@ferrum/records";
import { type Current, effects, findMacros, type Macro, type MacroScope, sendsReply } from "./macros";
import { useRemembered } from "./hooks";

type Row = Macro & { created_by?: string | null };

export function useMacros(enabled = true) {
  return useQuery({
    queryKey: ["helpdesk", "macros"],
    enabled,
    staleTime: 60_000,
    queryFn: ({ signal }) => api.get<{ rows: Row[] }>("/api/v1/SupportMacro", { fields: "macro_name,scope,team,uses,created_by", sort: "uses desc", limit: 500 }, signal).then((r) => r.rows),
  });
}

const WHO: Record<Macro["scope"], string> = { get Personal() { return t("Yours"); }, get Team() { return t("Team"); }, get Everyone() { return t("Everyone"); } };
export const MACRO_SCOPES: { id: MacroScope; label: string }[] = [
  { id: "all", get label() { return t("All"); } },
  { id: "mine", get label() { return t("Personal"); } },
  { id: "team", get label() { return t("My teams"); } },
  { id: "everyone", get label() { return t("Everyone"); } },
];

export interface MacroTicket {
  id: string;
  number?: string | null;
  current: Current;
}

export function MacroPicker({ ticket, open, onClose, onApplied }: { ticket: MacroTicket; open: boolean; onClose: () => void; onApplied: () => void }) {
  const me = useMe().data?.user;
  const list = useMacros(open);
  const [query, setQuery] = useState("");
  const [scope, setScope] = useRemembered<MacroScope>("helpdesk:macro-scope", "all");
  const [chosen, setChosen] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const id = useId();

  const found = useMemo(() => findMacros(list.data ?? [], { query, scope, me: me?.id }), [list.data, query, scope, me?.id]);
  const pickedId = chosen && found.some((m) => m.id === chosen) ? chosen : (found[0]?.id ?? null);
  const picked = found.find((m) => m.id === pickedId);
  // The steps are on the macro's own record, not in the list.
  const detail = useQuery({
    queryKey: ["helpdesk", "macro", pickedId],
    enabled: open && !!pickedId,
    staleTime: 60_000,
    queryFn: ({ signal }) => api.get<Macro>(docPath("SupportMacro", pickedId ?? ""), undefined, signal),
  });
  const steps = useMemo(() => detail.data?.steps ?? [], [detail.data]);
  const will = useMemo(() => effects(steps, ticket.current), [steps, ticket.current]);
  const sends = sendsReply(steps);

  const apply = async () => {
    if (!pickedId || busy) return;
    setBusy(true);
    setRefusal(null);
    try {
      // Sent once whatever happens to the connection: the key makes a second press the same request.
      const res = await api.post<{ applied?: number; macro?: string }>(docPath("Ticket", ticket.id, "actions", "apply_macro"), { macro: pickedId }, { idempotencyKey: newKey() });
      toast(t("{macro} applied: {applied} steps", { macro: res.macro ?? picked?.macro_name ?? "", applied: res.applied ?? steps.length }));
      onApplied();
      onClose();
    } catch (e) {
      setRefusal(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={t("Apply a macro to {value}", { value: ticket.number ?? t("this ticket") })}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" loading={busy} disabled={!picked || detail.isLoading || !!detail.error || !steps.length} onClick={() => void apply()}>
            {sends ? t("Apply and send the reply") : t("Apply macro")}
          </Button>
        </>
      }
    >
      <div className="stack hd-macros">
        <div className="hd-picker-top">
          <input className="input" type="search" aria-label={t("Search macros")} placeholder={t("Find a macro…")} value={query} onChange={(e) => setQuery(e.target.value)} />
          <div className="segmented" role="group" aria-label={t("Whose macros")}>
            {MACRO_SCOPES.map((f) => (
              <button key={f.id} type="button" aria-pressed={scope === f.id} onClick={() => setScope(f.id)}>
                {isolate(f.label)}
              </button>
            ))}
          </div>
        </div>
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.refetch()} title={t("Macros did not load")} />
        ) : !list.data ? (
          <Spinner label={t("Loading macros")} />
        ) : !found.length ? (
          <p className="muted">{list.data.length ? (query ? t("Nothing matches “{query}”.", { query }) : t("None here yet. Try another group above.")) : t("No macros yet. A manager makes them under Administration, Macros.")}</p>
        ) : (
          <div className="hd-macro-body">
            <ul role="listbox" aria-label={t("Macros")} id={`${id}-list`} className="hd-picker-list">
              {found.map((m) => (
                <li
                  key={m.id}
                  role="option"
                  aria-selected={m.id === pickedId}
                  tabIndex={0}
                  data-active={m.id === pickedId || undefined}
                  className="hd-picker-opt"
                  onClick={() => setChosen(m.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setChosen(m.id);
                    }
                  }}
                >
                  <span className="hd-picker-title">{isolate(m.macro_name)}</span>
                  <span className="faint">{isolate(WHO[m.scope])}</span>
                </li>
              ))}
            </ul>
            <div className="hd-picker-preview" aria-live="polite">
              <h3>{t("What it will do to this ticket")}</h3>
              {detail.isLoading ? (
                <Spinner label={t("Loading the steps")} />
              ) : detail.error ? (
                <ErrorState error={detail.error} onRetry={() => void detail.refetch()} title={t("The steps did not load")} />
              ) : (
                <ol className="hd-effects">
                  {will.map((e, i) => (
                    <li key={i} data-kind={e.kind}>
                      {e.kind === "reply" && <strong>{t("Sent to the customer")} </strong>}
                      {e.kind === "note" && <strong>{t("Team only")} </strong>}
                      {isolate(e.text)}
                    </li>
                  ))}
                </ol>
              )}
              {sends && <Alert tone="warning">{t("This macro sends the customer a reply. It goes out when you apply it, and merge fields are filled in from this ticket then.")}</Alert>}
              {steps.length > 0 && <p className="faint">{t("Every step applies, or none does.")}</p>}
            </div>
          </div>
        )}
        {refusal && <Alert tone="error">{isolate(refusal)}</Alert>}
      </div>
    </Dialog>
  );
}
