// Beside a ticket's feed: where its service levels stand, who has it and how urgent it is, who the
// customer is (what they are entitled to, what else they have asked), what is linked to it, its files.
// Fields change where they stand, against the version on screen.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { api, docPath } from "@ferrum/client";
import { Badge, Button, Spinner, statusTone } from "@ferrum/kit";
import { type EntityMeta, LinkInput, LinkTitle, type Person, Related } from "@ferrum/records";
import { fileSize } from "./feed";
import { PRIORITIES, requester, type Ticket } from "./inbox";
import { type SlaTicket, slaCards } from "./sla";

/** What a customer's support contract is called on a ticket and in the catalog. */
const CONTRACT = { field: "support_contract", entity: "SupportContract" } as const;

export interface RailTicket extends Ticket, SlaTicket {
  version: number;
  customer?: string | null;
  escalation_level?: number | null;
  entitlement?: string | null;
  _titles?: Record<string, string>;
  [key: string]: unknown;
}

const ENTITLEMENT: Record<string, { tone: "green" | "amber" | "red" | "neutral"; text: string }> = {
  Covered: { tone: "green", text: "Covered by a support contract" },
  Expired: { tone: "red", text: "Support contract has ended" },
  Exhausted: { tone: "amber", text: "Support contract has no tickets left" },
  None: { tone: "neutral", text: "No support contract" },
};

