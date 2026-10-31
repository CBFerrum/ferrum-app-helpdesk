// Helpdesk's home, the agent inbox: tickets that still need someone, as the agent's own, the
// unassigned, those whose SLA is at risk or breached, and every open one; the most pressing first.
// A ticket opens in the ticket view (./Ticket.tsx); the rest of Helpdesk's records in the frame.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useState } from "react";
import { Link } from "react-router";
import { api } from "@ferrum/client";
import { Badge, Button, EmptyState, ErrorState, Spinner, statusTone, TabPanel, Tabs, useLiveEntity, useShellTitle } from "@ferrum/kit";
import { can, useMe, useMetaIndex } from "@ferrum/records";
import { ACTIVE, byUrgency, dueAt, dueLabel, requester, SLA_ALERTS, type Ticket } from "./inbox";

const FIELDS = "number,subject,status,priority,customer_name,contact_name,contact_email,assigned_to,sla_status,response_by,resolution_by,first_responded_at,opened_at";
/** How many of each queue the page shows; the count says how many there are. */
const SHOWN = 100;

type Queue = "mine" | "unassigned" | "alerts" | "open";

function filters(queue: Queue, me: string): unknown[] {
  const active = ["status", "in", ACTIVE];
  switch (queue) {
    case "mine":
      return [active, ["assigned_to", "=", me]];
    case "unassigned":
      return [active, ["assigned_to", "is", "not set"]];
    case "alerts":
      return [active, ["sla_status", "in", SLA_ALERTS]];
    case "open":
      return [active];
  }
}

function useQueue(queue: Queue, me: string | undefined) {
  return useQuery({
    queryKey: ["helpdesk", "queue", queue, me],
    enabled: !!me,
    queryFn: ({ signal }) =>
      api
        .get<{ rows: Ticket[]; total?: number }>(
          "/api/v1/Ticket",
          { filters: JSON.stringify(filters(queue, me!)), fields: FIELDS, sort: "opened_at asc", limit: SHOWN, count: "true" },
          signal,
        )
        .then((r) => ({ rows: [...r.rows].sort(byUrgency), total: r.total ?? r.rows.length })),
  });
}

const PRIORITY_TONE: Record<string, "red" | "amber" | "neutral"> = { Urgent: "red", High: "amber" };
const SLA_TONE: Record<string, "red" | "amber" | "green"> = { Breached: "red", "At Risk": "amber", Fulfilled: "green" };

function TicketRow({ t }: { t: Ticket }) {
  const due = dueLabel(dueAt(t));
  return (
    <li>
      <Link className="hd-ticket" to={`t/${t.id}`}>
        <span className="hd-ticket-main">
          <span className="hd-ticket-subject">{t.subject || "No subject"}</span>
          <span className="faint">
            {[t.number, requester(t), t.assigned_to ? `assigned to ${t.assigned_to}` : "unassigned"].filter(Boolean).join(" · ")}
          </span>
        </span>
        <span className="hd-ticket-tags">
          {t.priority && t.priority !== "Medium" && <Badge tone={PRIORITY_TONE[t.priority] ?? "neutral"}>{t.priority}</Badge>}
          <Badge tone={statusTone(t.status)}>{t.status}</Badge>
          {t.sla_status && t.sla_status !== "On Track" && <Badge tone={SLA_TONE[t.sla_status] ?? "neutral"}>{t.sla_status}</Badge>}
          {due && <span className={due.startsWith("overdue") ? "hd-overdue" : "faint"}>{due}</span>}
        </span>
      </Link>
    </li>
  );
}

const EMPTY: Record<Queue, string> = {
  mine: "Nothing assigned to you is open. Well done.",
  unassigned: "Every open ticket has someone on it.",
  alerts: "No ticket is at risk of missing its SLA.",
  open: "No open tickets.",
};

function QueueList({ queue, q }: { queue: Queue; q: ReturnType<typeof useQueue> }) {
  if (q.error) return <ErrorState error={q.error} onRetry={() => void q.refetch()} />;
  if (!q.data) return <Spinner label="Loading tickets" />;
  if (!q.data.rows.length) return <EmptyState title={EMPTY[queue]} icon="check" />;
  return (
    <>
      <ul className="hd-tickets" data-ferrum-nocard>
        {q.data.rows.map((t) => (
          <TicketRow key={t.id} t={t} />
        ))}
      </ul>
      {q.data.total > q.data.rows.length && (
        <p className="muted">
          Showing the {q.data.rows.length} oldest of {q.data.total}, most pressing first.{" "}
          <Link to={`e/Ticket?filters=${encodeURIComponent(JSON.stringify(filters(queue, "")))}`}>See them all</Link>
        </p>
      )}
    </>
  );
}

export function HelpdeskHome() {
  useShellTitle("Inbox");
  const me = useMe().data?.user.email;
  const meta = useMetaIndex().data;
  const ticket = meta?.entities.find((e) => e.name === "Ticket");
  const [queue, setQueue] = useState<Queue>("mine");
  const queues = { mine: useQueue("mine", me), unassigned: useQueue("unassigned", me), alerts: useQueue("alerts", me), open: useQueue("open", me) };
  // Someone else's change to a ticket shows at once.
  const qc = useQueryClient();
  useLiveEntity(
    "Ticket",
    useCallback(() => void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] }), [qc]),
  );
  if (meta && !(ticket && can(ticket.permissions, "read"))) {
    return (
      <EmptyState title="No tickets for you" icon="alert">
        <p className="muted">Ask an administrator for a Support role.</p>
      </EmptyState>
    );
  }
  const count = (k: Queue) => (queues[k].data ? ` (${queues[k].data.total})` : "");
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1 className="page-title">Inbox</h1>
          <p className="page-sub">Tickets that still need someone, the most pressing first.</p>
        </div>
        {ticket && can(ticket.permissions, "create") && (
          <div className="page-actions">
            <Link to="e/Ticket/new">
              <Button variant="primary" icon="plus">
                New ticket
              </Button>
            </Link>
          </div>
        )}
      </div>
      <Tabs
        id="hd-queues"
        value={queue}
        onChange={(v) => setQueue(v as Queue)}
        tabs={[
          { id: "mine", label: `Mine${count("mine")}` },
          { id: "unassigned", label: `Unassigned${count("unassigned")}` },
          { id: "alerts", label: `SLA alerts${count("alerts")}` },
          { id: "open", label: `All open${count("open")}` },
        ]}
      />
      <TabPanel id="hd-queues" tab={queue}>
        <QueueList queue={queue} q={queues[queue]} />
      </TabPanel>
    </div>
  );
}
