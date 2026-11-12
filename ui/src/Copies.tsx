// Who is copied (CC) on a ticket's replies: shown, and changed where it stands (the ticket's `cc` field,
// against the version on screen). Replies carry the copies; internal notes never go to anyone.

import { type FormEvent, useEffect, useId, useState } from "react";
import { isolate, t as tr } from "@ferrum/client";
import { Button, Icon } from "@ferrum/kit";
import { copyList, copyProblem, copyText } from "./copywords";

export function Copies({ cc, contact, editable, save }: { cc?: string | null; contact?: string | null; editable: boolean; save: (field: string, value: unknown) => Promise<void> }) {
  const own = useId();
  const list = copyList(cc);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const problem = copyProblem(draft, list, contact);
  const put = async (next: string[]) => {
    if (busy) return false;
    setBusy(true);
    try {
      await save("cc", copyText(next));
      return true;
    } catch {
      return false;
    } finally {
      setBusy(false);
    }
  };
  // The box empties once the address is among the copies, not before: a refusal leaves it to be corrected.
  useEffect(() => {
    if (draft.trim() && list.includes(draft.trim().toLowerCase())) setDraft("");
  }, [cc]); // eslint-disable-line react-hooks/exhaustive-deps
  const add = async (e: FormEvent) => {
    e.preventDefault();
    if (!draft.trim() || problem) return;
    await put([...list, draft.trim().toLowerCase()]);
  };
  if (!list.length && !editable) return null;
  return (
    <section className="hd-card" aria-labelledby={`${own}-h`}>
      <h3 id={`${own}-h`}>{tr("Copied on replies")}</h3>
      {list.length ? (
        <ul className="hd-tags" aria-label={tr("Addresses copied on replies")}>
          {list.map((a) => (
            <li key={a} className="hd-tag">
              <span>{isolate(a)}</span>
              {editable && (
                <button type="button" className="icon-btn icon-btn-sm" aria-label={tr("Stop copying {a}", { a })} disabled={busy} onClick={() => void put(list.filter((x) => x !== a))}>
                  <Icon name="x" size={12} />
                </button>
              )}
            </li>
          ))}
        </ul>
      ) : (
        <p className="faint">{tr("Nobody is copied.")}</p>
      )}
      {editable && (
        <form className="hd-tag-add" onSubmit={(e) => void add(e)}>
          <input
            className="input"
            type="email"
            aria-label={tr("Copy another address")}
            aria-invalid={!!problem || undefined}
            aria-describedby={problem ? `${own}-problem` : undefined}
            placeholder={tr("name@example.com")}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          <Button type="submit" size="sm" loading={busy} disabled={!draft.trim() || !!problem}>
            {tr("Copy")}
          </Button>
          {problem && (
            <p id={`${own}-problem`} className="hd-warn" role="status">
              {problem}
            </p>
          )}
        </form>
      )}
    </section>
  );
}
