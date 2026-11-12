// Assignment rules, in the order they are tried: the first enabled rule whose condition fits a new ticket (or a
// reopened one) picks the team and an agent in it. A rule can ask for a skill, skip agents who are away or off
// shift, and name a fallback for when nobody can take it. The order is the rules' `priority`, highest first.

import { useState } from "react";
import { api, docPath, errorMessage, isolate, t } from "@ferrum/client";
import { Badge, Button, toast } from "@ferrum/kit";
import { AdminTable, type Column, type Row, SettingsCard, useRefresh } from "./Parts";
import type { FieldSpec } from "./form";
import { moved, renumber } from "./order";

const RULE_TYPES = ["Round Robin", "Load Balancing"] as const;
const SKILLS = ["No", "Ticket Language", "Fixed Skill"] as const;

export function Routing() {
  const refresh = useRefresh();
  const [busy, setBusy] = useState(false);
  const spec: FieldSpec[] = [
    { name: "rule_name", label: t("Rule"), kind: "text", required: true, max: 140 },
    { name: "team", label: t("Team it places tickets in"), kind: "link", to: "SupportTeam", required: true },
    { name: "condition", label: t("When"), kind: "code", rows: 3, hint: t("A condition on the ticket, for example doc.priority == 'Urgent' && doc.channel == 'Email'. Empty: every ticket.") },
    { name: "rule_type", label: t("How an agent is chosen"), kind: "select", required: true, options: RULE_TYPES, optionLabels: { "Round Robin": t("Take turns through the team"), "Load Balancing": t("The agent with the fewest open tickets") } },
    {
      name: "needs_skill",
      label: t("Skill needed"),
      kind: "select",
      required: true,
      options: SKILLS,
      optionLabels: { No: t("None"), "Ticket Language": t("The ticket's language"), "Fixed Skill": t("A skill named here") },
      hint: t("Only agents whose profile lists it are given the ticket."),
    },
    { name: "skill", label: t("Skill"), kind: "text", required: true, max: 40, when: (d) => d.needs_skill === "Fixed Skill" },
    { name: "only_available", label: t("Skip agents who are away or off shift"), kind: "check" },
    { name: "fallback_agent", label: t("Fallback agent"), kind: "email", hint: t("Gets the ticket when nobody in the team can take it. Empty: it stays with the team, unassigned.") },
    { name: "enabled", label: t("On"), kind: "check" },
  ];
  const columns: Column[] = [
    { key: "rule", label: t("Rule"), cell: (r) => isolate(String(r.rule_name ?? "")) },
    { key: "priority", label: t("Priority"), cell: (r) => String(r.priority ?? "") },
    { key: "team", label: t("Team"), cell: (r) => isolate(r._titles?.team ?? "") },
    { key: "when", label: t("When"), cell: (r) => (r.condition ? <code>{isolate(String(r.condition))}</code> : <span className="faint">{t("Every ticket")}</span>) },
    { key: "how", label: t("Chooses"), cell: (r) => isolate(String(r.rule_type ?? "")) },
    { key: "skill", label: t("Skill"), cell: (r) => (r.needs_skill === "Fixed Skill" ? isolate(String(r.skill ?? "")) : r.needs_skill === "Ticket Language" ? t("Ticket language") : <span className="faint">{t("None")}</span>) },
    { key: "fallback", label: t("Fallback"), cell: (r) => (r.fallback_agent ? isolate(String(r.fallback_agent)) : <span className="faint">{t("None")}</span>) },
    { key: "state", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("On")}</Badge> },
  ];

  /** Earlier or later: the order becomes numbers, written only where they changed. */
  const move = async (rows: Row[], from: number, to: number) => {
    if (busy) return;
    const order = moved(rows, from, to);
    const changes = renumber(order.map((r) => r.id), Object.fromEntries(rows.map((r) => [r.id, Number(r.priority)])));
    if (!changes.length) return;
    setBusy(true);
    try {
      for (const c of changes) {
        const row = rows.find((r) => r.id === c.id);
        await api.patch(docPath("TicketAssignmentRule", c.id), { priority: c.priority }, { ifMatch: Number(row?.version) });
      }
    } catch (e) {
      toast(errorMessage(e), "error");
    } finally {
      refresh("TicketAssignmentRule");
      setBusy(false);
    }
  };

  return (
    <div className="stack">
      <SettingsCard
        title={t("When a ticket is placed again")}
        spec={[
          { name: "route_reopened", label: t("Place a reopened ticket again when its agent cannot take it"), kind: "check" },
          { name: "reassign_unanswered_hours", label: t("Move a ticket nobody has answered after (hours)"), kind: "int", min: 0, max: 720, hint: t("To the next agent its rule picks. 0 means never.") },
          { name: "max_reassignments", label: t("At most this many moves for one ticket"), kind: "int", min: 0, max: 20 },
        ]}
      />
      <AdminTable
        entity="TicketAssignmentRule"
        noun={t("assignment rule")}
        fields="rule_name,team,condition,rule_type,priority,enabled,needs_skill,skill,only_available,fallback_agent"
        sort="priority desc"
        spec={spec}
        columns={columns}
        defaults={{ rule_type: "Round Robin", needs_skill: "No", only_available: true, enabled: true, priority: 1 }}
        note={<p className="faint">{t("Rules are tried from the top, the highest priority first. The first enabled rule whose condition fits, and whose team fits the ticket, places it. Earlier and Later renumber the priorities for you.")}</p>}
        rowActions={(r, rows) => {
          const i = rows.findIndex((x) => x.id === r.id);
          return (
            <>
              <Button size="sm" variant="ghost" disabled={busy || i <= 0} aria-label={t("Try {name} earlier", { name: String(r.rule_name) })} onClick={() => void move(rows, i, i - 1)}>
                {t("Earlier")}
              </Button>
              <Button size="sm" variant="ghost" disabled={busy || i >= rows.length - 1} aria-label={t("Try {name} later", { name: String(r.rule_name) })} onClick={() => void move(rows, i, i + 1)}>
                {t("Later")}
              </Button>
            </>
          );
        }}
        empty={t("No assignment rule yet. Tickets stay unassigned until someone takes them.")}
      />
    </div>
  );
}
