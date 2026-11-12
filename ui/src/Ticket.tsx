// A ticket as an agent works it: the whole conversation in one feed (what the customer wrote, every
// reply and internal note, and what happened to the ticket between them), one box to answer the
// customer or write to the team, and beside it where the service levels stand, who has it and who the
// customer is. Everything is kept current while it is open, and a write carries the version on screen,
// so two agents answering at once is told to the second, never overwritten.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router";
import { api, ApiError, bootstrap, docPath, errorMessage, formatMoment, t as tr, isolate } from "@ferrum/client";
import { Alert, Badge, Button, confirmDialog, Icon, Menu, MenuItem, placed, ReaderSkeleton, StatePanel, toast, useLiveEntity, useShellTitle } from "@ferrum/kit";
import { can, type Doc, type EntityMeta, useEntityMeta, useMe, useMetaIndex, useRecord } from "@ferrum/records";
import { MergeDialog, type Merged } from "./Merge";
import type { MergeTicket } from "./duplicates";
import { Composer, type Mode, type Outgoing } from "./Composer";
import { Conversation } from "./Conversation";
import { type AuditEvent, buildFeed, type FeedFilter, type Message, type Story, whatChanged } from "./feed";
import { prefersReducedMotion, useMedia, useNames, useQuiet } from "./hooks";
import { type Carried, inboxLink, initials, requester } from "./inbox";
import { command, isTyping } from "./keys";
import { Rail, type RailTicket } from "./Rail";
import { ResolveDialog, type Resolution } from "./Resolve";
import { DecisionDialog, type Decision } from "./Entitlement";
import { Shortcuts } from "./Shortcuts";
import { LoadFailed } from "./states";
import { PRIORITY_TONES, STATUS_TONES } from "./tones";

interface TicketDoc extends RailTicket, Story {
  conversation?: Message[];
  resolution_details?: string | null;
  entitlement_approval?: string | null;
  updated_by?: string | null;
  /** Closed as a duplicate of this ticket, whose conversation holds its words now. */
  merged_into?: string | null;
  merged_at?: string | null;
}

const KEYS = { r: "reply", n: "note", e: "resolve", a: "take", j: "next", k: "previous", d: "details", "?": "help" } as const;

const SHORTCUTS = [
  { keys: ["r"], get says() { return tr("Reply to the customer"); } },
  { keys: ["n"], get says() { return tr("Write an internal note"); } },
  { keys: ["e"], get says() { return tr("Resolve the ticket"); } },
  { keys: ["a"], get says() { return tr("Assign it to me"); } },
  { keys: ["j"], get says() { return tr("Next ticket in the list you came from"); } },
  { keys: ["k"], get says() { return tr("Previous ticket"); } },
  { keys: ["d"], get says() { return tr("Show or hide the details"); } },
  { keys: ["/"], get says() { return tr("In a reply: find a saved reply"); } },
  { get keys() { return [tr("Ctrl or ⌘"), tr("Enter")]; }, get says() { return tr("Send what you have written"); } },
  { keys: ["?"], get says() { return tr("This list"); } },
];

const key = (id: string) => ["peek", "Ticket", id];

/** What to tell the agent about a message just saved: whether it reached the customer by email. */
function sentNote(saved: TicketDoc, internal: boolean): string {
  const last = saved.conversation?.[saved.conversation.length - 1];
  if (internal) return tr("Note added");
  return last?.emailed ? tr("Reply sent. Emailed to {contact_email}.", { contact_email: String(saved.contact_email) }) : tr("Reply added. No email went out, but the customer can read it in the portal.");
}

/** The address of a ticket that works from anywhere, which takes a customer to the portal. */
async function copyLink(id: string) {
  const host = bootstrap()?.workspace.host ?? location.host;
  try {
    await navigator.clipboard.writeText(`${location.protocol}//${host}/open/Ticket/${id}`);
    toast(tr("Link copied"));
  } catch {
    toast(tr("Your browser did not allow copying"), "error");
  }
}

export function TicketRoute() {
  const { id = "" } = useParams();
  const meta = useEntityMeta("Ticket");
  if (meta.error) return <LoadFailed error={meta.error} onRetry={() => void meta.refetch()} />;
  if (!meta.data) return <ReaderSkeleton paragraphs={3} />;
  return <Workspace key={id} meta={meta.data} id={id} />;
}

