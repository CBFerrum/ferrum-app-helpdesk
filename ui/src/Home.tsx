// Helpdesk's home, the agent inbox: the tickets that still need someone, in queues with how many each
// holds, the most pressing first, with the filters a person saves, who has what, and the keys to work
// through them without the mouse (j and k move, Enter opens, x selects, e resolves, r replies, a takes,
// ? lists the rest). Filters, queue and order live in the address, so a link opens the same view; a
// change shows at once and goes back, with the server's reason, if it is refused. A ticket opens in
// its workspace (./Ticket.tsx) carrying this list's order, so next and previous walk it.

import { useInfiniteQuery, useQueries, useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link, useNavigate, useSearchParams } from "react-router";
import { api, ApiError, docPath, errorMessage, newKey, t as tr, isolate } from "@ferrum/client";
import { Alert, Button, confirmDialog, Dialog, Field, Icon, ListSkeleton, StatePanel, toast, useDebounced, useLiveEntity, useShellTitle, placed } from "@ferrum/kit";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuLabel, DropdownMenuSeparator, DropdownMenuTrigger } from "@ferrum/kit/primitives";
import { can, useMe, useMetaIndex } from "@ferrum/records";
import { useMedia, useNames, useNow, useQuiet } from "./hooks";
import {
  ACTIVE,
  type BulkResult,
  CHANNELS,
  type Carried,
  describeBulk,
  filterNameError,
  type InboxState,
  isFilter,
  isRefined,
  type QueueId,
  QUEUES,
  queueFilters,
  PRIORITIES,
  readState,
  refineFilters,
  type SavedFilter,
  saveFilter,
  savedFrom,
  SORTS,
  sortTickets,
  START,
  step,
  TARGET_PICKS,
  targetFilter,
  type Ticket,
  toggle,
  workload,
  writeState,
} from "./inbox";
import { command, isTyping } from "./keys";
import { MergeDialog } from "./Merge";
import { stepLine, sendsReply, type Macro } from "./macros";
import { useMacros } from "./MacroPicker";
import { useTagDefs } from "./Tags";
import { Queues } from "./Queues";
import { LoadFailed } from "./states";
import { ResolveDialog } from "./Resolve";
import { Row } from "./Row";
import { Shortcuts } from "./Shortcuts";

const FIELDS =
  "number,subject,status,priority,channel,team,customer_name,contact_name,contact_email,assigned_to,sla_status,response_by,resolution_by,first_responded_at,opened_at,last_customer_message_at,last_agent_message_at,last_message_preview,routed_by,routed_at,response_state,resolution_state,version";
/** How many tickets one request brings. */
const PAGE = 100;
/** Where a person's saved filters are kept (their preferences, which only they can read). */
const PREF = "helpdesk:inbox";

const KEYS = {
  j: "down",
  ArrowDown: "down",
  k: "up",
  ArrowUp: "up",
  Enter: "open",
  o: "open",
  x: "pick",
  "Shift+x": "range",
  e: "resolve",
  r: "reply",
  a: "take",
  "/": "search",
  "?": "help",
  Escape: "clear",
  "1": "queue-1",
  "2": "queue-2",
  "3": "queue-3",
  "4": "queue-4",
  "5": "queue-5",
} as const;

const SHORTCUTS = [
  { keys: ["j", "k"], get says() { return tr("Move down and up the list"); } },
  { get keys() { return [tr("Enter")]; }, get says() { return tr("Open the ticket"); } },
  { keys: ["r"], get says() { return tr("Open it to reply"); } },
  { keys: ["x"], get says() { return tr("Select it (Shift + x: everything since the last one)"); } },
  { keys: ["e"], get says() { return tr("Resolve it, or all that are selected"); } },
  { keys: ["a"], get says() { return tr("Assign it to me, or all that are selected"); } },
  { keys: ["1", "…", "5"], get says() { return tr("Go to a queue"); } },
  { keys: ["/"], get says() { return tr("Search"); } },
  { get keys() { return [tr("Esc")]; }, get says() { return tr("Clear the selection"); } },
  { keys: ["?"], get says() { return tr("This list"); } },
];

