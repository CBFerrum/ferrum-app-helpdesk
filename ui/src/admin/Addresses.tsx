// Support addresses: what customers write to, the brand each is known by, the team its mail goes to, and what to do
// with mail from a sender no customer owns (a setting of the whole workspace).
// Mail itself arrives through the Mail app; replies leave by the Mail account named here.

import { Badge } from "@ferrum/kit";
import { isolate, t } from "@ferrum/client";
import { PRIORITIES } from "../inbox";
import { type Column, AdminTable, SettingsCard } from "./Parts";
import type { FieldSpec } from "./form";

const UNKNOWN = ["Open Ticket", "Hold for Review", "Ignore"] as const;

export function Addresses() {
  const unknown = (): FieldSpec[] => [
    {
      name: "unknown_senders",
      label: t("Mail from a sender who is nobody's customer"),
      kind: "select",
      required: true,
      options: UNKNOWN,
      optionLabels: { "Open Ticket": t("Open a ticket with the sender as the requester"), "Hold for Review": t("Hold it in the inbound log for staff to open or discard"), Ignore: t("Ignore it, and note it in the log") },
      hint: t("Applies to mail for any support address that is not writing on a ticket."),
    },
    { name: "inbound_per_hour", label: t("Most mail taken from one sender in an hour"), kind: "int", min: 0, max: 10000, hint: t("More than this is logged and not turned into tickets. 0 means no limit.") },
    { name: "inbound_log_days", label: t("Keep the inbound log (days)"), kind: "int", min: 1, max: 3650 },
  ];
  const spec: FieldSpec[] = [
    { name: "address", label: t("Address"), kind: "email", required: true, hint: t("What customers write to, such as support@example.com. Kept in lower case.") },
    { name: "label", label: t("Brand"), kind: "text", max: 140, hint: t("What the customer knows this address by.") },
    { name: "team", label: t("Team"), kind: "link", to: "SupportTeam", hint: t("New tickets from this address go to this team; its assignment rules then pick the agent.") },
    { name: "default_priority", label: t("Priority of what arrives"), kind: "select", required: true, options: PRIORITIES },
    { name: "ticket_type", label: t("Type of what arrives"), kind: "link", to: "TicketType" },
    { name: "mail_account", label: t("Mail account replies go out from"), kind: "text", max: 140, hint: t("The name of an account in the Mail app, so the customer's answer comes back to the address they wrote to. Empty: Mail's default account.") },
    { name: "enabled", label: t("Takes mail"), kind: "check" },
  ];
  const columns: Column[] = [
    { key: "address", label: t("Address"), cell: (r) => isolate(String(r.address ?? "")) },
    { key: "label", label: t("Brand"), cell: (r) => (r.label ? isolate(String(r.label)) : <span className="faint">{t("None")}</span>) },
    { key: "team", label: t("Team"), cell: (r) => (r._titles?.team ? isolate(r._titles.team) : <span className="faint">{t("No team")}</span>) },
    { key: "priority", label: t("Priority"), cell: (r) => isolate(String(r.default_priority ?? "Medium")) },
    { key: "enabled", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("Takes mail")}</Badge> },
  ];
  return (
    <div className="stack">
      <SettingsCard title={t("Unknown senders")} intro={t("What happens to mail whose sender is no customer's address or contact.")} spec={unknown()} />
      <AdminTable
        entity="SupportAddress"
        noun={t("support address")}
        fields="address,label,team,default_priority,ticket_type,mail_account,enabled"
        sort="address asc"
        spec={spec}
        columns={columns}
        defaults={{ default_priority: "Medium", enabled: true }}
        empty={t("No support address yet. Add one so that mail to it becomes tickets.")}
      />
    </div>
  );
}