function Workspace({ meta, id }: { meta: EntityMeta; id: string }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const carried = (useLocation().state as Carried | null) ?? {};
  const [asked] = useSearchParams();
  const me = useMe().data?.user;
  const { name, people } = useNames();
  const index = useMetaIndex().data;
  const { record, save } = useRecord(meta, id);
  const t = record.data as TicketDoc | undefined;
  const [filter, setFilter] = useState<FeedFilter>("all");
  const [mode, setMode] = useState<Mode>("reply");
  const [resolving, setResolving] = useState<"Resolved" | "Closed" | null>(null);
  const [deciding, setDeciding] = useState<Decision | null>(null);
  const [merging, setMerging] = useState(false);
  const [help, setHelp] = useState(false);
  const [conflict, setConflict] = useState<string | null>(null);
  const wide = useMedia("(min-width: 1000px)");
  const [railChoice, setRailChoice] = useState<boolean | null>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  const bottom = useRef<HTMLElement>(null);
  const feedEnd = useRef<HTMLLIElement>(null);
  const arrived = useRef(false);
  const seen = useRef<TicketDoc | null>(null);
  useShellTitle(t ? `${t.number ?? ""} ${t.subject ?? ""}`.trim() : null);

  const audit = useQuery({
    queryKey: ["helpdesk", "audit", id],
    retry: false,
    queryFn: ({ signal }) =>
      api.get<{ events: AuditEvent[] }>(docPath("Ticket", id, "audit"), undefined, signal).catch((e) => {
        // Reading a ticket does not always include its history, and the conversation does not wait for it.
        if (e instanceof ApiError && e.status === 403) return { events: [] as AuditEvent[] };
        throw e;
      }),
  });
  // The customer's other tickets, for merging into this one, asked for only when the dialog opens.
  const by: unknown[] | null = t?.customer ? [["customer", "=", t.customer]] : t?.contact_email ? [["contact_email", "=", t.contact_email]] : null;
  const others = useQuery({
    queryKey: ["helpdesk", "merge-candidates", id, t?.customer, t?.contact_email],
    enabled: merging && !!by,
    staleTime: 0,
    queryFn: ({ signal }) =>
      api.get<{ rows: MergeTicket[] }>(docPath("Ticket"), { filters: JSON.stringify(by), fields: "number,subject,status,customer_name,contact_email,opened_at", sort: "opened_at desc", limit: 50 }, signal).then((r) => r.rows),
  });
  const feed = useMemo(() => (t ? buildFeed(t, audit.data?.events ?? [], name) : []), [t, audit.data, name]);

  const refresh = useCallback(() => {
    void qc.invalidateQueries({ queryKey: key(id) });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "audit", id] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "files", id] });
    void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
  }, [qc, id]);
  const soon = useQuiet(refresh, 400);
  useLiveEntity(
    "Ticket",
    useCallback(
      (e) => {
        if (e.id === id) soon();
      },
      [id, soon],
    ),
  );

  // A colleague's change while this is open is told in a line, by comparing what is on screen with what
  // arrived; the agent's own writes are not news.
  useEffect(() => {
    if (!t) return;
    const before = seen.current;
    seen.current = t;
    if (!before || before.version === t.version || t.updated_by === me?.id) return;
    const said = whatChanged(before, t, name);
    if (said) toast(said, "info");
  }, [t, me?.id, name]);

  // Open at the latest word, as a conversation does, with the box ready when the inbox asked to reply.
  useEffect(() => {
    // Once the history is in too, so that what arrives late does not push the end out of sight.
    if (!t || audit.isLoading || arrived.current) return;
    arrived.current = true;
    (bottom.current ?? feedEnd.current)?.scrollIntoView({ block: "end" });
    if (asked.has("reply") && t.status !== "Closed") input.current?.focus();
  }, [t, audit.isLoading, asked]);

  const status = t?.status ?? "Open";
  const closed = status === "Closed";
  const done = status === "Resolved" || closed;
  const writable = can(meta.permissions, "write") && !closed;
  const order = carried.order ?? [];
  const at = order.indexOf(id);
  const previous = at > 0 ? order[at - 1] : null;
  const next = at >= 0 && at < order.length - 1 ? order[at + 1] : null;
  const go = useCallback((to: string | null) => to && navigate(`/t/${to}`, { replace: true, state: carried }), [navigate, carried]);
  const home = inboxLink(carried);

  /** A write against the version on screen. A newer one is told to the agent and shown, not overwritten. */
  const write = async (body: Record<string, unknown>): Promise<TicketDoc | null> => {
    if (!t) return null;
    try {
      const saved = await api.patch<TicketDoc>(docPath("Ticket", id), body, { ifMatch: Number(t.version) });
      seen.current = saved;
      qc.setQueryData(key(id), (old: Doc | undefined) => ({ ...saved, _titles: { ...old?._titles, ...saved._titles } }));
      void qc.invalidateQueries({ queryKey: ["helpdesk", "audit", id] });
      void qc.invalidateQueries({ queryKey: ["helpdesk", "files", id] });
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
      return saved;
    } catch (err) {
      if (err instanceof ApiError && (err.status === 409 || err.status === 412)) {
        setConflict(tr("This ticket changed while you were writing. It shows the change now, and your words are kept. Read it, then send again if it still fits."));
        refresh();
      } else {
        toast(errorMessage(err), "error");
      }
      return null;
    }
  };

  const existing = () => (t?.conversation ?? []).map((m) => ({ id: m.id }));

  const send = async (m: Outgoing): Promise<boolean> => {
    if (!t || !me) return false;
    const mineAlready = (t.assigned_to ?? "").toLowerCase() === me.email.toLowerCase();
    const takeover: Record<string, unknown> = {};
    if (!m.internal && !mineAlready) {
      if (t.assigned_to) {
        const ok = await confirmDialog({
          title: tr("{value} has this ticket", { value: name(t.assigned_to) }),
          message: tr("Reply anyway? Sending also takes the ticket over, so the team can see you have it."),
          confirmLabel: tr("Reply and take it"),
          cancelLabel: tr("Not yet"),
        });
        if (!ok) return false;
      }
      takeover.assigned_to = me.email.toLowerCase();
    }
    const saved = await write({
      conversation: [...existing(), { author_kind: "Agent", body: m.body, is_internal: m.internal, attachments: m.files.map((f) => ({ id: f.id })) }],
      ...takeover,
    });
    if (!saved) return false;
    setConflict(null);
    toast(sentNote(saved, m.internal));
    requestAnimationFrame(() => (bottom.current ?? feedEnd.current)?.scrollIntoView({ block: "end", behavior: prefersReducedMotion() ? "auto" : "smooth" }));
    return true;
  };

  const setStatus = async (to: string) => {
    if (!t) return;
    if (to === "Resolved" || (to === "Closed" && !t.resolution_details)) {
      setResolving(to);
      return;
    }
    if (to === "Closed") {
      const ok = await confirmDialog({ title: tr("Close this ticket for good?"), message: tr("A closed ticket is final. It cannot be reopened and nothing more can be written on it."), confirmLabel: tr("Close it"), danger: true });
      if (!ok) return;
    }
    if (await write({ status: to })) toast(tr("Status: {to}", { to }));
  };

  const finish = async (r: Resolution): Promise<boolean> => {
    if (!t || !resolving) return false;
    const reply = r.reply ? [{ author_kind: "Agent", body: r.resolution, is_internal: false }] : [];
    const saved = await write({ status: resolving, resolution_details: r.resolution, ...(reply.length ? { conversation: [...existing(), ...reply] } : {}) });
    if (!saved) return false;
    toast(r.reply ? `${resolving}. ${sentNote(saved, false)}` : resolving);
    // The next ticket of the list is the next job.
    if (next) go(next);
    else if (order.length) navigate(home);
    return true;
  };

  const take = () => {
    if (me && writable) void save("assigned_to", me.email.toLowerCase());
  };

  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (isTyping(e.target as Element | null)) return;
      const c = command(e, KEYS);
      if (!c) return;
      e.preventDefault();
      switch (c) {
        case "reply":
        case "note":
          if (!writable) return;
          setMode(c === "reply" ? "reply" : "note");
          input.current?.focus();
          break;
        case "resolve":
          if (writable && !done) setResolving("Resolved");
          break;
        case "take":
          take();
          break;
        case "next":
          go(next);
          break;
        case "previous":
          go(previous);
          break;
        case "details":
          setRailChoice((v) => !(v ?? wide));
          break;
        case "help":
          setHelp(true);
          break;
      }
    };
    document.addEventListener("keydown", on);
    return () => document.removeEventListener("keydown", on);
  });

  if (record.error) {
    if (record.error instanceof ApiError && record.error.status === 404) {
      return (
        <StatePanel kind="empty" title={tr("This ticket no longer exists")} action={<Link to="/">{tr("Back to the inbox")}</Link>} />
      );
    }
    return <LoadFailed error={record.error} onRetry={() => void record.refetch()} />;
  }
  if (!t || !me) return <ReaderSkeleton paragraphs={3} />;

  const showRail = railChoice ?? wide;
  const hasReplies = !!index?.entities.some((e) => e.name === "SavedReply");
  const owner = t.assigned_to && t.assigned_to.toLowerCase() !== me.email.toLowerCase() ? name(t.assigned_to) : null;
  const who = requester(t);
  const about = {
    ticket: { number: t.number, subject: t.subject },
    contact: { name: t.contact_name, company: t.customer_name, email: t.contact_email },
    agent: { name: me.full_name, email: me.email },
  };
  const canMerge = !!meta.actions.some((a) => a.name === "merge_tickets") && writable;
  const mergedInto = t.merged_into ? { id: t.merged_into, title: t._titles?.merged_into ?? tr("another ticket") } : null;
  const thisOne: MergeTicket = { id, number: t.number, subject: t.subject, status: t.status, customer_name: t.customer_name, contact_email: t.contact_email, opened_at: t.opened_at };
  const afterMerge = (r: Merged) => {
    toast(r.message ?? tr("Merged"));
    refresh();
    void qc.invalidateQueries({ queryKey: ["helpdesk", "merge-candidates"] });
  };
  const canWarranty = !!t.item && !!t.customer && meta.actions.some((a) => a.name === "make_warranty_claim");
  // Held outside the customer's entitlement: a Support Manager decides (Support Settings).
  const waiting = t.entitlement_approval === "Waiting";
  const decides = waiting && writable && (me.is_admin || me.roles.includes("Support Manager"));
  const decide = async (reason: string) => {
    try {
      await api.post(docPath("Ticket", id, "actions", `${deciding}_outside_entitlement`), { reason });
      toast(deciding === "approve" ? tr("Approved: it can be worked now") : tr("Declined, and resolved"));
      setDeciding(null);
      refresh();
    } catch (err) {
      toast(errorMessage(err), "error");
    }
  };

  return (
    <div className="hd-ws">
      <header className="hd-ws-head">
        <div className="hd-ws-title">
          <p className="crumbs">
            <Link to={home}>{tr("Inbox")}</Link>
            <Icon name="chevronRight" size={12} />
            <span>{isolate(t.number ?? "")}</span>
          </p>
          <h1 className="page-title">{t.subject || tr("No subject")}</h1>
          <p className="row">
            <Badge tone={STATUS_TONES[status] ?? "neutral"}>{isolate(status)}</Badge>
            {t.priority && t.priority !== "Medium" && <Badge tone={PRIORITY_TONES[t.priority] ?? "neutral"}>{isolate(t.priority)}</Badge>}
            {t.assigned_to ? (
              <span className="hd-owner" title={t.assigned_to}>
                <span className="hd-avatar hd-avatar-sm" aria-hidden="true">
                  {isolate(initials(name(t.assigned_to)))}
                </span>
                {owner ?? tr("You have it")}
              </span>
            ) : (
              <span className="faint">{tr("Nobody has it yet")}</span>
            )}
            {who && <span className="faint">{tr("from {who}", { who })}</span>}
          </p>
        </div>
        <div className="page-actions">
          {order.length > 0 && (
            <span className="hd-step" role="group" aria-label={tr("Tickets in your list")}>
              <button type="button" className="icon-btn" aria-label={tr("Previous ticket")} title={tr("Previous ticket (k)")} disabled={!previous} onClick={() => go(previous)}>
                <Icon name="up" />
              </button>
              <button type="button" className="icon-btn" aria-label={tr("Next ticket")} title={tr("Next ticket (j)")} disabled={!next} onClick={() => go(next)}>
                <Icon name="down" />
              </button>
            </span>
          )}
          {writable && !done && (
            <Button variant="primary" icon="check" onClick={() => setResolving("Resolved")} title={tr("Resolve (e)")}>
              {tr("Resolve")}
            </Button>
          )}
          {writable && status === "Resolved" && <Button onClick={() => void setStatus("Open")}>{tr("Reopen")}</Button>}
          <button type="button" className="btn btn-default btn-md" aria-pressed={showRail} onClick={() => setRailChoice(!showRail)} title={tr("Details (d)")}>
            {tr("Details")}
          </button>
          <Menu ariaLabel={tr("More actions")} icon="chevronDown" buttonClass="btn btn-default btn-md">
            {(close) => (
              <>
                {writable && status !== "On Hold" && !done && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      void setStatus("On Hold");
                    }}
                  >
                    {tr("Put on hold")}
                  </MenuItem>
                )}
                {decides && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      setDeciding("approve");
                    }}
                  >
                    {tr("Approve work outside the entitlement…")}
                  </MenuItem>
                )}
                {decides && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      setDeciding("decline");
                    }}
                  >
                    {tr("Decline work outside the entitlement…")}
                  </MenuItem>
                )}
                {writable && status === "On Hold" && !waiting && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      void setStatus("Open");
                    }}
                  >
                    {tr("Take off hold")}
                  </MenuItem>
                )}
                {writable && !closed && (
                  <MenuItem
                    danger
                    onSelect={() => {
                      close();
                      void setStatus("Closed");
                    }}
                  >
                    {tr("Close for good…")}
                  </MenuItem>
                )}
                {canMerge && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      setMerging(true);
                    }}
                  >
                    {tr("Merge other tickets into this one…")}
                  </MenuItem>
                )}
                {canWarranty && writable && (
                  <MenuItem
                    onSelect={() => {
                      close();
                      void api
                        .post<{ id?: string }>(docPath("Ticket", id, "actions", "make_warranty_claim"), {})
                        .then((made) => made.id && navigate(`/e/WarrantyClaim/${made.id}`))
                        .catch((err) => toast(errorMessage(err), "error"));
                    }}
                  >
                    {tr("Create a warranty claim")}
                  </MenuItem>
                )}
                <MenuItem
                  onSelect={() => {
                    close();
                    void copyLink(id);
                  }}
                >
                  {tr("Copy link to this ticket")}
                </MenuItem>
                <MenuItem
                  onSelect={() => {
                    close();
                    setHelp(true);
                  }}
                >
                  {tr("Keyboard shortcuts")}
                </MenuItem>
              </>
            )}
          </Menu>
        </div>
      </header>

      {mergedInto ? (
        <Alert tone="info">
          {placed(tr("Merged into {p1}."), { p1: <Link to={`/t/${mergedInto.id}`}>{isolate(mergedInto.title)}</Link> })}
          {t.merged_at ? ` ${formatMoment(t.merged_at, { dateStyle: "medium" })}.` : ""} {tr("Its conversation moved there, and it cannot be replied to. Carry on in the ticket it was merged into.")}
        </Alert>
      ) : (
        closed && <Alert tone="info">{tr("This ticket is closed for good. You can still read everything on it.")}</Alert>
      )}
      {waiting && (
        <Alert tone="warning">
          {tr("Outside the customer's entitlement: it waits for a Support Manager to approve the work. Nothing goes to the customer until then; internal notes can be written.")}
        </Alert>
      )}

      <div className="hd-ws-body" data-rail={showRail || undefined}>
        <div className="hd-ws-main">
          {audit.error && <p className="faint">{tr("What happened to the ticket could not be loaded; the conversation is complete.")}</p>}
          <Conversation items={feed} filter={filter} onFilter={setFilter} address={t.contact_email ?? null} endRef={feedEnd} />
          {writable ? (
            <Composer
              ticket={id}
              mode={mode}
              onMode={setMode}
              inputRef={input}
              rootRef={bottom}
              to={{ name: t.contact_name || t.customer_name || tr("the customer"), email: t.contact_email ?? null }}
              assignee={owner ? tr("{owner} has this ticket.", { owner }) : t.assigned_to ? null : tr("Nobody has this ticket yet.")}
              about={about}
              replies={hasReplies}
              conflict={conflict}
              onSend={send}
            />
          ) : (
            !closed && <p className="muted">{tr("You can read this ticket but not answer it.")}</p>
          )}
        </div>
        {showRail && <Rail meta={meta} t={t} me={me.email} people={people} name={name} editable={writable} save={save} />}
      </div>

      <ResolveDialog
        open={!!resolving}
        status={resolving ?? "Resolved"}
        what={t.number ?? tr("the ticket")}
        tell={t.contact_email ? (t.contact_name ?? t.contact_email) : null}
        onClose={() => setResolving(null)}
        onSubmit={finish}
      />
      <MergeDialog
        open={merging}
        keep={id}
        loading={others.isLoading}
        failed={!!others.error}
        tickets={[thisOne, ...(others.data ?? []).filter((x) => x.id !== id)]}
        onClose={() => setMerging(false)}
        onDone={afterMerge}
      />
      <DecisionDialog decision={deciding} onClose={() => setDeciding(null)} onSubmit={decide} />
      <Shortcuts open={help} onClose={() => setHelp(false)} title={tr("Keyboard shortcuts")} rows={SHORTCUTS} />
    </div>
  );
}
