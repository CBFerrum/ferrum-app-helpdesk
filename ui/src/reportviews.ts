// The support reports as questions a desk asks, each opening the platform's report with sensible defaults: the
// report, how it is grouped, and the period. The reports are Helpdesk's own (`reports.rs`); the platform draws
// them, so a view is only a link that names what to ask. Pure, so it is tested.

import { t } from "@ferrum/client";

export type PeriodId = "7" | "30" | "90" | "month";

export const PERIODS: { id: PeriodId; label: string }[] = [
  { id: "7", get label() { return t("Last 7 days"); } },
  { id: "30", get label() { return t("Last 30 days"); } },
  { id: "90", get label() { return t("Last 90 days"); } },
  { id: "month", get label() { return t("This month"); } },
];

const iso = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/** The period as the reports' `from_date` and `to_date` (both days included), counted back from `today`. */
export function periodOf(id: PeriodId, today: Date): { from_date: string; to_date: string } {
  if (id === "month") return { from_date: iso(new Date(today.getFullYear(), today.getMonth(), 1)), to_date: iso(today) };
  const from = new Date(today.getFullYear(), today.getMonth(), today.getDate() - (Number(id) - 1));
  return { from_date: iso(from), to_date: iso(today) };
}

export interface ReportView {
  id: string;
  title: string;
  question: string;
  /** The report's name, as the platform lists it. */
  report: string;
  /** What it groups by first, then the other ways worth a click: the values the report's own Group By takes. Empty: a report with no Group By. */
  group: string;
  also: string[];
  /** Whether the report counts over a period (the backlog is how it stands now). */
  period: boolean;
  /** Said once beside it, where the figures need a reading. */
  note?: string;
}

export const VIEWS: ReportView[] = [
  {
    id: "targets",
    get title() { return t("Targets met and missed"); },
    get question() { return t("How many first responses and resolutions were on time, by agreement?"); },
    report: "support_sla_compliance",
    group: "SLA",
    also: ["Team", "Priority", "Agent", "Customer"],
    period: true,
    get note() { return t("Counted from the platform's target states: met, met late and breached."); },
  },
  {
    id: "times",
    get title() { return t("Time to respond and to resolve"); },
    get question() { return t("How long do customers wait, in working hours, against what was promised?"); },
    report: "support_response_times",
    group: "Priority",
    also: ["Team", "Agent", "Ticket Type"],
    period: true,
    get note() { return t("In working hours of the calendar each ticket's agreement counts on, else the workspace's (round the clock for a ticket without a target); time the clock was stopped is left out."); },
  },
  {
    id: "backlog",
    get title() { return t("Backlog by age"); },
    get question() { return t("How many open tickets are there, and how old are they?"); },
    report: "support_backlog",
    group: "Status",
    also: ["Team", "Agent", "Priority"],
    period: false,
  },
  {
    id: "volume",
    get title() { return t("Volume by channel and tag"); },
    get question() { return t("Where do tickets come from, and what are they about?"); },
    report: "support_volume",
    group: "Channel",
    also: ["Tag", "Team", "Customer", "Month"],
    period: true,
    get note() { return t("A ticket with several tags is counted under each, so a tag's total is not the number of tickets."); },
  },
  {
    id: "reopens",
    get title() { return t("Reopen rate"); },
    get question() { return t("How often does a solved ticket come back?"); },
    report: "support_reopens",
    group: "Agent",
    also: ["Team", "Ticket Type", "Month"],
    period: true,
  },
  {
    id: "satisfaction",
    get title() { return t("Satisfaction by agent and team"); },
    get question() { return t("How do customers rate the help they got?"); },
    report: "support_customer_satisfaction",
    group: "Agent",
    also: ["Team", "Month", "Priority"],
    period: true,
    get note() { return t("Counted from the date the rating was given, and only for tickets that were rated."); },
  },
  {
    id: "articles",
    get title() { return t("Help centre by article"); },
    get question() { return t("Which articles are read, and do they help?"); },
    report: "support_articles",
    group: "",
    also: [],
    period: false,
    get note() { return t("Views and votes are counted from the portal and from the public help centre. It is every article, as they stand now."); },
  },
];

/** Where a view opens: the platform's report, run at once, with the group and the period. */
export function viewLink(v: ReportView, group: string, period: PeriodId, today: Date): string {
  const sp = new URLSearchParams({ run: "1" });
  if (group) sp.set("group_by", group);
  if (v.period) for (const [k, val] of Object.entries(periodOf(period, today))) sp.set(k, val);
  return `/r/${v.report}?${sp.toString()}`;
}
