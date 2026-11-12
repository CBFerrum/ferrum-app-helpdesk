// The macro editor: a name, who may use it, and the steps in the order they run. The steps are the server's
// own (set status, priority, team, type, language, assignee or resolution; add or remove a tag; a public reply;
// a private note); a macro applies all of them or none. Teams, types and tags are chosen from the workspace's own.

import { useState } from "react";
import { api, ApiError, docPath, errorMessage, isolate, newKey, t } from "@ferrum/client";
import { Alert, Badge, Button, Dialog, Field, Icon, toast } from "@ferrum/kit";
import { LinkInput } from "@ferrum/records";
import { useNames } from "../hooks";
import { PRIORITIES } from "../inbox";
import { type Macro, type MacroStep, sendsReply, STEP_KINDS, stepLine, stepsProblem } from "../macros";
import { moved } from "./order";
import { type Column, AdminTable, type Row, useAdminRows, useRefresh, useRights } from "./Parts";

const STATUSES = ["Open", "Replied", "On Hold", "Resolved", "Closed"];
const LONG = ["Public Reply", "Private Note", "Set Resolution"];
const KIND_LABEL: Record<string, () => string> = {
  "Set Status": () => t("Set the status"),
  "Set Priority": () => t("Set the priority"),
  "Set Team": () => t("Move to a team"),
  "Assign To": () => t("Assign to"),
  "Set Type": () => t("Set the type"),
  "Set Language": () => t("Set the language"),
  "Set Resolution": () => t("Write how it was resolved"),
  "Add Tag": () => t("Add a tag"),
  "Remove Tag": () => t("Remove a tag"),
  "Public Reply": () => t("Send the customer a reply"),
  "Private Note": () => t("Add an internal note"),
};

/** One step's value, as the control its kind asks for. */
function Value({ s, id, onChange }: { s: MacroStep; id: string; onChange: (v: string) => void }) {
  const names = useNames();
  const teams = useAdminRows("SupportTeam", "team_name", "team_name asc");
  const types = useAdminRows("TicketType", "type_name", "type_name asc");
  const tags = useAdminRows("TicketTag", "tag_name", "tag_name asc");
  const v = s.value ?? "";
  const pick = (label: string, options: string[], none?: string) => (
    <select id={id} className="select" aria-label={label} value={v} onChange={(e) => onChange(e.target.value)}>
      <option value="">{none ?? t("Choose…")}</option>
      {v && !options.includes(v) && <option value={v}>{isolate(v)}</option>}
      {options.map((o) => (
        <option key={o}>{isolate(o)}</option>
      ))}
    </select>
  );
  switch (s.step) {
    case "Set Status":
      return pick(t("Status"), STATUSES);
    case "Set Priority":
      return pick(t("Priority"), PRIORITIES);
    case "Set Team":
      return pick(t("Team"), (teams.data ?? []).map((x) => String(x.team_name)));
    case "Set Type":
      return pick(t("Type"), (types.data ?? []).map((x) => String(x.type_name)));
    case "Add Tag":
    case "Remove Tag":
      return (
        <>
          <input id={id} className="input" list={`${id}-tags`} aria-label={t("Tag")} value={v} onChange={(e) => onChange(e.target.value)} />
          <datalist id={`${id}-tags`}>
            {(tags.data ?? []).map((x) => (
              <option key={String(x.tag_name)} value={String(x.tag_name)} />
            ))}
          </datalist>
        </>
      );
    case "Assign To":
      return (
        <select id={id} className="select" aria-label={t("Agent")} value={v.toLowerCase()} onChange={(e) => onChange(e.target.value)}>
          <option value="">{t("Nobody (take the ticket off its agent)")}</option>
          {v && !names.people.some((p) => p.email.toLowerCase() === v.toLowerCase()) && <option value={v}>{isolate(v)}</option>}
          {names.people.map((p) => (
            <option key={p.id} value={p.email.toLowerCase()}>
              {p.full_name || p.email}
            </option>
          ))}
        </select>
      );
    case "Set Language":
      return <input id={id} className="input" aria-label={t("Language")} placeholder={t("en, hi, ar… empty clears it")} value={v} onChange={(e) => onChange(e.target.value)} />;
    default:
      return (
        <textarea id={id} className="textarea" rows={LONG.includes(s.step) ? 3 : 2} aria-label={t("Text")} value={v} onChange={(e) => onChange(e.target.value)} />
      );
  }
}