interface Page {
  rows: Ticket[];
  total?: number;
}

const without = <T,>(o: Record<string, T>, key: string) => Object.fromEntries(Object.entries(o).filter(([k]) => k !== key));

/** A menu of choices on a button: a group of bulk actions. */
function Choose({ label, children }: { label: string; children: ReactNode }) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button type="button" className="btn btn-default btn-sm">
          {isolate(label)}
          <Icon name="chevronDown" size={12} />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" className="hd-menu-scroll">
        {children}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Naming a view to keep it. */
function SaveView({ saved, onSave, onClose }: { saved: SavedFilter[]; onSave: (name: string) => Promise<boolean>; onClose: () => void }) {
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const problem = name ? filterNameError(name, saved) : null;
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (filterNameError(name, saved) || busy) return;
    setBusy(true);
    if (await onSave(name)) onClose();
    setBusy(false);
  };
  return (
    <Dialog
      open
      onClose={onClose}
      title={tr("Save these filters")}
      footer={
        <>
          <Button onClick={onClose}>{tr("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-save-view" loading={busy} disabled={!name.trim() || !!problem}>
            {tr("Save")}
          </Button>
        </>
      }
    >
      <form id="hd-save-view" className="stack" onSubmit={submit}>
        <Field label={tr("Name")} htmlFor="hd-view-name" required error={problem ?? undefined} hint={tr("It appears under Saved filters, only for you.")}>
          <input id="hd-view-name" className="input" maxLength={80} value={name} onChange={(e) => setName(e.target.value)} />
        </Field>
      </form>
    </Dialog>
  );
}

export function HelpdeskHome() {
  useShellTitle(tr("Inbox"));
  const qc = useQueryClient();
  const navigate = useNavigate();
  const [sp, setSp] = useSearchParams();
  const state = useMemo(() => readState(sp), [sp]);
  const me = useMe().data?.user;
  const index = useMetaIndex().data;
  const entity = index?.entities.find((e) => e.name === "Ticket");
  const { name, people } = useNames();
  const wide = useMedia("(min-width: 900px)");
  const now = useNow();
  const [more, setMore] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const anchor = useRef<string | null>(null);
  const [cursor, setCursor] = useState<string | null>(null);
  const [overrides, setOverrides] = useState<Record<string, Partial<Ticket>>>({});
  const [resolving, setResolving] = useState<string[] | null>(null);
  const [merging, setMerging] = useState(false);
  const [saving, setSaving] = useState(false);
  const [help, setHelp] = useState(false);
  const [text, setText] = useState(state.q);
  const search = useRef<HTMLInputElement>(null);
  const all = useRef<HTMLInputElement>(null);

  // ── what the server holds ──
  const teams = useQuery({
    queryKey: ["helpdesk", "teams"],
    staleTime: 5 * 60_000,
    queryFn: ({ signal }) =>
      api
        .get<{ rows: { id: string; team_name: string }[] }>("/api/v1/SupportTeam", { fields: "team_name", sort: "team_name asc", limit: 200 }, signal)
        .then((r) => r.rows)
        .catch(() => [] as { id: string; team_name: string }[]),
  });
  const mine = useQuery({
    queryKey: ["helpdesk", "my-teams", me?.email],
    enabled: !!me,
    staleTime: 5 * 60_000,
    queryFn: ({ signal }) =>
      api
        .get<{ rows: { id: string }[] }>("/api/v1/SupportTeam", { filters: JSON.stringify([["members.agent", "=", me?.email.toLowerCase()]]), fields: "team_name", limit: 200 }, signal)
        .then((r) => r.rows.map((t) => t.id))
        .catch(() => [] as string[]),
  });
  const who = useMemo(() => ({ me: me?.email ?? "", teams: mine.data ?? [] }), [me?.email, mine.data]);
  const ready = !!me && mine.data !== undefined;
  const queues = QUEUES.filter((q) => q.id !== "teams" || who.teams.length > 0 || state.queue === "teams");

  const counts = useQueries({
    queries: queues.map((q) => ({
      queryKey: ["helpdesk", "queue", "count", q.id, who.me, who.teams.join()],
      enabled: ready,
      staleTime: 30_000,
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        api.get<{ total?: number }>("/api/v1/Ticket", { filters: JSON.stringify(queueFilters(q.id, who)), fields: "id", limit: 1, count: true }, signal).then((r) => r.total ?? 0),
    })),
  });
  const countOf: Partial<Record<QueueId, number>> = Object.fromEntries(queues.flatMap((q, i) => (counts[i]?.data === undefined ? [] : [[q.id, counts[i].data]])));

  // The targets that need an eye, each with how many open tickets are in that state.
  const targetIds = ["response:breached", "resolution:breached", "response:paused", "resolution:paused"];
  const targetCounts = useQueries({
    queries: targetIds.map((id) => ({
      queryKey: ["helpdesk", "queue", "count", "target", id],
      enabled: ready,
      staleTime: 30_000,
      queryFn: ({ signal }: { signal: AbortSignal }) =>
        api.get<{ total?: number }>("/api/v1/Ticket", { filters: JSON.stringify([["status", "in", ACTIVE], targetFilter(id)]), fields: "id", limit: 1, count: true }, signal).then((r) => r.total ?? 0),
    })),
  });
  const tagDefs = useTagDefs();
  const macros = useMacros(!!entity && can(entity.permissions, "write"));

  const list = useInfiniteQuery({
    queryKey: ["helpdesk", "queue", "list", state.queue, state.sort === "newest", state.q, state.priority, state.team, state.channel, state.assignee, state.tag, state.target, who.me, who.teams.join()],
    enabled: ready,
    initialPageParam: 0,
    queryFn: ({ pageParam, signal }) =>
      api.get<Page>(
        "/api/v1/Ticket",
        {
          filters: JSON.stringify([...queueFilters(state.queue, who), ...refineFilters(state)]),
          search: state.q || undefined,
          fields: FIELDS,
          titles: true,
          sort: `opened_at ${state.sort === "newest" ? "desc" : "asc"}`,
          limit: PAGE,
          offset: pageParam,
          count: true,
        },
        signal,
      ),
    getNextPageParam: (last, pages) => {
      const loaded = pages.reduce((n, p) => n + p.rows.length, 0);
      return (last.total ?? 0) > loaded ? loaded : undefined;
    },
  });
  const loaded = useMemo(() => list.data?.pages.flatMap((p) => p.rows) ?? [], [list.data]);
  const total = list.data?.pages[0]?.total ?? loaded.length;

  // Who has what: one light request, whatever the queue shown.
  const load = useQuery({
    queryKey: ["helpdesk", "queue", "load"],
    enabled: ready,
    staleTime: 30_000,
    queryFn: ({ signal }) =>
      api.get<{ rows: { assigned_to?: string | null }[] }>("/api/v1/Ticket", { filters: JSON.stringify([["status", "in", ACTIVE]]), fields: "assigned_to", limit: 500 }, signal).then((r) => workload(r.rows)),
  });

  // The filters a person saved.
  const prefs = useQuery({
    queryKey: ["helpdesk", "prefs"],
    staleTime: 5 * 60_000,
    queryFn: ({ signal }) =>
      api
        .get<{ prefs: Record<string, { value: unknown; version: number }> }>("/api/prefs", undefined, signal)
        .then((r) => r.prefs[PREF] ?? null)
        .catch(() => null),
  });
  const saved = useMemo(() => savedFrom(prefs.data?.value), [prefs.data]);
  const keep = async (next: SavedFilter[]): Promise<boolean> => {
    try {
      await api.put(`/api/prefs/${encodeURIComponent(PREF)}`, { value: { filters: next }, version: prefs.data?.version });
      await qc.invalidateQueries({ queryKey: ["helpdesk", "prefs"] });
      return true;
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        // Written from another tab meanwhile: what is there now is shown, and nothing is overwritten.
        void qc.invalidateQueries({ queryKey: ["helpdesk", "prefs"] });
        toast(tr("Your saved filters were changed in another tab. They are up to date now; try again."), "error");
      } else {
        toast(errorMessage(e), "error");
      }
      return false;
    }
  };

  // ── what is shown ──
  const shown = useMemo(() => sortTickets(loaded.map((t) => (overrides[t.id] ? { ...t, ...overrides[t.id] } : t)), state.sort), [loaded, overrides, state.sort]);
  const order = useMemo(() => shown.map((t) => t.id), [shown]);
  const carried = useMemo<Carried>(() => ({ order, search: sp.toString() }), [order, sp]);
  const numbers = useMemo(() => new Map(loaded.map((t) => [t.id, t.number ?? tr("A ticket")])), [loaded]);
  const writable = !!entity && can(entity.permissions, "write");
  const queue = QUEUES.find((q) => q.id === state.queue) ?? QUEUES[0];
  const refined = isRefined(state);
  const alreadySaved = saved.some((f) => isFilter(f, state));

  // Kept fresh as colleagues work: looked at again once the changes stop coming.
  const later = useQuiet(() => void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] }), 600);
  useLiveEntity("Ticket", useCallback(() => later(), [later]));
  // What was shown ahead of the server's word is dropped once the server has spoken.
  useEffect(() => setOverrides({}), [loaded]);
  // The keyboard's place and the selection only name tickets still on the list.
  useEffect(() => {
    setCursor((c) => (c && order.includes(c) ? c : null));
    setPicked((p) => (p.size && [...p].some((id) => !order.includes(id)) ? new Set([...p].filter((id) => order.includes(id))) : p));
  }, [order]);
  useEffect(() => {
    if (cursor) document.getElementById(`hd-row-${cursor}`)?.scrollIntoView({ block: "nearest" });
  }, [cursor]);
  // The words typed are put in the address once typing stops; a link that changes them wins.
  const typed = useDebounced(text, 300);
  useEffect(() => {
    if (typed === text && typed !== state.q) update({ q: typed });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [typed]);
  useEffect(() => setText(state.q), [state.q]);
  useEffect(() => {
    if (all.current) all.current.indeterminate = picked.size > 0 && picked.size < order.length;
  }, [picked, order]);

  const update = (patch: Partial<InboxState>) => {
    setSp(writeState({ ...state, ...patch }), { replace: true });
    setPicked(new Set());
  };

  // ── changes ──
  /** One ticket, shown changed at once and put back, with the server's reason, if it is refused. */
  const change = async (t: Ticket, values: Record<string, unknown>, done: string) => {
    setOverrides((o) => ({ ...o, [t.id]: { ...o[t.id], ...(values as Partial<Ticket>) } }));
    try {
      await api.patch(docPath("Ticket", t.id), values, { ifMatch: t.version });
      toast(done);
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
    } catch (e) {
      setOverrides((o) => without(o, t.id));
      const stale = e instanceof ApiError && (e.status === 409 || e.status === 412);
      toast(`${t.number ?? "The ticket"}: ${stale ? "someone else changed it first, and it shows their change now" : errorMessage(e)}`, "error");
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
    }
  };
  const take = (t: Ticket) => me && void change(t, { assigned_to: me.email.toLowerCase() }, tr("{value} is yours", { value: t.number ?? "Ticket" }));
  const hold = (t: Ticket) => void change(t, { status: "On Hold" }, tr("{value} is on hold", { value: t.number ?? "Ticket" }));

  /** Many tickets in one request, each in its own transaction: those that fail stay selected, with why. */
  const bulkOp = async (ids: string[], op: Record<string, unknown>, done: string): Promise<boolean> => {
    try {
      const res = await api.post<BulkResult>(docPath("Ticket", "_bulk"), { ids, ...op }, { idempotencyKey: newKey() });
      const told = describeBulk(done, res, (id) => numbers.get(id) ?? tr("A ticket"));
      toast(told.message, told.bad ? "error" : "success");
      setPicked(new Set(told.left));
      void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
      return true;
    } catch (e) {
      toast(errorMessage(e), "error");
      return false;
    }
  };
  const bulk = (ids: string[], values: Record<string, unknown>, done: string) => bulkOp(ids, { op: "update", values }, done);
  /** A macro on every selected ticket, each in its own transaction (all of its steps or none): said first if it writes to customers. */
  const applyMacro = async (m: Macro) => {
    const ids = chosen;
    const steps = await qc.fetchQuery({ queryKey: ["helpdesk", "macro", m.id], staleTime: 60_000, queryFn: ({ signal }) => api.get<Macro>(docPath("SupportMacro", m.id), undefined, signal) }).then((x) => x.steps ?? []).catch((e) => {
      toast(errorMessage(e), "error");
      return null;
    });
    if (!steps) return;
    const ok = await confirmDialog({
      title: tr("Apply {macro} to {value} tickets?", { macro: m.macro_name, value: ids.length }),
      message: `${steps.map(stepLine).join(" · ")}. ${sendsReply(steps) ? tr("It sends each customer a reply, which cannot be taken back.") : tr("Nothing is sent to customers.")} ${tr("A ticket that refuses a step is left as it was, and stays selected.")}`,
      confirmLabel: sendsReply(steps) ? tr("Apply and send the replies") : tr("Apply macro"),
    });
    if (ok) void bulkOp(ids, { op: "action", action: "apply_macro", params: { macro: m.id } }, tr("{macro} applied", { macro: m.macro_name }));
  };
  const chosen = [...picked];
  const target = (): string[] => (picked.size ? chosen : cursor ? [cursor] : []);
  const takeAll = () => {
    const ids = target();
    if (!me || !ids.length) return toast(tr("Pick a ticket first: j and k move, x selects"), "info");
    if (ids.length === 1) {
      const t = shown.find((x) => x.id === ids[0]);
      if (t) take(t);
    } else void bulk(ids, { assigned_to: me.email.toLowerCase() }, tr("Assigned to you"));
  };

  const saveView = async (viewName: string) => {
    const next = saveFilter(saved, viewName, state, newKey());
    if (!(await keep(next))) return false;
    toast(tr("Saved as {viewName}", { viewName: viewName.trim() }));
    return true;
  };
  const removeView = async (f: SavedFilter) => {
    if (await confirmDialog({ title: tr("Remove “{name}”?", { name: f.name }), message: tr("Only the saved filter goes. Its tickets stay where they are."), confirmLabel: tr("Remove"), danger: true })) void keep(saved.filter((x) => x.id !== f.id));
  };

  // ── the keyboard ──
  useEffect(() => {
    const on = (e: KeyboardEvent) => {
      if (isTyping(e.target as Element | null)) return;
      const c = command(e, KEYS);
      if (!c) return;
      // Arrows and Enter belong to whatever has the focus; with nothing focused they are the list's.
      if ((c === "open" && e.key === "Enter") || e.key === "ArrowDown" || e.key === "ArrowUp") {
        if (e.target !== document.body) return;
      }
      e.preventDefault();
      if (c.startsWith("queue-")) {
        const q = queues[Number(c.slice(6)) - 1];
        if (q) setSp(writeState({ ...START, queue: q.id }), { replace: true });
        return;
      }
      switch (c) {
        case "down":
        case "up":
          setCursor((cur) => step(order, cur, c === "down" ? 1 : -1));
          break;
        case "open":
          if (cursor) navigate(`/t/${cursor}`, { state: carried });
          break;
        case "reply":
          if (cursor && writable) navigate(`/t/${cursor}?reply`, { state: carried });
          break;
        case "pick":
        case "range":
          if (cursor) {
            setPicked((p) => toggle(p, order, cursor, anchor.current, c === "range"));
            anchor.current = cursor;
          }
          break;
        case "resolve":
          if (writable && target().length) setResolving(target());
          break;
        case "take":
          if (writable) takeAll();
          break;
        case "search":
          search.current?.focus();
          break;
        case "help":
          setHelp(true);
          break;
        case "clear":
          if (picked.size) setPicked(new Set());
          else setCursor(null);
          break;
      }
    };
    document.addEventListener("keydown", on);
    return () => document.removeEventListener("keydown", on);
  });

  if (index && !(entity && can(entity.permissions, "read"))) {
    return (
      <StatePanel kind="denied" title={tr("No tickets for you")}>
        {tr("Ask an administrator for a Support role.")}
      </StatePanel>
    );
  }

  const pick = (id: string, range: boolean) => {
    setPicked((p) => toggle(p, order, id, anchor.current, range));
    anchor.current = id;
    setCursor(id);
  };
  const body = list.error ? (
    <LoadFailed error={list.error} onRetry={() => void list.refetch()} />
  ) : !ready || list.isLoading ? (
    <ListSkeleton rows={8} />
  ) : !shown.length ? (
    refined ? (
      <StatePanel kind="no-matches" title={tr("No ticket matches")} clearLabel={tr("Clear the filters")} onClear={() => update({ q: "", priority: "", team: "", channel: "", assignee: "", tag: "", target: "" })}>
        {placed(tr("Nothing in {p1} fits these filters."), { p1: isolate(queue.label.toLowerCase()) })}
      </StatePanel>
    ) : (
      <StatePanel kind="empty" title={queue.empty} />
    )
  ) : (
    <>
      <ul className="hd-rows" aria-label={tr("Tickets")}>
        {shown.map((t) => (
          <Row
            key={t.id}
            t={t}
            now={now}
            me={who.me}
            who={name}
            picked={picked.has(t.id)}
            current={cursor === t.id}
            writable={writable}
            carried={carried}
            onPick={pick}
            onTake={take}
            onHold={hold}
            onResolve={(x) => setResolving([x.id])}
          />
        ))}
      </ul>
      {list.hasNextPage && (
        <div className="load-more">
          <Button loading={list.isFetchingNextPage} onClick={() => void list.fetchNextPage()}>
            {placed(tr("Show more ({p1} left)"), { p1: total - loaded.length })}
          </Button>
        </div>
      )}
    </>
  );

  return (
    <div className="hd-inbox">
      <div className="page-head">
        <div>
          <h1 className="page-title">{tr("Inbox")}</h1>
          <p className="page-sub" aria-live="polite">
            {list.data ? `${total} ${total === 1 ? "ticket" : "tickets"} · ${SORTS.find((s) => s.id === state.sort)?.label.toLowerCase()}` : " "}
          </p>
        </div>
        <div className="page-actions">
          <button type="button" className="icon-btn" aria-label={tr("Keyboard shortcuts")} title={tr("Keyboard shortcuts (?)")} onClick={() => setHelp(true)}>
            <Icon name="list" />
          </button>
          {entity && can(entity.permissions, "create") && (
            <Link className="btn btn-primary btn-md" to="e/Ticket/new">
              <Icon name="plus" />
              {tr("New ticket")}
            </Link>
          )}
        </div>
      </div>

      {!wide && (
        <button type="button" className="btn btn-default btn-md hd-more-toggle" aria-expanded={more} onClick={() => setMore(!more)}>
          {tr("Saved filters and who has what")}
        </button>
      )}
      <div className="hd-inbox-body">
        <Queues
          state={state}
          queues={queues}
          counts={countOf}
          saved={saved}
          onRemove={(f) => void removeView(f)}
          people={load.data ?? []}
          name={name}
          onPerson={(email) => update({ queue: "open", assignee: state.assignee.toLowerCase() === email.toLowerCase() ? "" : email })}
          showMore={wide || more}
          tags={(tagDefs.data ?? []).map((d) => ({ tag: d.tag_name, colour: d.colour }))}
          onTag={(tag) => update({ queue: "open", tag: state.tag.toLowerCase() === tag.toLowerCase() ? "" : tag })}
          targets={targetIds.map((id, i) => ({ id, label: TARGET_PICKS.find((p) => p.id === id)?.label ?? id, count: targetCounts[i]?.data }))}
          onTarget={(id) => update({ queue: "open", target: state.target === id ? "" : id })}
        />
        <section className="hd-list" aria-label={tr("The queue")}>
          <div className="hd-toolbar">
            <input
              ref={all}
              type="checkbox"
              className="hd-pick"
              aria-label={tr("Select every ticket shown")}
              checked={!!order.length && picked.size === order.length}
              disabled={!order.length || !writable}
              onChange={() => setPicked(picked.size === order.length ? new Set() : new Set(order))}
            />
            <input
              ref={search}
              className="input hd-search"
              type="search"
              aria-label={tr("Search tickets")}
              placeholder={tr("Search subject, customer, address…  ( / )")}
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <select className="select" aria-label={tr("Priority")} value={state.priority} onChange={(e) => update({ priority: e.target.value })}>
              <option value="">{tr("Any priority")}</option>
              {[...PRIORITIES].reverse().map((p) => (
                <option key={p}>{isolate(p)}</option>
              ))}
            </select>
            {(teams.data?.length ?? 0) > 0 && (
              <select className="select" aria-label={tr("Team")} value={state.team} onChange={(e) => update({ team: e.target.value })}>
                <option value="">{tr("Any team")}</option>
                {teams.data?.map((t) => (
                  <option key={t.id} value={t.id}>
                    {isolate(t.team_name)}
                  </option>
                ))}
              </select>
            )}
            <select className="select" aria-label={tr("Channel")} value={state.channel} onChange={(e) => update({ channel: e.target.value })}>
              <option value="">{tr("Any channel")}</option>
              {CHANNELS.map((c) => (
                <option key={c}>{isolate(c)}</option>
              ))}
            </select>
            {(tagDefs.data?.length ?? 0) > 0 && (
              <select className="select" aria-label={tr("Tag")} value={state.tag} onChange={(e) => update({ tag: e.target.value })}>
                <option value="">{tr("Any tag")}</option>
                {tagDefs.data?.map((d) => (
                  <option key={d.tag_name} value={d.tag_name}>
                    {isolate(d.tag_name)}
                  </option>
                ))}
              </select>
            )}
            <select className="select" aria-label={tr("Target state")} value={state.target} onChange={(e) => update({ target: e.target.value })}>
              <option value="">{tr("Any target state")}</option>
              {TARGET_PICKS.map((p) => (
                <option key={p.id} value={p.id}>
                  {isolate(p.label)}
                </option>
              ))}
            </select>
            <select className="select" aria-label={tr("Order")} value={state.sort} onChange={(e) => update({ sort: e.target.value as InboxState["sort"] })}>
              {SORTS.map((s) => (
                <option key={s.id} value={s.id}>
                  {isolate(s.label)}
                </option>
              ))}
            </select>
            {state.assignee && (
              <button type="button" className="chip" aria-label={tr("Showing {value}'s tickets. Show everyone's", { value: name(state.assignee) })} onClick={() => update({ assignee: "" })}>
                {isolate(name(state.assignee))}
                <Icon name="x" size={12} />
              </button>
            )}
            {refined && !alreadySaved && (
              <Button size="sm" onClick={() => setSaving(true)}>
                {tr("Save these filters")}
              </Button>
            )}
          </div>

          {picked.size > 0 && (
            <div className="hd-bulk" role="region" aria-label={tr("Actions for the selected tickets")}>
              <strong>{tr("{size} selected", { size: picked.size })}</strong>
              {me && (
                <Button size="sm" onClick={() => void bulk(chosen, { assigned_to: me.email.toLowerCase() }, tr("Assigned to you"))}>
                  {tr("Assign to me")}
                </Button>
              )}
              <Choose label={tr("Assign to…")}>
                {people.map((p) => (
                  <DropdownMenuItem key={p.id} onSelect={() => void bulk(chosen, { assigned_to: p.email.toLowerCase() }, tr("Assigned to {value}", { value: p.full_name || p.email }))}>
                    {p.full_name || p.email}
                  </DropdownMenuItem>
                ))}
              </Choose>
              <Choose label={tr("Priority")}>
                {[...PRIORITIES].reverse().map((p) => (
                  <DropdownMenuItem key={p} onSelect={() => void bulk(chosen, { priority: p }, tr("Priority {p}", { p }))}>
                    {isolate(p)}
                  </DropdownMenuItem>
                ))}
              </Choose>
              <Choose label={tr("Status")}>
                <DropdownMenuLabel>{tr("Set the status")}</DropdownMenuLabel>
                <DropdownMenuItem onSelect={() => void bulk(chosen, { status: "Open" }, "Opened")}>{tr("Open")}</DropdownMenuItem>
                <DropdownMenuItem onSelect={() => void bulk(chosen, { status: "On Hold" }, tr("Put on hold"))}>{tr("On hold")}</DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onSelect={() => setResolving(chosen)}>{tr("Resolve…")}</DropdownMenuItem>
              </Choose>
              {(tagDefs.data?.length ?? 0) > 0 && (
                <Choose label={tr("Tag")}>
                  <DropdownMenuLabel>{tr("Add the tag")}</DropdownMenuLabel>
                  {tagDefs.data?.map((d) => (
                    <DropdownMenuItem key={`add-${d.tag_name}`} onSelect={() => void bulkOp(chosen, { op: "add_tags", tags: [d.tag_name] }, tr("Tagged {tag}", { tag: d.tag_name }))}>
                      {isolate(d.tag_name)}
                    </DropdownMenuItem>
                  ))}
                  <DropdownMenuSeparator />
                  <DropdownMenuLabel>{tr("Remove the tag")}</DropdownMenuLabel>
                  {tagDefs.data?.map((d) => (
                    <DropdownMenuItem key={`rm-${d.tag_name}`} onSelect={() => void bulkOp(chosen, { op: "remove_tags", tags: [d.tag_name] }, tr("Untagged {tag}", { tag: d.tag_name }))}>
                      {isolate(d.tag_name)}
                    </DropdownMenuItem>
                  ))}
                </Choose>
              )}
              {(macros.data?.length ?? 0) > 0 && (
                <Choose label={tr("Macro")}>
                  {macros.data?.map((m) => (
                    <DropdownMenuItem key={m.id} onSelect={() => void applyMacro(m)}>
                      {isolate(m.macro_name)}
                    </DropdownMenuItem>
                  ))}
                </Choose>
              )}
              {writable && (
                <Button size="sm" disabled={picked.size < 2} title={picked.size < 2 ? tr("Select two or more tickets of one customer") : undefined} onClick={() => setMerging(true)}>
                  {tr("Merge…")}
                </Button>
              )}
              <span className="spacer" />
              <Button size="sm" variant="ghost" onClick={() => setPicked(new Set())}>
                {tr("Clear")}
              </Button>
            </div>
          )}
          {!writable && picked.size === 0 && entity && <Alert tone="info">{tr("You can read tickets here but not change them.")}</Alert>}
          {body}
        </section>
      </div>

      <ResolveDialog
        open={!!resolving}
        status="Resolved"
        what={resolving?.length === 1 ? (numbers.get(resolving[0]) ?? tr("the ticket")) : tr("{value} tickets", { value: resolving?.length ?? 0 })}
        onClose={() => setResolving(null)}
        onSubmit={(r) => bulk(resolving ?? [], { status: "Resolved", resolution_details: r.resolution }, "Resolved")}
      />
      <MergeDialog
        open={merging}
        tickets={shown.filter((x) => picked.has(x.id))}
        onClose={() => setMerging(false)}
        onDone={(r) => {
          toast(r.message ?? tr("Merged"));
          setPicked(new Set());
          void qc.invalidateQueries({ queryKey: ["helpdesk", "queue"] });
        }}
      />
      {saving && <SaveView saved={saved} onSave={saveView} onClose={() => setSaving(false)} />}
      <Shortcuts open={help} onClose={() => setHelp(false)} title={tr("Keyboard shortcuts")} rows={SHORTCUTS} />
    </div>
  );
}
