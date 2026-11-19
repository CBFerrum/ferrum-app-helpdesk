// A workspace's own statuses: a name the team reads ("Waiting on supplier"), inside one of the four working stages that
// queues, service targets and reports read. Choosing one on a ticket moves it to that stage.

import { Badge } from "@ferrum/kit";
import { isolate, t } from "@ferrum/client";
import { STATUS_TONES } from "../tones";
import { AdminTable, type Column } from "./Parts";
import type { FieldSpec } from "./form";

export const STAGES = ["Open", "Replied", "On Hold", "Resolved"] as const;

/** A stage's name, as people read it (the stored id stays English). */
export function stageLabel(stage: string): string {
  switch (stage) {
    case "Open":
      return t("Open");
    case "Replied":
      return t("Replied");
    case "On Hold":
      return t("On Hold");
    case "Resolved":
      return t("Resolved");
    default:
      return stage;
  }
}

export function StatusesAdmin() {
  const spec: FieldSpec[] = [
    { name: "status_name", label: t("Name"), kind: "text", required: true, max: 60, hint: t("What the team reads, such as Waiting on supplier. Not a stage's own name.") },
    {
      name: "category",
      label: t("Stage"),
      kind: "select",
      required: true,
      options: STAGES,
      optionLabels: Object.fromEntries(STAGES.map((s) => [s, stageLabel(s)])),
      hint: t("Where a ticket with this status stands for queues, service targets and reports. It cannot change while tickets have the status."),
    },
    { name: "enabled", label: t("On"), kind: "check" },
    { name: "description", label: t("What it is for"), kind: "textarea", rows: 2 },
  ];
  const columns: Column[] = [
    { key: "name", label: t("Name"), cell: (r) => isolate(String(r.status_name ?? "")) },
    { key: "stage", label: t("Stage"), cell: (r) => <Badge tone={STATUS_TONES[String(r.category)] ?? "neutral"}>{isolate(stageLabel(String(r.category ?? "")))}</Badge> },
    { key: "on", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("On")}</Badge> },
  ];
  return (
    <AdminTable
      entity="TicketStatus"
      noun={t("status")}
      fields="status_name,category,enabled,description"
      sort="status_name asc"
      spec={spec}
      columns={columns}
      defaults={{ enabled: true }}
      empty={t("No statuses of your own yet. Tickets use the five built-in stages.")}
    />
  );
}
