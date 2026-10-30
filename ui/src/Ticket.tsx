// A ticket as an agent works it: the conversation (the description first, as the requester wrote
// it), a reply or an internal note, and beside them its status, priority, assignment and SLA. The
// server stamps each new message's author and time and moves the status with the conversation (a
// first public reply marks an open ticket replied). Every field is in the full form too.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useCallback, useState } from "react";
import { Link, useParams } from "react-router";
import { api, ApiError, errorMessage } from "@ferrum/client";
import { Alert, Badge, Button, Dialog, EmptyState, ErrorState, Field, Markdown, PageLoading, statusTone, toast, useLiveEntity, useShellTitle } from "@ferrum/kit";
import { LinkTitle, useMe } from "@ferrum/records";
import { dueLabel, requester, type Ticket } from "./inbox";

interface Message {
  id?: string;
  author_kind?: string | null;
  author?: string | null;
  body?: string | null;
  is_internal?: boolean | null;
  sent_at?: string | null;
}

interface TicketDoc extends Ticket {
  version: number;
  description?: string | null;
  team?: string | null;
  customer?: string | null;
  resolution_details?: string | null;
  conversation?: Message[];
}

const STATUSES = ["Open", "Replied", "On Hold", "Resolved", "Closed"];
const PRIORITIES = ["Low", "Medium", "High", "Urgent"];
const when = (s?: string | null) => (s ? new Date(s).toLocaleString() : "");

function Thread({ t }: { t: TicketDoc }) {
  return (
    <ol className="hd-thread" aria-label="Conversation">
      <li className="hd-message hd-from-customer">
        <div className="hd-message-head">
          <strong>{requester(t) || "Requester"}</strong>
          <span className="faint">{when(t.opened_at)}</span>
        </div>
        {t.description ? <Markdown source={t.description} /> : <p className="muted">{t.subject}</p>}
      </li>
      {(t.conversation ?? []).map((m, i) => (
        <li key={m.id ?? i} className={`hd-message ${m.is_internal ? "hd-internal" : m.author_kind === "Customer" ? "hd-from-customer" : "hd-from-agent"}`}>
          <div className="hd-message-head">
            <strong>{m.author || m.author_kind}</strong>
            {m.is_internal && <Badge tone="amber">Internal note</Badge>}
            <span className="faint">{when(m.sent_at)}</span>
          </div>
          <Markdown source={m.body ?? ""} />
        </li>
      ))}
    </ol>
  );
}

/** A save with the version the page showed: someone else's change in between is not overwritten. */
function useSave(t: TicketDoc) {
  const qc = useQueryClient();
  return async (patch: Record<string, unknown>, done: string): Promise<boolean> => {
    try {
      const saved = await api.patch<TicketDoc>(`/api/v1/Ticket/${t.id}`, patch, { ifMatch: t.version });
      qc.setQueryData(["helpdesk", "ticket", t.id], saved);
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
      toast(done);
      return true;
    } catch (err) {
      if (err instanceof ApiError && err.status === 409) {
        void qc.invalidateQueries({ queryKey: ["helpdesk", "ticket", t.id] });
        toast("Someone else changed this ticket meanwhile; it shows their change now. Try again.", "error");
      } else {
        toast(errorMessage(err), "error");
      }
      return false;
    }
  };
}

function Composer({ t, save }: { t: TicketDoc; save: ReturnType<typeof useSave> }) {
  const [body, setBody] = useState("");
  const [internal, setInternal] = useState(false);
  const [busy, setBusy] = useState(false);
  const send = async (e: FormEvent) => {
    e.preventDefault();
    if (!body.trim()) return;
    setBusy(true);
    // Rows the ticket has keep their ids (unchanged); the new one has none.
    const conversation = [...(t.conversation ?? []).map((m) => ({ id: m.id })), { author_kind: "Agent", body: body.trim(), is_internal: internal }];
    if (await save({ conversation }, internal ? "Note added" : "Reply added")) setBody("");
    setBusy(false);
  };
  return (
    <form className="hd-composer card" onSubmit={send}>
      <div className="card-body stack">
        <Field label={internal ? "Internal note (agents only)" : "Reply"} htmlFor="hd-reply">
          <textarea id="hd-reply" className="input" rows={5} value={body} onChange={(e) => setBody(e.target.value)} placeholder="Markdown works here" />
        </Field>
        <div className="row hd-composer-foot">
          <label className="row">
            <input type="checkbox" checked={internal} onChange={(e) => setInternal(e.target.checked)} /> Internal note
          </label>
          <Button type="submit" variant="primary" loading={busy} disabled={!body.trim()}>
            {internal ? "Add note" : "Add reply"}
          </Button>
        </div>
      </div>
    </form>
  );
}

