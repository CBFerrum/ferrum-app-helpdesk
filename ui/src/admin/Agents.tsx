// Agent profiles: the skills and languages an agent covers, the shift calendar they are on duty by, and the
// signature their replies end with. Assignment rules read the skills and the shift; replies read the signature.

import { useQuery } from "@tanstack/react-query";
import { api, isolate, t } from "@ferrum/client";
import { Badge } from "@ferrum/kit";
import { AdminTable, type Column } from "./Parts";
import type { FieldSpec } from "./form";

export function Agents() {
  // The shift calendar is named from the workspace's calendars; a person who may not list them types the name.
  const calendars = useQuery({
    queryKey: ["helpdesk", "calendars"],
    staleTime: 5 * 60_000,
    retry: false,
    queryFn: ({ signal }) => api.get<{ calendars: { name: string }[] }>("/api/admin/calendars", undefined, signal).then((r) => r.calendars.map((c) => c.name)),
  });
  const spec: FieldSpec[] = [
    { name: "agent", label: t("Agent's address"), kind: "email", required: true, hint: t("The address they sign in with.") },
    { name: "display_name", label: t("Name shown"), kind: "text", max: 140 },
    { name: "skills", label: t("Skills and languages"), kind: "text", max: 500, hint: t("Comma-separated: billing, hardware, en, hi. A rule that asks for a skill, or for the ticket's language, gives tickets only to agents who list it. A language covers its variants: hi covers hi-IN.") },
    calendars.data
      ? { name: "shift_calendar", label: t("Shift calendar"), kind: "select", options: calendars.data, hint: t("The agent is given tickets only while this calendar is open. None: always on shift.") }
      : { name: "shift_calendar", label: t("Shift calendar"), kind: "text", max: 64, hint: t("The name of a working calendar. The agent is given tickets only while it is open. Empty: always on shift.") },
    { name: "signature", label: t("Signature"), kind: "markdown", max: 2000, rows: 5, hint: t("Markdown. It ends every public reply this agent emails, never an internal note.") },
  ];
  const columns: Column[] = [
    { key: "agent", label: t("Agent"), cell: (r) => isolate(String(r.display_name || r.agent || "")) },
    {
      key: "skills",
      label: t("Skills and languages"),
      cell: (r) =>
        String(r.skills ?? "")
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
          .map((s) => (
            <Badge key={s} tone="neutral">
              {isolate(s)}
            </Badge>
          )),
    },
    { key: "shift", label: t("Shift calendar"), cell: (r) => (r.shift_calendar ? isolate(String(r.shift_calendar)) : <span className="faint">{t("Always on shift")}</span>) },
    { key: "sig", label: t("Signature"), cell: (r) => (r.signature ? t("Yes") : <span className="faint">{t("None")}</span>) },
  ];
  return (
    <AdminTable
      entity="AgentProfile"
      noun={t("agent profile")}
      fields="agent,display_name,skills,shift_calendar,signature"
      sort="agent asc"
      spec={spec}
      columns={columns}
      empty={t("No agent profile yet. Agents without one have no skills, are always on shift and sign with their words only.")}
    />
  );
}
