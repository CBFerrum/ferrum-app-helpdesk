// Support addresses: what customers write to, the brand each is known by, the team its mail goes to, and what to do
// with mail from a sender no customer owns (a setting of the whole workspace).
// Mail itself arrives through the Mail app; replies leave by the Mail account named here.

import { Badge } from "@ferrum/kit";
import { isolate, t } from "@ferrum/client";
import { PRIORITIES } from "../inbox";
import { type Column, AdminTable, SettingsCard } from "./Parts";
import type { FieldSpec } from "./form";

const UNKNOWN = ["Open Ticket", "Hold for Review", "Ignore"] as const;
const UNVERIFIED = ["Hold Failed", "Hold All Unverified", "Accept"] as const;

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
  // Mail from someone the desk knows, that the sender's own mail server did not vouch for.
  const unverified = (): FieldSpec[] => [
    {
      name: "unverified_senders",
      label: t("Mail from a known sender that fails authentication"),
      kind: "select",
      required: true,
      options: UNVERIFIED,
      optionLabels: {
        "Hold Failed": t("Hold mail whose checks failed for staff to read first; take mail with no result and mark it"),
        "Hold All Unverified": t("Hold every mail that was not verified, including mail with no result"),
        Accept: t("Take every mail, and mark the ones that were not verified"),
      },
      hint: t("For a known customer, or the requester or a copy of the ticket the mail answers. A mail that is taken and was not verified shows “Sender not verified” on the message. Hold Failed suits most workspaces; hold all only if your mail server always adds a result."),
    },
  ];
  const spec: FieldSpec[] = [
    { name: "address", label: t("Address"), kind: "email", required: true, hint: t("What customers write to, such as support@example.com. Kept in lower case.") },
    { name: "brand", label: t("Brand"), kind: "link", to: "SupportBrand", hint: t("Whose name, signature and wrapping replies to this address's tickets carry. Brands are made under Brands.") },
    { name: "label", label: t("Label"), kind: "text", max: 140, hint: t("A note for people. What the address was called before there were brands.") },
    { name: "team", label: t("Team"), kind: "link", to: "SupportTeam", hint: t("New tickets from this address go to this team; its assignment rules then pick the agent.") },
    { name: "default_priority", label: t("Priority of what arrives"), kind: "select", required: true, options: PRIORITIES },
    { name: "ticket_type", label: t("Type of what arrives"), kind: "link", to: "TicketType" },
    { name: "mail_account", label: t("Mail account replies go out from"), kind: "text", max: 140, hint: t("The name of an account in the Mail app, so the customer's answer comes back to the address they wrote to. Empty: Mail's default account.") },
    { name: "enabled", label: t("Takes mail"), kind: "check" },
  ];
  const columns: Column[] = [
    { key: "address", label: t("Address"), cell: (r) => isolate(String(r.address ?? "")) },
    { key: "brand", label: t("Brand"), cell: (r) => (r._titles?.brand ? isolate(r._titles.brand) : <span className="faint">{t("None")}</span>) },
    { key: "team", label: t("Team"), cell: (r) => (r._titles?.team ? isolate(r._titles.team) : <span className="faint">{t("No team")}</span>) },
    { key: "priority", label: t("Priority"), cell: (r) => isolate(String(r.default_priority ?? "Medium")) },
    { key: "enabled", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("Takes mail")}</Badge> },
    {
      key: "mail",
      label: t("Registered with Mail"),
      cell: (r) =>
        r.enabled === false ? (
          <span className="faint">{t("Not needed while it is off")}</span>
        ) : r.mail_registered ? (
          <Badge tone="green">{t("Registered")}</Badge>
        ) : (
          <>
            <Badge tone="red">{t("Mail does not hand it mail")}</Badge>
            {r.registration_note ? <span className="faint"> {isolate(String(r.registration_note))}</span> : null}
          </>
        ),
    },
  ];
  return (
    <div className="stack">
      <SettingsCard title={t("Unknown senders")} intro={t("What happens to mail whose sender is no customer's address or contact.")} spec={unknown()} />
      <SettingsCard title={t("Senders that are not verified")} intro={t("What happens to mail that claims to be from a known sender but that the mail server could not vouch for.")} spec={unverified()} />
      <AdminTable
        entity="SupportAddress"
        noun={t("support address")}
        fields="address,label,brand,team,default_priority,ticket_type,mail_account,enabled,mail_registered,registration_note"
        sort="address asc"
        spec={spec}
        columns={columns}
        defaults={{ default_priority: "Medium", enabled: true }}
        empty={t("No support address yet. Add one so that mail to it becomes tickets.")}
      />
    </div>
  );
}
