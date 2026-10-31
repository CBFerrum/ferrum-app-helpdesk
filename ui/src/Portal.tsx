// Helpdesk's page of the workspace's portal (Ferrum docs/plan/apps-ui.md §9.1) at /portal/tickets/:
// the tickets a customer raised, each one's conversation with the team (agents' internal notes
// never come here), a reply, a new ticket, and a rating once one is resolved. Everything comes
// from /api/app/helpdesk/portal/…, which answers by the address the person signed in with.

import { useInfiniteQuery, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api, errorMessage } from "@ferrum/client";
import { Badge, Button, EmptyState, ErrorState, Field, Markdown, PageLoading, statusTone, toast, useShellTitle } from "@ferrum/kit";

const BASE = "/api/app/helpdesk/portal/tickets";

export interface PortalTicketSummary {
  id: string;
  number?: string | null;
  subject?: string | null;
  status?: string | null;
  label?: string | null;
  priority?: string | null;
  opened_at?: string | null;
  updated_at?: string | null;
}

export interface PortalMessage {
  id?: string | null;
  mine: boolean;
  author: string;
  body?: string | null;
  sent_at?: string | null;
}

export interface PortalTicketDetail extends PortalTicketSummary {
  description?: string | null;
  messages: PortalMessage[];
  resolution?: string | null;
  rating?: number | null;
  feedback?: string | null;
  can_reply: boolean;
  can_rate: boolean;
}

const when = (s?: string | null) => (s ? new Date(s).toLocaleString() : "");
const day = (s?: string | null) => (s ? new Date(s).toLocaleDateString() : "");