interface Draft {
  macro_name: string;
  scope: string;
  team: string;
  steps: MacroStep[];
}

function MacroDialog({ record, onClose, onSaved }: { record: Row | null; onClose: () => void; onSaved: () => void }) {
  const [draft, setDraft] = useState<Draft>({
    macro_name: String(record?.macro_name ?? ""),
    scope: String(record?.scope ?? "Personal"),
    team: String(record?.team ?? ""),
    steps: ((record?.steps as MacroStep[] | undefined) ?? []).map((s) => ({ step: s.step, value: s.value ?? "" })),
  });
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [shown, setShown] = useState(false);
  const [key] = useState(newKey);
  const set = (p: Partial<Draft>) => setDraft((d) => ({ ...d, ...p }));
  const setStep = (i: number, p: Partial<MacroStep>) => set({ steps: draft.steps.map((s, j) => (j === i ? { ...s, ...p } : s)) });
  const problems: Record<string, string> = {};
  if (!draft.macro_name.trim()) problems.name = t("This is needed.");
  if (draft.scope === "Team" && !draft.team) problems.team = t("A team's macro names the team.");
  const stepsBad = stepsProblem(draft.steps);
  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    setShown(true);
    if (busy || Object.keys(problems).length || stepsBad) return;
    setBusy(true);
    setRefusal(null);
    const body = { macro_name: draft.macro_name.trim(), scope: draft.scope, team: draft.scope === "Team" ? draft.team : null, steps: draft.steps.map((s) => ({ step: s.step, value: (s.value ?? "").trim() || null })) };
    try {
      if (record) await api.patch(docPath("SupportMacro", record.id), body, { ifMatch: Number(record.version) });
      else await api.post(docPath("SupportMacro"), body, { idempotencyKey: key });
      onSaved();
      onClose();
    } catch (err) {
      setRefusal(err instanceof ApiError && (err.status === 409 || err.status === 412) ? t("Someone else changed this macro while you had it open. Close it and open it again.") : errorMessage(err));
      setBusy(false);
    }
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={record ? t("Edit macro") : t("Add macro")}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-macro-form" loading={busy}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <form id="hd-macro-form" className="stack" onSubmit={(e) => void submit(e)} noValidate>
        {refusal && <Alert tone="error">{isolate(refusal)}</Alert>}
        <Field label={t("Name")} htmlFor="hd-macro-name" required error={shown ? problems.name : undefined}>
          <input id="hd-macro-name" className="input" maxLength={140} value={draft.macro_name} onChange={(e) => set({ macro_name: e.target.value })} />
        </Field>
        <Field label={t("Who can use it")} htmlFor="hd-macro-scope">
          <select id="hd-macro-scope" className="select" value={draft.scope} onChange={(e) => set({ scope: e.target.value })}>
            <option value="Personal">{t("Only me")}</option>
            <option value="Team">{t("A team")}</option>
            <option value="Everyone">{t("Every agent")}</option>
          </select>
        </Field>
        {draft.scope === "Team" && (
          <Field label={t("Team")} htmlFor="hd-macro-team" required error={shown ? problems.team : undefined}>
            <LinkInput to="SupportTeam" id="hd-macro-team" value={draft.team || null} title={record?._titles?.team} onChange={(v) => set({ team: v ?? "" })} />
          </Field>
        )}
        <fieldset className="hd-steps">
          <legend>{t("Steps, in the order they run")}</legend>
          {draft.steps.length === 0 && <p className="faint">{t("No steps yet.")}</p>}
          <ol>
            {draft.steps.map((s, i) => (
              <li key={i} className="hd-step-row">
                <select className="select" aria-label={t("Step {n}", { n: i + 1 })} value={s.step} onChange={(e) => setStep(i, { step: e.target.value, value: "" })}>
                  {STEP_KINDS.map((k) => (
                    <option key={k} value={k}>
                      {KIND_LABEL[k]()}
                    </option>
                  ))}
                </select>
                <Value s={s} id={`hd-step-${i}`} onChange={(v) => setStep(i, { value: v })} />
                <span className="hd-step-tools">
                  <button type="button" className="icon-btn icon-btn-sm" aria-label={t("Move step {n} up", { n: i + 1 })} disabled={i === 0} onClick={() => set({ steps: moved(draft.steps, i, i - 1) })}>
                    <Icon name="up" size={12} />
                  </button>
                  <button type="button" className="icon-btn icon-btn-sm" aria-label={t("Move step {n} down", { n: i + 1 })} disabled={i === draft.steps.length - 1} onClick={() => set({ steps: moved(draft.steps, i, i + 1) })}>
                    <Icon name="down" size={12} />
                  </button>
                  <button type="button" className="icon-btn icon-btn-sm" aria-label={t("Remove step {n}", { n: i + 1 })} onClick={() => set({ steps: draft.steps.filter((_, j) => j !== i) })}>
                    <Icon name="x" size={12} />
                  </button>
                </span>
              </li>
            ))}
          </ol>
          <Button size="sm" icon="plus" onClick={() => set({ steps: [...draft.steps, { step: "Set Status", value: "" }] })}>
            {t("Add a step")}
          </Button>
          {shown && stepsBad && (
            <p className="field-error" role="alert">
              {stepsBad}
            </p>
          )}
        </fieldset>
        {sendsReply(draft.steps) && <Alert tone="warning">{t("This macro sends the customer a reply each time it is applied.")}</Alert>}
        {draft.steps.length > 0 && (
          <div>
            <h3 className="hd-subhead">{t("In short")}</h3>
            <ol className="hd-effects">
              {draft.steps.map((s, i) => (
                <li key={i}>{isolate(stepLine(s))}</li>
              ))}
            </ol>
          </div>
        )}
      </form>
    </Dialog>
  );
}

