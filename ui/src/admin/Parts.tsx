// What every administration section is made of: a table of a kind of record with its add and edit dialog,
// and a card for a setting's few fields. A write carries the version on screen, so two managers editing at
// once is told to the second; a refusal is shown as the server words it; a double press never sends twice.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useEffect, useId, useMemo, useState } from "react";
import { api, ApiError, docPath, errorMessage, isolate, newKey, t } from "@ferrum/client";
import { Alert, Button, confirmDialog, Dialog, Field, ListSkeleton, StatePanel, toast } from "@ferrum/kit";
import { can, LinkInput, useMetaIndex } from "@ferrum/records";
import { LoadFailed } from "../states";
import { bodyOf, type Draft, draftOf, type FieldSpec, problems } from "./form";

export type Row = Record<string, unknown> & { id: string; version?: number; _titles?: Record<string, string> };

/** One field of a draft as its control. */
export function Control({ f, draft, set, id, error, disabled, title }: { f: FieldSpec; draft: Draft; set: (name: string, v: string | boolean) => void; id: string; error?: string; disabled?: boolean; title?: string }) {
  const v = draft[f.name];
  const text = typeof v === "string" ? v : "";
  const common = { id, disabled, "aria-invalid": error ? true : undefined } as const;
  if (f.kind === "check")
    return (
      <label className="hd-check">
        <input type="checkbox" id={id} checked={!!v} disabled={disabled} onChange={(e) => set(f.name, e.target.checked)} />
        {f.label}
        {f.hint && <span className="faint"> {f.hint}</span>}
      </label>
    );
  const control = (() => {
    switch (f.kind) {
      case "select":
        return (
          <select {...common} className="select" value={text} onChange={(e) => set(f.name, e.target.value)}>
            {!f.required && <option value="">{f.noneLabel ?? t("None")}</option>}
            {(f.options ?? []).map((o) => (
              <option key={o} value={o}>
                {isolate(f.optionLabels?.[o] ?? o)}
              </option>
            ))}
          </select>
        );
      case "textarea":
      case "markdown":
      case "code":
        return <textarea {...common} className={f.kind === "code" ? "textarea hd-code" : "textarea"} rows={f.rows ?? 4} spellCheck={f.kind !== "code"} value={text} onChange={(e) => set(f.name, e.target.value)} />;
      case "link":
        return <LinkInput to={f.to ?? ""} id={id} value={text || null} title={title} disabled={disabled} onChange={(x) => set(f.name, x ?? "")} />;
      case "colour":
        return (
          <div className="hd-colour">
            <input {...common} className="input" value={text} placeholder="#2563eb" maxLength={7} onChange={(e) => set(f.name, e.target.value)} />
            {/* ui-lint-allow raw-color: a colour input needs some #rrggbb while the tag has none; the colour chosen is the tag's own data */}
            <input type="color" aria-label={t("Pick a colour")} value={/^#[0-9a-f]{6}$/i.test(text) ? text : "#2563eb"} disabled={disabled} onChange={(e) => set(f.name, e.target.value)} />
          </div>
        );
      case "int":
        return <input {...common} className="input" type="number" inputMode="numeric" min={f.min} max={f.max} value={text} onChange={(e) => set(f.name, e.target.value)} />;
      case "email":
        return <input {...common} className="input" type="email" value={text} onChange={(e) => set(f.name, e.target.value)} />;
      default:
        return <input {...common} className="input" value={text} onChange={(e) => set(f.name, e.target.value)} />;
    }
  })();
  return (
    <Field label={f.label} htmlFor={id} required={f.required} error={error} hint={f.hint}>
      {control}
    </Field>
  );
}

/** A record in a dialog: new, or edited against its version. Closes when the server has taken it. */
export function RecordDialog({
  entity,
  spec,
  record,
  defaults,
  title,
  extra,
  onClose,
  onSaved,
}: {
  entity: string;
  spec: FieldSpec[];
  record: Row | null;
  defaults?: Record<string, unknown>;
  title: string;
  /** More fields under the form's own, for a record with a part of its own (a macro's steps). */
  extra?: (draft: Draft, set: (name: string, v: string | boolean) => void) => ReactNode;
  onClose: () => void;
  onSaved: (saved: Row) => void;
}) {
  const own = useId();
  const [draft, setDraft] = useState<Draft>(() => draftOf(spec, record, defaults));
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [shown, setShown] = useState(false);
  const bad = problems(spec, draft);
  const set = (name: string, v: string | boolean) => setDraft((d) => ({ ...d, [name]: v }));
  const [key] = useState(newKey);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setShown(true);
    if (busy || Object.keys(bad).length) return;
    setBusy(true);
    setRefusal(null);
    try {
      const body = bodyOf(spec, draft, record);
      const saved = record
        ? await api.patch<Row>(docPath(entity, record.id), body, { ifMatch: Number(record.version) })
        : await api.post<Row>(docPath(entity), body, { idempotencyKey: key });
      onSaved(saved);
      onClose();
    } catch (err) {
      setRefusal(err instanceof ApiError && (err.status === 409 || err.status === 412) ? t("Someone else changed this while you had it open. Close it and open it again to see their change.") : errorMessage(err));
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={title}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form={`${own}-form`} loading={busy}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <form id={`${own}-form`} className="stack" onSubmit={(e) => void submit(e)} noValidate>
        {refusal && <Alert tone="error">{isolate(refusal)}</Alert>}
        {spec
          .filter((f) => !f.when || f.when(draft))
          .map((f) => (
            <Control key={f.name} f={f} draft={draft} set={set} id={`${own}-${f.name}`} error={shown ? bad[f.name] : undefined} title={record?._titles?.[f.name]} />
          ))}
        {extra?.(draft, set)}
      </form>
    </Dialog>
  );
}

export interface Column {
  key: string;
  label: string;
  cell: (r: Row) => ReactNode;
}

/** Whether the person may read, create, change and delete a kind of record, as the platform says. */
export function useRights(entity: string) {
  const index = useMetaIndex();
  const e = index.data?.entities.find((x) => x.name === entity);
  return { loaded: !!index.data, read: !e ? false : can(e.permissions, "read"), create: !!e && can(e.permissions, "create"), write: !!e && can(e.permissions, "write"), del: !!e && can(e.permissions, "delete") };
}

export function useAdminRows(entity: string, fields: string, sort: string, filters?: unknown[]) {
  return useQuery({
    queryKey: ["helpdesk", "admin", entity, sort, filters ?? null],
    queryFn: ({ signal }) => api.get<{ rows: Row[] }>(docPath(entity), { fields, sort, limit: 500, titles: true, filters: filters ? JSON.stringify(filters) : undefined }, signal).then((r) => r.rows),
  });
}

/** The admin screens read each other's records (a tag's colour in the queues, a macro in the picker): they are looked at again after a change. */
export const useRefresh = () => {
  const qc = useQueryClient();
  return (entity: string) => {
    void qc.invalidateQueries({ queryKey: ["helpdesk", "admin", entity] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "tag-defs"] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "macros"] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "macro"] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "teams"] });
  };
};

