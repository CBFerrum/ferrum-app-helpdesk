// A ticket's tags as chips with the colour the workspace gave each tag: add one (from the workspace's
// tags or a new word) and remove one. The tags are the platform's (`/api/v1/Ticket/{id}/tags`), so a
// list can filter by them and a merge carries them over.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type CSSProperties, type FormEvent, useId, useState } from "react";
import { api, docPath, errorMessage, isolate, t as tr } from "@ferrum/client";
import { Button, Icon, Spinner, toast } from "@ferrum/kit";
import { cleanTag, colourOf, colours, type TagDef, tagProblem } from "./tagwords";
import { LoadFailed } from "./states";

/** The tags the workspace defined, with their colours. A person who may not read them still sees the words. */
export function useTagDefs() {
  return useQuery({
    queryKey: ["helpdesk", "tag-defs"],
    staleTime: 5 * 60_000,
    queryFn: ({ signal }) =>
      api
        .get<{ rows: TagDef[] }>("/api/v1/TicketTag", { fields: "tag_name,colour,description", sort: "tag_name asc", limit: 500 }, signal)
        .then((r) => r.rows)
        .catch(() => [] as TagDef[]),
  });
}

export function TagChip({ tag, colour, onRemove, busy }: { tag: string; colour?: string | null; onRemove?: () => void; busy?: boolean }) {
  const c = colourOf(colour);
  return (
    <li className="hd-tag">
      <span className="hd-tag-dot" aria-hidden="true" style={c ? ({ "--tag": c } as CSSProperties) : undefined} />
      <span>{isolate(tag)}</span>
      {onRemove && (
        <button type="button" className="icon-btn icon-btn-sm" aria-label={tr("Remove the tag {tag}", { tag })} disabled={busy} onClick={onRemove}>
          <Icon name="x" size={12} />
        </button>
      )}
    </li>
  );
}

export function TicketTags({ id, editable }: { id: string; editable: boolean }) {
  const qc = useQueryClient();
  const key = ["helpdesk", "tags", id];
  const own = useId();
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const defs = useTagDefs();
  const q = useQuery({ queryKey: key, queryFn: ({ signal }) => api.get<{ tags: string[] }>(docPath("Ticket", id, "tags"), undefined, signal) });
  const tags = q.data?.tags ?? [];
  const shade = colours(defs.data ?? []);
  const problem = tagProblem(draft, tags);

  /** The whole list is what the server takes; a double press cannot send twice. */
  const put = async (next: string[]): Promise<boolean> => {
    if (busy) return false;
    setBusy(true);
    try {
      const saved = await api.put<{ tags: string[] }>(docPath("Ticket", id, "tags"), { tags: next });
      qc.setQueryData(key, saved);
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
      return true;
    } catch (e) {
      toast(errorMessage(e), "error");
      return false;
    } finally {
      setBusy(false);
    }
  };
  const add = async (e: FormEvent) => {
    e.preventDefault();
    const t = cleanTag(draft);
    if (!t || problem) return;
    if (await put([...tags, t])) setDraft("");
  };

  return (
    <section className="hd-card" aria-labelledby={`${own}-h`}>
      <h3 id={`${own}-h`}>{tr("Tags")}</h3>
      {q.isLoading ? (
        <Spinner label={tr("Loading the tags")} />
      ) : q.error ? (
        <LoadFailed error={q.error} onRetry={() => void q.refetch()} title={tr("The tags did not load")} />
      ) : (
        <>
          {tags.length ? (
            <ul className="hd-tags" aria-label={tr("Tags on this ticket")}>
              {tags.map((t) => (
                <TagChip key={t} tag={t} colour={shade.get(t.toLowerCase())} busy={busy} onRemove={editable ? () => void put(tags.filter((x) => x !== t)) : undefined} />
              ))}
            </ul>
          ) : (
            <p className="faint">{tr("No tags.")}</p>
          )}
          {editable && (
            <form className="hd-tag-add" onSubmit={(e) => void add(e)}>
              <input
                className="input"
                list={`${own}-known`}
                aria-label={tr("Add a tag")}
                aria-invalid={!!problem || undefined}
                aria-describedby={problem ? `${own}-problem` : undefined}
                placeholder={tr("Add a tag")}
                autoComplete="off"
                maxLength={40}
                value={draft}
                onChange={(e) => setDraft(e.target.value)}
              />
              <datalist id={`${own}-known`}>
                {(defs.data ?? []).filter((d) => !tags.includes(d.tag_name)).map((d) => (
                  <option key={d.tag_name} value={d.tag_name} />
                ))}
              </datalist>
              <Button type="submit" size="sm" loading={busy} disabled={!draft.trim() || !!problem}>
                {tr("Add")}
              </Button>
              {problem && (
                <p id={`${own}-problem`} className="hd-warn" role="status">
                  {problem}
                </p>
              )}
            </form>
          )}
        </>
      )}
    </section>
  );
}
