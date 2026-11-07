// Saved replies, found from the composer: type to search, arrows and Enter to choose. A reply is
// filled in from the ticket before it goes into the text, so what you read is what is sent.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type KeyboardEvent, useId, useMemo, useRef, useState } from "react";
import { Link } from "react-router";
import { api, docPath, errorMessage, t, isolate } from "@ferrum/client";
import { Alert, Button, Dialog, ErrorState, Field, Spinner, toast, useDismiss } from "@ferrum/kit";
import { LinkInput, useMe } from "@ferrum/records";
import { useRemembered } from "./hooks";
import { fill, type FillWith, pick, SCOPE_FILTERS, type ReplyScope, type SavedReply, type ScopeFilter } from "./replies";

const KEY = ["helpdesk", "replies"];

export function useReplies(enabled = true) {
  return useQuery({
    queryKey: KEY,
    enabled,
    staleTime: 60_000,
    queryFn: ({ signal }) =>
      api.get<{ rows: SavedReply[] }>("/api/v1/SavedReply", { fields: "title,body,scope,team,uses,created_by", sort: "uses desc", limit: 500 }, signal).then((r) => r.rows),
  });
}

const WHO: Record<ReplyScope, string> = { get Personal() { return t("Yours"); }, get Team() { return t("Team"); }, get Everyone() { return t("Everyone"); } };

/** Keeping what was typed as a reply to use again. */
function SaveReply({ body, onClose }: { body: string; onClose: () => void }) {
  const qc = useQueryClient();
  const [title, setTitle] = useState("");
  const [scope, setScope] = useState<ReplyScope>("Personal");
  const [team, setTeam] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const ready = title.trim() && (scope !== "Team" || team);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!ready) return;
    setBusy(true);
    try {
      await api.post("/api/v1/SavedReply", { title: title.trim(), scope, team: scope === "Team" ? team : null, body });
      void qc.invalidateQueries({ queryKey: KEY });
      toast(t("Saved as a reply"));
      onClose();
    } catch (err) {
      setProblem(errorMessage(err));
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={t("Save as a saved reply")}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-save-reply" loading={busy} disabled={!ready}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <form id="hd-save-reply" className="stack" onSubmit={submit}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        <Field label={t("Name")} htmlFor="hd-reply-title" required>
          <input id="hd-reply-title" className="input" maxLength={120} value={title} onChange={(e) => setTitle(e.target.value)} />
        </Field>
        <Field label={t("Who can use it")} htmlFor="hd-reply-scope">
          <select id="hd-reply-scope" className="select" value={scope} onChange={(e) => setScope(e.target.value as ReplyScope)}>
            <option value="Personal">{t("Only me")}</option>
            <option value="Team">{t("A team")}</option>
            <option value="Everyone">{t("Every agent")}</option>
          </select>
        </Field>
        {scope === "Team" && (
          <Field label={t("Team")} htmlFor="hd-reply-team" required>
            <LinkInput to="SupportTeam" id="hd-reply-team" value={team} onChange={setTeam} required />
          </Field>
        )}
        <p className="faint">
          {t("It is saved as typed. To have the customer's name filled in each time, write")} <code>{isolate("{{customer.first_name}}")}</code> {t("in its place.")}
        </p>
      </form>
    </Dialog>
  );
}