export function AdminTable({
  entity,
  noun,
  fields,
  sort,
  spec,
  columns,
  defaults,
  note,
  rowActions,
  extra,
  order,
  empty,
  hideAdd,
  hideEdit,
  headActions,
}: {
  entity: string;
  /** What one record is called, for the buttons: "support address". */
  noun: string;
  /** The fields asked of the list. */
  fields: string;
  sort: string;
  spec: FieldSpec[];
  columns: Column[];
  defaults?: Record<string, unknown>;
  note?: ReactNode;
  rowActions?: (r: Row, rows: Row[]) => ReactNode;
  extra?: (draft: Draft, set: (name: string, v: string | boolean) => void) => ReactNode;
  /** Rows come in this order unless the sort says otherwise (the list is already sorted by the server). */
  order?: (rows: Row[]) => Row[];
  empty: string;
  /** A section with its own editor draws its own add and edit buttons. */
  hideAdd?: boolean;
  hideEdit?: boolean;
  headActions?: ReactNode;
}) {
  const rights = useRights(entity);
  const rows = useAdminRows(entity, fields, sort);
  const refresh = useRefresh();
  const [editing, setEditing] = useState<Row | "new" | null>(null);
  const list = useMemo(() => (rows.data ? (order ? order(rows.data) : rows.data) : []), [rows.data, order]);
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  if (rows.error) return <LoadFailed error={rows.error} onRetry={() => void rows.refetch()} />;
  const remove = async (r: Row, name: string) => {
    if (!(await confirmDialog({ title: t("Delete “{name}”?", { name }), message: t("This cannot be undone."), confirmLabel: t("Delete"), danger: true }))) return;
    try {
      await api.del(docPath(entity, r.id));
      toast(t("Deleted"));
      refresh(entity);
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  return (
    <div className="stack">
      {note}
      <div className="row">
        <span className="spacer" />
        {headActions}
        {rights.create && !hideAdd && (
          <Button variant="primary" icon="plus" onClick={() => setEditing("new")}>
            {t("Add {noun}", { noun })}
          </Button>
        )}
      </div>
      {!rows.data ? (
        <ListSkeleton rows={5} />
      ) : !list.length ? (
        <StatePanel kind="empty" title={empty} />
      ) : (
        <div className="hd-table-wrap">
          <table className="table hd-table">
            <thead>
              <tr>
                {columns.map((c) => (
                  <th key={c.key} scope="col">
                    {c.label}
                  </th>
                ))}
                <th scope="col">
                  <span className="sr-only">{t("Actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {list.map((r) => (
                <tr key={r.id}>
                  {columns.map((c) => (
                    <td key={c.key}>{c.cell(r)}</td>
                  ))}
                  <td className="hd-table-actions">
                    {rowActions?.(r, list)}
                    {rights.write && !hideEdit && (
                      <Button size="sm" onClick={() => setEditing(r)} aria-label={t("Edit {name}", { name: String(columns[0].cell(r) ?? noun) })}>
                        {t("Edit")}
                      </Button>
                    )}
                    {rights.del && (
                      <Button size="sm" variant="ghost" onClick={() => void remove(r, String(r[spec[0].name] ?? noun))}>
                        {t("Delete")}
                      </Button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
      {editing && (
        <RecordDialog
          entity={entity}
          spec={spec}
          record={editing === "new" ? null : editing}
          defaults={defaults}
          title={editing === "new" ? t("Add {noun}", { noun }) : t("Edit {noun}", { noun })}
          extra={extra}
          onClose={() => setEditing(null)}
          onSaved={() => {
            toast(t("Saved"));
            refresh(entity);
          }}
        />
      )}
    </div>
  );
}

/** The workspace's one Support Settings record: a few of its fields, saved together. */
export function SettingsCard({ title, intro, spec, children }: { title: string; intro?: ReactNode; spec: FieldSpec[]; children?: (draft: Draft) => ReactNode }) {
  const own = useId();
  const qc = useQueryClient();
  const rights = useRights("SupportSettings");
  const q = useQuery({
    queryKey: ["helpdesk", "settings"],
    queryFn: ({ signal }) => api.get<{ rows: Row[] }>(docPath("SupportSettings"), { limit: 1 }, signal).then((r) => r.rows[0] ?? null),
  });
  const [draft, setDraft] = useState<Draft | null>(() => (q.data !== undefined ? draftOf(spec, q.data) : null));
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  useEffect(() => {
    if (q.data !== undefined) setDraft(draftOf(spec, q.data));
  }, [q.data]); // eslint-disable-line react-hooks/exhaustive-deps
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  if (q.error) return <LoadFailed error={q.error} onRetry={() => void q.refetch()} />;
  if (!draft) return <ListSkeleton rows={3} />;
  const bad = problems(spec, draft);
  const dirty = Object.keys(bodyOf(spec, draft, q.data ?? {})).length > 0;
  const set = (name: string, v: string | boolean) => setDraft((d) => ({ ...(d as Draft), [name]: v }));
  const save = async (e: FormEvent) => {
    e.preventDefault();
    if (busy || !dirty || Object.keys(bad).length) return;
    setBusy(true);
    setRefusal(null);
    try {
      if (q.data) await api.patch(docPath("SupportSettings", q.data.id), bodyOf(spec, draft, q.data), { ifMatch: Number(q.data.version) });
      else await api.post(docPath("SupportSettings"), bodyOf(spec, draft), { idempotencyKey: newKey() });
      await qc.invalidateQueries({ queryKey: ["helpdesk", "settings"] });
      toast(t("Saved"));
    } catch (err) {
      setRefusal(err instanceof ApiError && (err.status === 409 || err.status === 412) ? t("Someone else changed the settings. They are up to date now; make your change again.") : errorMessage(err));
      void qc.invalidateQueries({ queryKey: ["helpdesk", "settings"] });
    } finally {
      setBusy(false);
    }
  };
  return (
    <section className="hd-card" aria-labelledby={`${own}-h`}>
      <h3 id={`${own}-h`}>{title}</h3>
      {intro && <p className="faint">{intro}</p>}
      <form className="stack" onSubmit={(e) => void save(e)} noValidate>
        {refusal && <Alert tone="error">{isolate(refusal)}</Alert>}
        {spec
          .filter((f) => !f.when || f.when(draft))
          .map((f) => (
            <Control key={f.name} f={f} draft={draft} set={set} id={`${own}-${f.name}`} error={bad[f.name]} disabled={!rights.write} />
          ))}
        {children?.(draft)}
        {rights.write ? (
          <div className="row">
            <Button type="submit" variant="primary" loading={busy} disabled={!dirty || Object.keys(bad).length > 0}>
              {t("Save settings")}
            </Button>
          </div>
        ) : (
          <p className="faint">{t("You can read these settings but not change them.")}</p>
        )}
      </form>
    </section>
  );
}