export function MacrosAdmin() {
  const rights = useRights("SupportMacro");
  const refresh = useRefresh();
  const [editing, setEditing] = useState<Row | "new" | null>(null);
  const [opening, setOpening] = useState<string | null>(null);
  const columns: Column[] = [
    { key: "name", label: t("Macro"), cell: (r) => isolate(String(r.macro_name ?? "")) },
    { key: "scope", label: t("Who can use it"), cell: (r) => <Badge tone="neutral">{isolate(r.scope === "Personal" ? t("Only its author") : r.scope === "Team" ? (r._titles?.team ?? t("A team")) : t("Every agent"))}</Badge> },
    { key: "uses", label: t("Used"), cell: (r) => String(r.uses ?? 0) },
  ];
  /** The steps are on the macro's own record, not in the list. */
  const open = async (r: Row | "new") => {
    if (r === "new") return setEditing("new");
    setOpening(r.id);
    try {
      setEditing(await api.get<Row>(docPath("SupportMacro", r.id), { titles: true }));
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      setOpening(null);
    }
  };
  const done = () => {
    toast(t("Saved"));
    refresh("SupportMacro");
  };
  return (
    <div className="stack">
      <AdminTable
        entity="SupportMacro"
        noun={t("macro")}
        fields="macro_name,scope,team,uses"
        sort="macro_name asc"
        spec={[{ name: "macro_name", label: t("Macro"), kind: "text" }]}
        columns={columns}
        note={<p className="faint">{t("A macro changes a ticket in one go: every step applies, or none does, as the person who presses it. Agents apply one from the box under a ticket, or to several tickets at once from the inbox.")}</p>}
        empty={t("No macros yet.")}
        hideAdd
        hideEdit
        rowActions={(r) =>
          rights.write ? (
            <Button size="sm" loading={opening === r.id} onClick={() => void open(r)} aria-label={t("Edit {name}", { name: String(r.macro_name) })}>
              {t("Edit")}
            </Button>
          ) : null
        }
        headActions={
          rights.create ? (
            <Button variant="primary" icon="plus" onClick={() => void open("new")}>
              {t("Add macro")}
            </Button>
          ) : null
        }
      />
      {editing && <MacroDialog record={editing === "new" ? null : editing} onClose={() => setEditing(null)} onSaved={done} />}
    </div>
  );
}

export type { Macro };