export function ReplyPicker({ about, draft, onPick, onClose }: { about: FillWith; draft: string; onPick: (text: string) => void; onClose: () => void }) {
  const qc = useQueryClient();
  const me = useMe().data?.user;
  const list = useReplies();
  const [query, setQuery] = useState("");
  const [scope, setScope] = useRemembered<ScopeFilter>("helpdesk:reply-scope", "all");
  const [at, setAt] = useState(0);
  const [saving, setSaving] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const id = useId();
  useDismiss(box, !saving, onClose);

  const found = useMemo(() => pick(list.data ?? [], { query, scope, me: me?.id }), [list.data, query, scope, me?.id]);
  const active = Math.min(at, found.length - 1);
  const chosen: SavedReply | undefined = found[active];
  const filled = chosen ? fill(chosen.body, about) : null;

  const choose = (r: SavedReply) => {
    // Counting a use is for ordering the list next time; it never holds up the reply.
    void api
      .post(docPath("SavedReply", r.id, "actions", "used"), {})
      .then(() => qc.invalidateQueries({ queryKey: KEY }))
      .catch(() => {});
    onPick(fill(r.body, about).text);
  };
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setAt(Math.min(found.length - 1, active + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setAt(Math.max(0, active - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (chosen) choose(chosen);
    }
  };

  return (
    <div ref={box} className="hd-picker" role="dialog" aria-label={t("Saved replies")}>
      <div className="hd-picker-top">
        <input
          className="input"
          type="search"
          role="combobox"
          aria-label={t("Search saved replies")}
          aria-expanded="true"
          aria-controls={`${id}-list`}
          aria-activedescendant={chosen ? `${id}-${chosen.id}` : undefined}
          aria-autocomplete="list"
          placeholder={t("Find a saved reply…")}
          autoFocus
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setAt(0);
          }}
          onKeyDown={onKey}
        />
        <div className="segmented" role="group" aria-label={t("Whose replies")}>
          {SCOPE_FILTERS.map((f) => (
            <button
              key={f.id}
              type="button"
              aria-pressed={scope === f.id}
              onClick={() => {
                setScope(f.id);
                setAt(0);
              }}
            >
              {isolate(f.label)}
            </button>
          ))}
        </div>
      </div>
      <div className="hd-picker-body">
        {list.error ? (
          <ErrorState error={list.error} onRetry={() => void list.refetch()} title={t("Saved replies did not load")} />
        ) : !list.data ? (
          <Spinner label={t("Loading saved replies")} />
        ) : !found.length ? (
          <p className="muted hd-picker-empty">
            {list.data.length ? (query ? t("Nothing matches “{query}”.", { query }) : t("None here yet. Try another group above.")) : t("No saved replies yet. Write one and save it with the button below.")}
          </p>
        ) : (
          <>
            <ul role="listbox" id={`${id}-list`} aria-label={t("Saved replies")} className="hd-picker-list">
              {found.map((r, i) => (
                <li
                  key={r.id}
                  id={`${id}-${r.id}`}
                  role="option"
                  aria-selected={i === active}
                  data-active={i === active || undefined}
                  className="hd-picker-opt"
                  onMouseEnter={() => setAt(i)}
                  onClick={() => choose(r)}
                >
                  <span className="hd-picker-title">{isolate(r.title)}</span>
                  <span className="faint">{isolate(WHO[r.scope])}</span>
                </li>
              ))}
            </ul>
            {filled && (
              <div className="hd-picker-preview" aria-live="polite">
                <h3>{t("For this ticket")}</h3>
                <p className="hd-picker-text">{isolate(filled.text)}</p>
                {filled.missing.length > 0 && (
                  <p className="hd-warn">
                    {isolate(filled.missing.map((m) => `{{${m}}}`).join(", "))} {t("cannot be filled from this ticket. It stays in the text for you to write, and sending waits for it.")}
                  </p>
                )}
              </div>
            )}
          </>
        )}
      </div>
      <div className="hd-picker-foot">
        <Link to="/e/SavedReply" className="faint">
          {t("Manage saved replies")}
        </Link>
        <span className="spacer" />
        <Button size="sm" disabled={!draft.trim()} onClick={() => setSaving(true)}>
          {t("Save what I have written")}
        </Button>
        <Button size="sm" variant="ghost" onClick={onClose}>
          {t("Close")}
        </Button>
      </div>
      {saving && <SaveReply body={draft} onClose={() => setSaving(false)} />}
    </div>
  );
}
