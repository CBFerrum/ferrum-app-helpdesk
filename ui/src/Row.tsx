// One ticket in the inbox: who it is from and what was last said, how long it has waited, where its
// service level stands and who has it, with the actions an agent reaches for.

import { memo } from "react";
import { Link } from "react-router";
import { Badge, Button, Icon, statusTone } from "@ferrum/kit";
import { DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger } from "@ferrum/kit/primitives";
import { type Carried, dueAt, dueLabel, initials, lastSpeaker, requester, type Ticket, waitLabel } from "./inbox";
import { t as tr } from "@ferrum/client";

const PRIORITY: Record<string, "red" | "amber" | "neutral"> = { Urgent: "red", High: "amber", Low: "neutral" };
const SLA: Record<string, "red" | "amber"> = { Breached: "red", "At Risk": "amber" };

export interface RowProps {
  t: Ticket;
  now: number;
  /** The signed-in agent's address, and the name an address goes by. */
  me: string;
  who: (email: string) => string;
  picked: boolean;
  /** The keyboard's place in the list. */
  current: boolean;
  writable: boolean;
  carried: Carried;
  onPick: (id: string, range: boolean) => void;
  onTake: (t: Ticket) => void;
  onHold: (t: Ticket) => void;
  onResolve: (t: Ticket) => void;
}

export const Row = memo(function Row({ t, now, me, who, picked, current, writable, carried, onPick, onTake, onHold, onResolve }: RowProps) {
  const due = dueLabel(dueAt(t), now);
  const wait = waitLabel(t, now);
  const speaker = lastSpeaker(t);
  const owner = t.assigned_to ? who(t.assigned_to) : null;
  const mine = (t.assigned_to ?? "").toLowerCase() === me.toLowerCase();
  return (
    <li id={`hd-row-${t.id}`} className="hd-row" data-picked={picked || undefined} data-current={current || undefined} data-priority={t.priority ?? "Medium"}>
      <input type="checkbox" className="hd-pick" checked={picked} aria-label={`Select ${t.number ?? "ticket"}`} onChange={(e) => onPick(t.id, (e.nativeEvent as MouseEvent).shiftKey)} />
      <Link className="hd-row-main" to={`/t/${t.id}`} state={carried}>
        <span className="hd-row-subject">{t.subject || "No subject"}</span>
        <span className="hd-row-meta">{[t.number, requester(t), t.channel && t.channel !== "Email" ? t.channel : null].filter(Boolean).join(" · ")}</span>
        {t.last_message_preview && (
          <span className="hd-row-preview">
            {speaker === "Team" && <span className="faint">{tr("Team:")} </span>}
            {t.last_message_preview}
          </span>
        )}
      </Link>
      <span className="hd-row-chips">
        {t.priority && PRIORITY[t.priority] && <Badge tone={PRIORITY[t.priority]}>{t.priority}</Badge>}
        {t.status && t.status !== "Open" && <Badge tone={statusTone(t.status)}>{t.status}</Badge>}
        {t.sla_status && SLA[t.sla_status] && <Badge tone={SLA[t.sla_status]}>{t.sla_status}</Badge>}
        {due && <span className={due.startsWith("overdue") ? "hd-overdue" : "faint"}>{due}</span>}
        {wait && <span className="faint">{wait}</span>}
      </span>
      <span className="hd-row-who">
        {owner ? (
          <span className="hd-avatar" role="img" aria-label={mine ? "You have this ticket" : `${owner} has this ticket`} title={mine ? "You have it" : `${owner} has it`}>
            {initials(owner)}
          </span>
        ) : writable ? (
          <Button size="sm" onClick={() => onTake(t)} title={tr("Assign it to me (a)")}>
            {tr("Take")}
          </Button>
        ) : (
          <span className="faint">{tr("Nobody")}</span>
        )}
      </span>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button type="button" className="icon-btn" aria-label={`Actions for ${t.number ?? "this ticket"}`}>
            <Icon name="chevronDown" size={14} />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end">
          <DropdownMenuItem asChild>
            <Link to={`/t/${t.id}`} state={carried}>
              {tr("Open")}
            </Link>
          </DropdownMenuItem>
          {writable && (
            <DropdownMenuItem asChild>
              <Link to={`/t/${t.id}?reply`} state={carried}>
                {tr("Reply")}
              </Link>
            </DropdownMenuItem>
          )}
          {writable && <DropdownMenuSeparator />}
          {writable && !mine && <DropdownMenuItem onSelect={() => onTake(t)}>{tr("Assign to me")}</DropdownMenuItem>}
          {writable && t.status !== "On Hold" && <DropdownMenuItem onSelect={() => onHold(t)}>{tr("Put on hold")}</DropdownMenuItem>}
          {writable && <DropdownMenuItem onSelect={() => onResolve(t)}>{tr("Resolve…")}</DropdownMenuItem>}
        </DropdownMenuContent>
      </DropdownMenu>
    </li>
  );
});