function Others({ t }: { t: RailTicket }) {
  const by: unknown[] | null = t.customer ? [["customer", "=", t.customer]] : t.contact_email ? [["contact_email", "=", t.contact_email]] : null;
  const q = useQuery({
    queryKey: ["helpdesk", "others", t.id, t.customer, t.contact_email],
    enabled: !!by,
    staleTime: 60_000,
    queryFn: ({ signal }) =>
      api.get<{ rows: Ticket[] }>("/api/v1/Ticket", { filters: JSON.stringify(by), fields: "number,subject,status,opened_at", sort: "opened_at desc", limit: 6 }, signal).then((r) => r.rows.filter((x) => x.id !== t.id).slice(0, 5)),
  });
  if (!by) return null;
  return (
    <div className="hd-others">
      <h4>Their other tickets</h4>
      {q.isLoading ? (
        <Spinner label="Looking for their other tickets" />
      ) : q.error ? (
        <p className="faint">They could not be loaded.</p>
      ) : !q.data?.length ? (
        <p className="faint">None. This is their first.</p>
      ) : (
        <ul>
          {q.data.map((o) => (
            <li key={o.id}>
              <Link to={`/t/${o.id}`}>
                <span className="hd-others-subject">{o.subject || o.number}</span>
              </Link>
              <Badge tone={statusTone(o.status)}>{o.status}</Badge>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Files({ id }: { id: string }) {
  const q = useQuery({
    queryKey: ["helpdesk", "files", id],
    queryFn: ({ signal }) => api.get<{ attachments: { id: string; file_name: string; size_bytes: number; url: string }[] }>(docPath("Ticket", id, "attachments"), undefined, signal),
  });
  const files = q.data?.attachments ?? [];
  if (!files.length) return null;
  return (
    <section className="hd-card" aria-labelledby="hd-rail-files">
      <h3 id="hd-rail-files">Files</h3>
      <ul className="hd-files">
        {files.map((f) => (
          <li key={f.id}>
            <a className="hd-file" href={f.url} download={f.file_name}>
              <span className="hd-file-name">{f.file_name}</span>
              <span className="faint">{fileSize(f.size_bytes)}</span>
            </a>
          </li>
        ))}
      </ul>
    </section>
  );
}

export function Rail({
  meta,
  t,
  me,
  people,
  name,
  editable,
  save,
}: {
  meta: EntityMeta;
  t: RailTicket;
  me: string;
  people: Person[];
  name: (email: string) => string;
  /** Whether the person may change the ticket, and it is not closed. */
  editable: boolean;
  save: (field: string, value: unknown) => Promise<void>;
}) {
  const cards = slaCards(t);
  const assignee = (t.assigned_to ?? "").toLowerCase();
  const entitled = t.entitlement ? ENTITLEMENT[t.entitlement] : null;
  const known = people.some((p) => p.email.toLowerCase() === assignee);
  return (
    <aside className="hd-rail" aria-label="Ticket details">
      <section className="hd-card" aria-labelledby="hd-rail-sla">
        <h3 id="hd-rail-sla">Service levels</h3>
        <ul className="hd-sla">
          {cards.map((c) => (
            <li key={c.id} data-state={c.state}>
              <span className="hd-sla-title">{c.title}</span>
              <strong>{c.text}</strong>
              {c.detail && <span className="faint">{c.detail}</span>}
            </li>
          ))}
        </ul>
        {(t.escalation_level ?? 0) > 0 && <Badge tone="amber">Escalated to level {t.escalation_level}</Badge>}
      </section>

      <section className="hd-card" aria-labelledby="hd-rail-fields">
        <h3 id="hd-rail-fields">Handling</h3>
        <div className="stack hd-fields">
          <div className="field">
            <label className="field-label" htmlFor="hd-priority">
              Priority
            </label>
            <select id="hd-priority" className="select" disabled={!editable} value={t.priority ?? "Medium"} onChange={(e) => void save("priority", e.target.value)}>
              {PRIORITIES.map((p) => (
                <option key={p}>{p}</option>
              ))}
            </select>
          </div>
          <div className="field">
            <label className="field-label" htmlFor="hd-assignee">
              Assigned to
            </label>
            <select id="hd-assignee" className="select" disabled={!editable} value={assignee} onChange={(e) => void save("assigned_to", e.target.value || null)}>
              <option value="">Nobody</option>
              {assignee && !known && <option value={assignee}>{name(assignee)}</option>}
              {people.map((p) => (
                <option key={p.id} value={p.email.toLowerCase()}>
                  {p.full_name || p.email}
                </option>
              ))}
            </select>
            {editable && assignee !== me.toLowerCase() && (
              <Button size="sm" onClick={() => void save("assigned_to", me.toLowerCase())}>
                Assign to me
              </Button>
            )}
          </div>
          <div className="field">
            <label className="field-label" htmlFor="hd-team">
              Team
            </label>
            <LinkInput to="SupportTeam" id="hd-team" value={t.team ?? null} title={t._titles?.team} disabled={!editable} onChange={(v) => void save("team", v)} />
          </div>
          <div className="field">
            <label className="field-label" htmlFor="hd-type">
              Type
            </label>
            <LinkInput to="TicketType" id="hd-type" value={(t.ticket_type as string | null | undefined) ?? null} title={t._titles?.ticket_type} disabled={!editable} onChange={(v) => void save("ticket_type", v)} />
          </div>
        </div>
      </section>

      <section className="hd-card" aria-labelledby="hd-rail-customer">
        <h3 id="hd-rail-customer">Customer</h3>
        <p className="hd-customer">
          {t.customer ? <LinkTitle to="Customer" id={t.customer} title={t.customer_name ?? undefined} /> : <strong>{requester(t) || "Unknown"}</strong>}
        </p>
        <p className="faint">
          {[t.contact_name && t.contact_name !== t.customer_name ? t.contact_name : null, t.channel ? `by ${t.channel.toLowerCase()}` : null].filter(Boolean).join(" · ")}
        </p>
        {t.contact_email && (
          <p>
            <a href={`mailto:${t.contact_email}`}>{t.contact_email}</a>
          </p>
        )}
        {entitled && (
          <p className="hd-entitled">
            <Badge tone={entitled.tone}>{entitled.text}</Badge>
            {t[CONTRACT.field] ? <LinkTitle to={CONTRACT.entity} id={String(t[CONTRACT.field])} title={t._titles?.[CONTRACT.field]} /> : null}
          </p>
        )}
        <Others t={t} />
      </section>

      {meta.linked_from?.length ? (
        <section className="hd-card" aria-labelledby="hd-rail-linked">
          <h3 id="hd-rail-linked">Linked records</h3>
          <Related meta={meta} id={t.id} />
        </section>
      ) : null}

      <Files id={t.id} />

      <Link className="hd-rail-form" to={`/e/Ticket/${t.id}`}>
        Every field, in the form
      </Link>
    </aside>
  );
}
