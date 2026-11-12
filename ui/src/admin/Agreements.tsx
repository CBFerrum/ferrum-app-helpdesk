// Service agreements: which working calendar each one's targets are counted on. The targets themselves, who an
// agreement applies to and its pauses stay in the agreement's own form (one click from each row); what this adds
// is the pick of a calendar, because the form has only a name to type. An agreement with no calendar counts on
// the workspace's, `helpdesk_hours`.

import { Link } from "react-router";
import { isolate, t } from "@ferrum/client";
import { Badge } from "@ferrum/kit";
import { type Column, AdminTable, useAdminRows } from "./Parts";
import { useCalendarNames, WORKSPACE_CALENDAR } from "./calendars";
import type { FieldSpec } from "./form";

const FIELDS = "sla_name,enabled,customer,customer_group,ticket_type,team,calendar";
const SORT = "sla_name asc";

export function Agreements() {
  const names = useCalendarNames();
  const rows = useAdminRows("ServiceLevelAgreement", FIELDS, SORT);
  // A calendar an administrator deleted since stays in the list, so saving the agreement does not quietly drop it.
  const gone = [...new Set((rows.data ?? []).map((r) => String(r.calendar ?? "")).filter((c) => c && names && !names.includes(c)))];
  const spec: FieldSpec[] = [
    { name: "sla_name", label: t("Name"), kind: "text", required: true, max: 140 },
    names
      ? {
          name: "calendar",
          label: t("Working calendar"),
          kind: "select",
          options: [...names, ...gone],
          optionLabels: Object.fromEntries(gone.map((g) => [g, t("{name} (no longer exists)", { name: g })])),
          noneLabel: t("The workspace's calendar ({name})", { name: WORKSPACE_CALENDAR }),
          hint: t("The hours, time zone and holidays this agreement's targets are counted on. Calendars are made under Admin, Calendars."),
        }
      : { name: "calendar", label: t("Working calendar"), kind: "text", max: 64, hint: t("The name of a working calendar. Empty: the workspace's calendar ({name}).", { name: WORKSPACE_CALENDAR }) },
  ];
  const columns: Column[] = [
    { key: "name", label: t("Agreement"), cell: (r) => isolate(String(r.sla_name ?? "")) },
    {
      key: "applies",
      label: t("Applies to"),
      cell: (r) => {
        const to = ["customer", "customer_group", "team", "ticket_type"].map((k) => r._titles?.[k]).filter(Boolean);
        return to.length ? to.map((x) => isolate(String(x))).join(", ") : <span className="faint">{t("Everything else")}</span>;
      },
    },
    {
      key: "calendar",
      label: t("Working calendar"),
      cell: (r) => (r.calendar ? isolate(String(r.calendar)) : <span className="faint">{t("The workspace's calendar ({name})", { name: WORKSPACE_CALENDAR })}</span>),
    },
    { key: "enabled", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("On")}</Badge> },
  ];
  return (
    <AdminTable
      entity="ServiceLevelAgreement"
      noun={t("agreement")}
      fields={FIELDS}
      sort={SORT}
      spec={spec}
      columns={columns}
      hideAdd
      note={<p className="faint">{t("Each agreement's targets are counted in working time on its calendar. The targets, and who an agreement applies to, are in the agreement's own form.")}</p>}
      rowActions={(r) => (
        <Link className="btn btn-ghost btn-sm" to={`/e/ServiceLevelAgreement/${r.id}`}>
          {t("Targets and criteria")}
        </Link>
      )}
      empty={t("No service agreement yet. Tickets have no targets until one applies.")}
    />
  );
}
