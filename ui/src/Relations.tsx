// What a ticket is connected to (Helpdesk 0.18): its parent, its children and the tickets linked to it either way, with
// the ways to change that. Everything is the server's: a parent with open children, a child under a resolved parent, a
// link to itself are refused there and shown here in its words.

import { type FormEvent, useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { Link } from "react-router";
import { api, docPath, isolate, newKey, t as tr } from "@ferrum/client";
import { Alert, Badge, Button, Dialog, Field, Spinner, toast } from "@ferrum/kit";
import { LinkInput } from "@ferrum/records";
import { refusedBy } from "./Merge";
import { STATUS_TONES } from "./tones";

export interface LinkRow {
  id: string;
  ticket: string;
  _titles?: Record<string, string>;
}

export interface RelatedTicket {
  id: string;
  version?: number;
  number?: string | null;
  subject?: string | null;
  status?: string | null;
  links?: LinkRow[];
}

/** What the card needs of the open ticket. */
export interface RelationsTicket {
  id: string;
  parent_ticket?: string | null;
  links?: LinkRow[] | null;
  team?: string | null;
  customer?: string | null;
  contact_name?: string | null;
  contact_email?: string | null;
  priority?: string | null;
  channel?: string | null;
  _titles?: Record<string, string>;
}

const FIELDS = "number,subject,status";
const list = (filters: unknown[], fields = FIELDS, limit = 50, signal?: AbortSignal) =>
  api.get<{ rows: RelatedTicket[] }>(docPath("Ticket"), { filters: JSON.stringify(filters), fields, sort: "opened_at asc", limit }, signal).then((r) => r.rows);

/** The three lists, as the card reads them (keys exported for tests that seed the cache). */
export const relationKeys = {
  children: (id: string) => ["helpdesk", "children", id],
  linkedHere: (id: string) => ["helpdesk", "linked-here", id],
  linked: (ids: string[]) => ["helpdesk", "linked", ids.join(",")],
  parent: (id: string) => ["helpdesk", "parent", id],
};

function Line({ x, onRemove }: { x: RelatedTicket; onRemove?: () => void }) {
  return (
    <li>
      <Link to={`/t/${x.id}`}>
        <span className="hd-others-subject">{isolate([x.number, x.subject].filter(Boolean).join(" · "))}</span>
      </Link>
      <Badge tone={STATUS_TONES[x.status ?? ""] ?? "neutral"}>{isolate(x.status ?? "")}</Badge>
      {onRemove && (
        <Button size="sm" onClick={onRemove}>
          {tr("Remove")}
        </Button>
      )}
    </li>
  );
}

export function Relations({ t, editable, save }: { t: RelationsTicket; editable: boolean; save: (field: string, value: unknown) => Promise<void> }) {
  const qc = useQueryClient();
  const rows = t.links ?? [];
  const ids = rows.map((r) => r.ticket);
  const children = useQuery({ queryKey: relationKeys.children(t.id), queryFn: ({ signal }) => list([["parent_ticket", "=", t.id]], FIELDS, 50, signal) });
  const here = useQuery({ queryKey: relationKeys.linkedHere(t.id), queryFn: ({ signal }) => list([["links.ticket", "=", t.id]], `${FIELDS},links`, 50, signal) });
  const linked = useQuery({ queryKey: relationKeys.linked(ids), enabled: ids.length > 0, queryFn: ({ signal }) => list([["id", "in", ids]], FIELDS, 50, signal) });
  const parent = useQuery({
    queryKey: relationKeys.parent(t.parent_ticket ?? ""),
    enabled: !!t.parent_ticket,
    queryFn: ({ signal }) => list([["id", "=", t.parent_ticket]], FIELDS, 1, signal).then((r) => r[0] ?? null),
  });
  const [dialog, setDialog] = useState<"child" | "link" | "parent" | null>(null);
  const refresh = () => {
    for (const k of [relationKeys.children(t.id), relationKeys.linkedHere(t.id), ["helpdesk", "linked"], ["helpdesk", "parent"]]) void qc.invalidateQueries({ queryKey: k });
  };

  const hasChildren = !!children.data?.length;
  const nothing = !t.parent_ticket && !rows.length && children.isSuccess && !hasChildren && here.isSuccess && !here.data.length;
  if (nothing && !editable) return null;
  const failed = children.isError || here.isError || linked.isError || parent.isError;
  const loading = children.isLoading || here.isLoading || linked.isLoading || parent.isLoading;
  const mine = (linked.data ?? []).map((x) => ({ x, row: rows.find((r) => r.ticket === x.id) }));

  const unlinkHere = async (row: LinkRow) => save("links", rows.filter((r) => r.id !== row.id).map((r) => ({ id: r.id })));
  const unlinkThere = async (other: RelatedTicket) => {
    try {
      const fresh = await api.get<RelatedTicket>(docPath("Ticket", other.id));
      const keep = (fresh.links ?? []).filter((r) => r.ticket !== t.id).map((r) => ({ id: r.id }));
      await api.patch(docPath("Ticket", other.id), { links: keep }, { ifMatch: Number(fresh.version) });
      refresh();
    } catch (e) {
      toast(refusedBy(e), "error");
    }
  };

  return (
    <section className="hd-card" aria-labelledby="hd-rail-related">
      <h3 id="hd-rail-related">{tr("Related tickets")}</h3>
      {failed ? (
        <p className="faint">{tr("They could not be loaded.")}</p>
      ) : loading ? (
        <Spinner label={tr("Looking for related tickets")} />
      ) : (
        <>
          {t.parent_ticket && (
            <div className="hd-others">
              <h4 id="hd-rel-parent">{tr("Parent")}</h4>
              <ul aria-labelledby="hd-rel-parent">
                <Line x={parent.data ?? { id: t.parent_ticket, subject: t._titles?.parent_ticket ?? tr("another ticket") }} />
              </ul>
            </div>
          )}
          {hasChildren && (
            <div className="hd-others">
              <h4 id="hd-rel-children">{tr("Child tickets")}</h4>
              <ul aria-labelledby="hd-rel-children">
                {children.data?.map((x) => <Line key={x.id} x={x} />)}
              </ul>
            </div>
          )}
          {(mine.length > 0 || !!here.data?.length) && (
            <div className="hd-others">
              <h4 id="hd-rel-linked">{tr("Linked")}</h4>
              <ul aria-labelledby="hd-rel-linked">
                {mine.map(({ x, row }) => (
                  <Line key={`m-${x.id}`} x={x} onRemove={editable && row ? () => void unlinkHere(row) : undefined} />
                ))}
                {here.data?.filter((x) => !ids.includes(x.id)).map((x) => <Line key={`h-${x.id}`} x={x} onRemove={editable ? () => void unlinkThere(x) : undefined} />)}
              </ul>
            </div>
          )}
          {nothing && <p className="faint">{tr("No related tickets.")}</p>}
        </>
      )}
      {editable && (
        <div className="row">
          {!t.parent_ticket && <Button size="sm" onClick={() => setDialog("child")}>{tr("New child ticket")}</Button>}
          <Button size="sm" onClick={() => setDialog("link")}>{tr("Link a ticket")}</Button>
          {!t.parent_ticket && !hasChildren && <Button size="sm" onClick={() => setDialog("parent")}>{tr("Move under a parent")}</Button>}
          {t.parent_ticket && <Button size="sm" onClick={() => void save("parent_ticket", null).then(refresh)}>{tr("Remove from its parent")}</Button>}
        </div>
      )}
      <ChildDialog open={dialog === "child"} t={t} onClose={() => setDialog(null)} onDone={refresh} />
      <NumberDialog
        open={dialog === "link" || dialog === "parent"}
        title={dialog === "parent" ? tr("Move under a parent") : tr("Link a ticket")}
        action={dialog === "parent" ? tr("Move it") : tr("Link it")}
        self={t.id}
        onClose={() => setDialog(null)}
        onPick={async (found) => {
          if (dialog === "parent") await save("parent_ticket", found.id);
          else await save("links", [...rows.map((r) => ({ id: r.id })), { ticket: found.id }]);
          refresh();
        }}
      />
    </section>
  );
}

function ChildDialog({ open, t, onClose, onDone }: { open: boolean; t: RelationsTicket; onClose: () => void; onDone: () => void }) {
  const [subject, setSubject] = useState("");
  const [description, setDescription] = useState("");
  const [team, setTeam] = useState<string | null>(t.team ?? null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [key, setKey] = useState(newKey);
  useEffect(() => {
    if (!open) return;
    setSubject("");
    setDescription("");
    setTeam(t.team ?? null);
    setProblem(null);
    setKey(newKey());
  }, [open, t.team]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!subject.trim() || busy) return;
    setBusy(true);
    setProblem(null);
    try {
      const body = { subject: subject.trim(), description: description.trim() || null, team, parent_ticket: t.id, customer: t.customer ?? null, contact_name: t.contact_name ?? null, contact_email: t.contact_email ?? null, priority: t.priority ?? undefined, channel: t.channel ?? undefined };
      const made = await api.post<RelatedTicket>(docPath("Ticket"), body, { idempotencyKey: key });
      toast(tr("Child ticket {number} made", { number: made.number ?? "" }));
      onDone();
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
      title={tr("New child ticket")}
      footer={
        <>
          <Button onClick={onClose}>{tr("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-child" loading={busy} disabled={!subject.trim()}>
            {tr("Make it")}
          </Button>
        </>
      }
    >
      <form id="hd-child" className="stack" onSubmit={submit}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        <Field label={tr("Subject")} htmlFor="hd-child-subject" required>
          <input id="hd-child-subject" className="input" maxLength={300} value={subject} onChange={(e) => setSubject(e.target.value)} />
        </Field>
        <Field label={tr("Description")} htmlFor="hd-child-description">
          <textarea id="hd-child-description" className="textarea" rows={4} value={description} onChange={(e) => setDescription(e.target.value)} />
        </Field>
        <Field label={tr("Team")} htmlFor="hd-child-team">
          <LinkInput to="SupportTeam" id="hd-child-team" value={team} title={t._titles?.team} onChange={(v) => setTeam(v)} />
        </Field>
      </form>
    </Dialog>
  );
}

function NumberDialog({ open, title, action, self, onClose, onPick }: { open: boolean; title: string; action: string; self: string; onClose: () => void; onPick: (found: RelatedTicket) => Promise<void> }) {
  const [typed, setTyped] = useState("");
  const [found, setFound] = useState<RelatedTicket | null | undefined>(undefined);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!open) return;
    setTyped("");
    setFound(undefined);
    setProblem(null);
  }, [open]);
  const look = async () => {
    const n = typed.trim();
    if (!n) return;
    setProblem(null);
    try {
      const r = await list([["number", "=", n]], FIELDS, 1);
      setFound(r[0] && r[0].id !== self ? r[0] : null);
    } catch (e) {
      setProblem(refusedBy(e));
    }
  };
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!found) {
      void look();
      return;
    }
    if (busy) return;
    setBusy(true);
    setProblem(null);
    try {
      await onPick(found);
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
      title={title}
      footer={
        <>
          <Button onClick={onClose}>{tr("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-number" loading={busy} disabled={!found}>
            {action}
          </Button>
        </>
      }
    >
      <form id="hd-number" className="stack" onSubmit={submit}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        <Field label={tr("Ticket number")} htmlFor="hd-number-input">
          <input
            id="hd-number-input"
            className="input"
            value={typed}
            onChange={(e) => {
              setTyped(e.target.value);
              setFound(undefined);
            }}
            onBlur={() => void look()}
          />
        </Field>
        {found === null && <p className="faint">{tr("No ticket has that number, or you may not see it.")}</p>}
        {found && (
          <p>
            {isolate([found.number, found.subject].filter(Boolean).join(" · "))} <Badge tone={STATUS_TONES[found.status ?? ""] ?? "neutral"}>{isolate(found.status ?? "")}</Badge>
          </p>
        )}
      </form>
    </Dialog>
  );
}

/** Splitting a customer's message off into a ticket of its own (`split_message`): the message stays, its files stay here. */
export function SplitDialog({ ticket, message, onClose, onDone }: { ticket: string; message: string | null; onClose: () => void; onDone: (made: { id: string; number?: string | null }) => void }) {
  const [subject, setSubject] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    if (!message) return;
    setSubject("");
    setProblem(null);
  }, [message]);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!message || busy) return;
    setBusy(true);
    setProblem(null);
    try {
      const made = await api.post<{ id: string; number?: string | null }>(docPath("Ticket", ticket, "actions", "split_message"), { message, subject: subject.trim() || null });
      onDone(made);
      onClose();
    } catch (err) {
      setProblem(refusedBy(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={!!message}
      onClose={onClose}
      title={tr("Split into a new ticket")}
      footer={
        <>
          <Button onClick={onClose}>{tr("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-split" loading={busy}>
            {tr("Make the ticket")}
          </Button>
        </>
      }
    >
      <form id="hd-split" className="stack" onSubmit={submit}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        <p>{tr("A new ticket is made from this message for the same customer. The message stays here too, and its files stay on this ticket.")}</p>
        <Field label={tr("Subject")} htmlFor="hd-split-subject" hint={tr("Empty: the start of the message.")}>
          <input id="hd-split-subject" className="input" maxLength={300} value={subject} onChange={(e) => setSubject(e.target.value)} />
        </Field>
      </form>
    </Dialog>
  );
}