export function PortalTickets() {
  useShellTitle("Your tickets");
  const q = useInfiniteQuery({
    queryKey: ["helpdesk", "portal", "tickets"],
    queryFn: ({ pageParam, signal }) =>
      api.get<{ tickets: PortalTicketSummary[]; next_cursor?: string | null }>(BASE, pageParam ? { cursor: pageParam } : {}, signal),
    initialPageParam: "",
    getNextPageParam: (last) => last.next_cursor || undefined,
  });
  const tickets = q.data?.pages.flatMap((p) => p.tickets) ?? [];
  return (
    <div className="stack hd-portal">
      <div className="page-head">
        <div>
          <h1 className="page-title">Your tickets</h1>
          <p className="page-sub">Ask for help, and follow what the team says.</p>
        </div>
        <Link className="btn btn-primary btn-md" to="/new">
          New ticket
        </Link>
      </div>
      {q.error ? (
        <ErrorState error={q.error} onRetry={() => void q.refetch()} />
      ) : !q.data ? (
        <PageLoading />
      ) : !tickets.length ? (
        <EmptyState title="No tickets yet" icon="headset">
          <p className="muted">When you ask for help, your request and the team's answers are here.</p>
          <Link to="/new">Raise a ticket</Link>
        </EmptyState>
      ) : (
        <ul className="hd-tickets" data-ferrum-nocard>
          {tickets.map((t) => (
            <li key={t.id}>
              <Link className="hd-ticket" to={`/t/${t.id}`}>
                <span className="hd-ticket-main">
                  <span className="hd-ticket-subject">{t.subject}</span>
                  <span className="faint">
                    {t.number} · opened {day(t.opened_at)}
                  </span>
                </span>
                <span className="hd-ticket-tags">
                  <Badge tone={statusTone(t.status)}>{t.label ?? t.status}</Badge>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      )}
      {q.hasNextPage && (
        <Button loading={q.isFetchingNextPage} onClick={() => void q.fetchNextPage()}>
          Show older tickets
        </Button>
      )}
    </div>
  );
}

export function PortalNewTicket() {
  useShellTitle("New ticket");
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [subject, setSubject] = useState("");
  const [description, setDescription] = useState("");
  const [priority, setPriority] = useState("Medium");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      const t = await api.post<PortalTicketDetail>(BASE, { subject: subject.trim(), description: description.trim(), priority });
      qc.setQueryData(["helpdesk", "portal", "ticket", t.id], t);
      void qc.invalidateQueries({ queryKey: ["helpdesk", "portal", "tickets"] });
      toast(`Ticket ${t.number ?? ""} raised`);
      navigate(`/t/${t.id}`);
    } catch (err) {
      toast(errorMessage(err), "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="stack hd-portal" onSubmit={submit}>
      <p className="crumbs">
        <Link to="/">Your tickets</Link>
      </p>
      <h1 className="page-title">New ticket</h1>
      <div className="card">
        <div className="card-body stack">
          <Field label="Subject" htmlFor="hd-subject">
            <input id="hd-subject" className="input" required maxLength={300} value={subject} onChange={(e) => setSubject(e.target.value)} />
          </Field>
          <Field label="What happened?" htmlFor="hd-description">
            <textarea id="hd-description" className="input" required rows={7} maxLength={20000} value={description} onChange={(e) => setDescription(e.target.value)} />
          </Field>
          <Field label="How urgent is it?" htmlFor="hd-priority">
            <select id="hd-priority" className="input" value={priority} onChange={(e) => setPriority(e.target.value)}>
              <option>Low</option>
              <option>Medium</option>
              <option>High</option>
            </select>
          </Field>
          <div className="row">
            <Button type="submit" variant="primary" loading={busy} disabled={!subject.trim() || !description.trim()}>
              Send
            </Button>
          </div>
        </div>
      </div>
    </form>
  );
}

function Rating({ t, onSaved }: { t: PortalTicketDetail; onSaved: (t: PortalTicketDetail) => void }) {
  const [rating, setRating] = useState(0);
  const [feedback, setFeedback] = useState("");
  const [busy, setBusy] = useState(false);
  const send = async (e: FormEvent) => {
    e.preventDefault();
    setBusy(true);
    try {
      onSaved(await api.post<PortalTicketDetail>(`${BASE}/${t.id}/rating`, { rating, feedback: feedback.trim() || null }));
      toast("Thank you for rating the help");
    } catch (err) {
      toast(errorMessage(err), "error");
    } finally {
      setBusy(false);
    }
  };
  return (
    <form className="card" onSubmit={send}>
      <div className="card-body stack">
        <fieldset className="hd-stars">
          <legend>How was the help?</legend>
          {[1, 2, 3, 4, 5].map((n) => (
            <label key={n} title={`${n} of 5`}>
              <input type="radio" name="rating" value={n} checked={rating === n} onChange={() => setRating(n)} />
              <span aria-hidden="true">{n <= rating ? "★" : "☆"}</span>
              <span className="sr-only">{n} of 5</span>
            </label>
          ))}
        </fieldset>
        <Field label="Anything to add? (optional)" htmlFor="hd-feedback">
          <textarea id="hd-feedback" className="input" rows={3} maxLength={2000} value={feedback} onChange={(e) => setFeedback(e.target.value)} />
        </Field>
        <div className="row">
          <Button type="submit" variant="primary" loading={busy} disabled={!rating}>
            Send rating
          </Button>
        </div>
      </div>
    </form>
  );
}

export function PortalTicket() {
  const { id = "" } = useParams();
  const qc = useQueryClient();
  const key = ["helpdesk", "portal", "ticket", id];
  const q = useQuery({ queryKey: key, queryFn: ({ signal }) => api.get<PortalTicketDetail>(`${BASE}/${id}`, {}, signal) });
  const [body, setBody] = useState("");
  const [busy, setBusy] = useState(false);
  useShellTitle(q.data?.subject ?? "Ticket");
  const saved = (t: PortalTicketDetail) => {
    qc.setQueryData(key, t);
    void qc.invalidateQueries({ queryKey: ["helpdesk", "portal", "tickets"] });
  };
  const send = async (e: FormEvent) => {
    e.preventDefault();
    if (!body.trim()) return;
    setBusy(true);
    try {
      saved(await api.post<PortalTicketDetail>(`${BASE}/${id}/replies`, { body: body.trim() }));
      setBody("");
      toast("Reply sent");
    } catch (err) {
      toast(errorMessage(err), "error");
    } finally {
      setBusy(false);
    }
  };
  if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
  if (!q.data) return <PageLoading />;
  const t = q.data;
  return (
    <div className="stack hd-portal">
      <p className="crumbs">
        <Link to="/">Your tickets</Link>
      </p>
      <div className="page-head">
        <div>
          <h1 className="page-title">{t.subject}</h1>
          <p className="page-sub">
            {t.number} · opened {when(t.opened_at)}
          </p>
        </div>
        <Badge tone={statusTone(t.status)}>{t.label ?? t.status}</Badge>
      </div>
      <ol className="hd-thread" aria-label="Conversation">
        <li className="hd-message hd-from-customer">
          <div className="hd-message-head">
            <strong>You</strong>
            <span className="faint">{when(t.opened_at)}</span>
          </div>
          {t.description ? <Markdown source={t.description} /> : <p className="muted">{t.subject}</p>}
        </li>
        {t.messages.map((m, i) => (
          <li key={m.id ?? i} className={`hd-message ${m.mine ? "hd-from-customer" : "hd-from-agent"}`}>
            <div className="hd-message-head">
              <strong>{m.author}</strong>
              <span className="faint">{when(m.sent_at)}</span>
            </div>
            <Markdown source={m.body ?? ""} />
          </li>
        ))}
      </ol>
      {t.resolution && (
        <section className="card" aria-labelledby="hd-resolution">
          <div className="card-head">
            <h2 id="hd-resolution">How it was resolved</h2>
          </div>
          <div className="card-body">
            <Markdown source={t.resolution} />
          </div>
        </section>
      )}
      {t.can_rate && <Rating t={t} onSaved={saved} />}
      {t.rating ? <p className="muted">You rated the help {t.rating} of 5.</p> : null}
      {t.can_reply ? (
        <form className="hd-composer card" onSubmit={send}>
          <div className="card-body stack">
            <Field label={t.status === "Resolved" ? "Still need help? Reply to reopen it" : "Reply"} htmlFor="hd-reply">
              <textarea id="hd-reply" className="input" rows={5} maxLength={20000} value={body} onChange={(e) => setBody(e.target.value)} />
            </Field>
            <div className="row hd-composer-foot">
              <Button type="submit" variant="primary" loading={busy} disabled={!body.trim()}>
                Send reply
              </Button>
            </div>
          </div>
        </form>
      ) : (
        <p className="muted">This ticket is closed. If you need more help, raise a new ticket.</p>
      )}
    </div>
  );
}
