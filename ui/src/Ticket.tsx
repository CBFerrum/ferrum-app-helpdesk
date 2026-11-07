// A ticket as an agent works it: the whole conversation in one feed (what the customer wrote, every
// reply and internal note, and what happened to the ticket between them), one box to answer the
// customer or write to the team, and beside it where the service levels stand, who has it and who the
// customer is. Everything is kept current while it is open, and a write carries the version on screen,
// so two agents answering at once is told to the second, never overwritten.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useLocation, useNavigate, useParams, useSearchParams } from "react-router";
import { api, ApiError, bootstrap, docPath, errorMessage, t as tr } from "@ferrum/client";
import { Alert, Badge, Button, confirmDialog, EmptyState, ErrorState, Icon, Menu, MenuItem, PageLoading, statusTone, toast, useLiveEntity, useShellTitle } from "@ferrum/kit";
import { can, type Doc, type EntityMeta, useEntityMeta, useMe, useMetaIndex, useRecord } from "@ferrum/records";
import { Composer, type Mode, type Outgoing } from "./Composer";
import { Conversation } from "./Conversation";
import { type AuditEvent, buildFeed, type FeedFilter, type Message, type Story, whatChanged } from "./feed";
import { prefersReducedMotion, useMedia, useNames, useQuiet } from "./hooks";
import { type Carried, inboxLink, initials, requester } from "./inbox";
import { command, isTyping } from "./keys";
import { Rail, type RailTicket } from "./Rail";
import { ResolveDialog, type Resolution } from "./Resolve";
import { Shortcuts } from "./Shortcuts";

interface TicketDoc extends RailTicket, Story {
  conversation?: Message[];
  resolution_details?: string | null;
  updated_by?: string | null;
}

const KEYS = { r: "reply", n: "note", e: "resolve", a: "take", j: "next", k: "previous", d: "details", "?": "help" } as const;

const SHORTCUTS = [
  { keys: ["r"], says: "Reply to the customer" },
  { keys: ["n"], says: "Write an internal note" },
  { keys: ["e"], says: "Resolve the ticket" },
  { keys: ["a"], says: "Assign it to me" },
  { keys: ["j"], says: "Next ticket in the list you came from" },
  { keys: ["k"], says: "Previous ticket" },
  { keys: ["d"], says: "Show or hide the details" },
  { keys: ["/"], says: "In a reply: find a saved reply" },
  { keys: ["Ctrl or ⌘", "Enter"], says: "Send what you have written" },
  { keys: ["?"], says: "This list" },
];

const key = (id: string) => ["peek", "Ticket", id];

/** What to tell the agent about a message just saved: whether it reached the customer by email. */
function sentNote(saved: TicketDoc, internal: boolean): string {
  const last = saved.conversation?.[saved.conversation.length - 1];
  if (internal) return "Note added";
  return last?.emailed ? `Reply sent. Emailed to ${saved.contact_email}.` : "Reply added. No email went out, but the customer can read it in the portal.";
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
  if (meta.error) return <ErrorState error={meta.error} onRetry={() => void meta.refetch()} />;
  if (!meta.data) return <PageLoading label={tr("Loading the ticket")} />;
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
        setConflict("This ticket changed while you were writing. It shows the change now, and your words are kept. Read it, then send again if it still fits.");
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
          title: `${name(t.assigned_to)} has this ticket`,
          message: "Reply anyway? Sending also takes the ticket over, so the team can see you have it.",
          confirmLabel: "Reply and take it",
          cancelLabel: "Not yet",
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
      const ok = await confirmDialog({ title: "Close this ticket for good?", message: "A closed ticket is final. It cannot be reopened and nothing more can be written on it.", confirmLabel: "Close it", danger: true });
      if (!ok) return;
    }
    if (await write({ status: to })) toast(`Status: ${to}`);
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
        <EmptyState title={tr("This ticket no longer exists")}>
          <Link to="/">{tr("Back to the inbox")}</Link>
        </EmptyState>
      );
    }
    return <ErrorState error={record.error} onRetry={() => void record.refetch()} />;
  }
  if (!t || !me) return <PageLoading label={tr("Loading the ticket")} />;

  const showRail = railChoice ?? wide;
  const hasReplies = !!index?.entities.some((e) => e.name === "SavedReply");
  const owner = t.assigned_to && t.assigned_to.toLowerCase() !== me.email.toLowerCase() ? name(t.assigned_to) : null;
  const who = requester(t);
  const about = {
    ticket: { number: t.number, subject: t.subject },
    contact: { name: t.contact_name, company: t.customer_name, email: t.contact_email },
    agent: { name: me.full_name, email: me.email },
  };
  const canWarranty = !!t.item && !!t.customer && meta.actions.some((a) => a.name === "make_warranty_claim");

  return (
    <div className="hd-ws">
      <header className="hd-ws-head">
        <div className="hd-ws-title">
          <p className="crumbs">
            <Link to={home}>{tr("Inbox")}</Link>
            <Icon name="chevronRight" size={12} />
            <span>{t.number}</span>
          </p>
          <h1 className="page-title">{t.subject || "No subject"}</h1>
          <p className="row">
            <Badge tone={statusTone(status)}>{status}</Badge>
            {t.priority && t.priority !== "Medium" && <Badge tone={t.priority === "Urgent" ? "red" : t.priority === "High" ? "amber" : "neutral"}>{t.priority}</Badge>}
            {t.assigned_to ? (
              <span className="hd-owner" title={t.assigned_to}>
                <span className="hd-avatar hd-avatar-sm" aria-hidden="true">
                  {initials(name(t.assigned_to))}
                </span>
                {owner ?? "You have it"}
              </span>
            ) : (
              <span className="faint">{tr("Nobody has it yet")}</span>
            )}
            {who && <span className="faint">from {who}</span>}
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
          <Menu ariaLabel="More actions" icon="chevronDown" buttonClass="btn btn-default btn-md">
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
                {writable && status === "On Hold" && (
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

      {closed && <Alert tone="info">{tr("This ticket is closed for good. You can still read everything on it.")}</Alert>}

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
              to={{ name: t.contact_name || t.customer_name || "the customer", email: t.contact_email ?? null }}
              assignee={owner ? `${owner} has this ticket.` : t.assigned_to ? null : "Nobody has this ticket yet."}
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
        what={t.number ?? "the ticket"}
        tell={t.contact_email ? (t.contact_name ?? t.contact_email) : null}
        onClose={() => setResolving(null)}
        onSubmit={finish}
      />
      <Shortcuts open={help} onClose={() => setHelp(false)} title={tr("Keyboard shortcuts")} rows={SHORTCUTS} />
    </div>
  );
}