/** Resolving or closing asks for the resolution, which the ticket requires then. */
function StatusControl({ t, save }: { t: TicketDoc; save: ReturnType<typeof useSave> }) {
  const [pending, setPending] = useState<string | null>(null);
  const [resolution, setResolution] = useState("");
  const change = (status: string) => {
    if ((status === "Resolved" || status === "Closed") && !t.resolution_details) {
      setResolution("");
      setPending(status);
      return;
    }
    void save({ status }, `Status: ${status}`);
  };
  return (
    <>
      <Field label="Status" htmlFor="hd-status">
        <select id="hd-status" className="input" value={t.status ?? "Open"} onChange={(e) => change(e.target.value)}>
          {STATUSES.map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
      </Field>
      <Dialog
        open={!!pending}
        onClose={() => setPending(null)}
        title={`${pending === "Closed" ? "Close" : "Resolve"} the ticket`}
        footer={
          <>
            <Button onClick={() => setPending(null)}>Cancel</Button>
            <Button
              variant="primary"
              disabled={!resolution.trim()}
              onClick={async () => {
                if (pending && (await save({ status: pending, resolution_details: resolution.trim() }, `Status: ${pending}`))) setPending(null);
              }}
            >
              {pending === "Closed" ? "Close" : "Resolve"}
            </Button>
          </>
        }
      >
        <Field label="How it was resolved" htmlFor="hd-resolution" required>
          <textarea id="hd-resolution" className="input" rows={4} value={resolution} onChange={(e) => setResolution(e.target.value)} />
        </Field>
      </Dialog>
    </>
  );
}

function Details({ t, save, me }: { t: TicketDoc; save: ReturnType<typeof useSave>; me?: string }) {
  const responseDue = !t.first_responded_at && dueLabel(t.response_by);
  const resolutionDue = !["Resolved", "Closed"].includes(t.status ?? "") && dueLabel(t.resolution_by);
  return (
    <aside className="hd-details card" aria-label="Ticket details">
      <div className="card-body stack">
        <StatusControl t={t} save={save} />
        <Field label="Priority" htmlFor="hd-priority">
          <select id="hd-priority" className="input" value={t.priority ?? "Medium"} onChange={(e) => void save({ priority: e.target.value }, `Priority: ${e.target.value}`)}>
            {PRIORITIES.map((p) => (
              <option key={p}>{p}</option>
            ))}
          </select>
        </Field>
        <div className="hd-detail">
          <span className="hd-detail-label">Assigned to</span>
          <span>{t.assigned_to || <span className="muted">Nobody</span>}</span>
          {me && t.assigned_to !== me && (
            <Button size="sm" onClick={() => void save({ assigned_to: me }, "Assigned to you")}>
              Assign to me
            </Button>
          )}
        </div>
        {t.team && (
          <div className="hd-detail">
            <span className="hd-detail-label">Team</span>
            <LinkTitle to="SupportTeam" id={t.team} />
          </div>
        )}
        <div className="hd-detail">
          <span className="hd-detail-label">From</span>
          {t.customer ? <LinkTitle to="Customer" id={t.customer} title={t.customer_name ?? undefined} /> : <span>{requester(t) || "—"}</span>}
          {t.contact_email && <a href={`mailto:${t.contact_email}`}>{t.contact_email}</a>}
        </div>
        {(t.sla_status || responseDue || resolutionDue) && (
          <div className="hd-detail">
            <span className="hd-detail-label">Service level</span>
            {t.sla_status && <Badge tone={t.sla_status === "Breached" ? "red" : t.sla_status === "At Risk" ? "amber" : "green"}>{t.sla_status}</Badge>}
            {responseDue && <span className={responseDue.startsWith("overdue") ? "hd-overdue" : "faint"}>First response {responseDue}</span>}
            {resolutionDue && <span className={resolutionDue.startsWith("overdue") ? "hd-overdue" : "faint"}>Resolution {resolutionDue}</span>}
          </div>
        )}
        <Link to={`/e/Ticket/${t.id}`}>Every field, in the form</Link>
      </div>
    </aside>
  );
}

export function TicketPage() {
  const { id } = useParams();
  const me = useMe().data?.user.email;
  const qc = useQueryClient();
  const q = useQuery({
    queryKey: ["helpdesk", "ticket", id],
    queryFn: ({ signal }) => api.get<TicketDoc>(`/api/v1/Ticket/${id}`, undefined, signal),
    enabled: !!id,
  });
  useShellTitle(q.data ? `${q.data.number ?? ""} ${q.data.subject ?? ""}`.trim() : null);
  useLiveEntity(
    "Ticket",
    useCallback(
      (e: { id?: string }) => {
        if (e.id === id) void qc.invalidateQueries({ queryKey: ["helpdesk", "ticket", id] });
      },
      [qc, id],
    ),
  );
  if (q.error) {
    if (q.error instanceof ApiError && q.error.status === 404) {
      return (
        <EmptyState title="This ticket no longer exists">
          <Link to="/">Back to the inbox</Link>
        </EmptyState>
      );
    }
    return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
  }
  if (!q.data) return <PageLoading label="Loading the ticket" />;
  return <TicketBody t={q.data} me={me} />;
}

function TicketBody({ t, me }: { t: TicketDoc; me?: string }) {
  const save = useSave(t);
  const closed = t.status === "Closed";
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <p className="page-sub">
            <Link to="/">Inbox</Link> · {t.number}
          </p>
          <h1 className="page-title">{t.subject || "No subject"}</h1>
          <p className="row">
            <Badge tone={statusTone(t.status)}>{t.status}</Badge>
            {t.priority && <Badge tone={t.priority === "Urgent" ? "red" : t.priority === "High" ? "amber" : "neutral"}>{t.priority}</Badge>}
          </p>
        </div>
      </div>
      <div className="hd-ticket-layout">
        <div className="stack">
          <Thread t={t} />
          {closed ? <Alert tone="info">This ticket is closed. Reopen it to reply.</Alert> : <Composer t={t} save={save} />}
        </div>
        <Details t={t} save={save} me={me} />
      </div>
    </div>
  );
}
