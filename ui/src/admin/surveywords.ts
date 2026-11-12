// The satisfaction survey, as a manager sees it before it goes out: what the settings add up to in words, and the
// message with an example ticket put in. The platform draws the real message and the real rating page; these
// are what they say, which is why the preview names itself as one. Pure, so it is tested.

import { t } from "@ferrum/client";

export interface SurveySettings {
  survey_enabled?: boolean;
  survey_delay_hours?: number | null;
  survey_link_days?: number | null;
  survey_reopen_at_or_below?: number | null;
  survey_alert_at_or_below?: number | null;
  survey_alert_to?: string | null;
}

const n = (v: number | null | undefined, otherwise: number) => (typeof v === "number" ? v : otherwise);

/** What the settings do, one line each; a survey that is off says so alone. */
export function surveyLines(s: SurveySettings): string[] {
  if (!s.survey_enabled) return [t("The survey is off: no one is asked to rate their help.")];
  const lines = [
    n(s.survey_delay_hours, 1) === 0
      ? t("The customer is mailed a link as soon as the ticket is solved.")
      : t("The customer is mailed a link {hours} hours after the ticket is solved.", { hours: n(s.survey_delay_hours, 1) }),
    t("The link works once, for {days} days, and needs no sign-in.", { days: n(s.survey_link_days, 30) }),
  ];
  const reopen = n(s.survey_reopen_at_or_below, 0);
  lines.push(reopen > 0 ? t("A rating of {rating} or below reopens a ticket that is only solved (a closed one stays closed).", { rating: reopen }) : t("A low rating does not reopen the ticket."));
  const alert = n(s.survey_alert_at_or_below, 0);
  lines.push(
    alert > 0
      ? s.survey_alert_to
        ? t("A rating of {rating} or below tells the agent and {to}.", { rating: alert, to: s.survey_alert_to })
        : t("A rating of {rating} or below tells the agent.", { rating: alert })
      : t("A low rating tells no one."),
  );
  return lines;
}

export interface Sample {
  number: string;
  subject: string;
  name: string;
  url: string;
}

const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

/** A message template's text with an example ticket put in. A `{% … %}` block is dropped and its content kept, so the
 *  preview shows every part the message can have. A field it does not know stays as written. */
export function previewMessage(template: string, s: Sample): string {
  const values: Record<string, string> = { "doc.number": s.number, "doc.subject": s.subject, "doc.contact_name": s.name, "values.url": s.url };
  return template
    .replace(/\{%[\s\S]*?%\}/g, "")
    .replace(/\{\{\s*([a-z_.]+)\s*(?:\|[a-z_]+\s*)*\}\}/gi, (whole, key: string) => (key in values ? escape(values[key]) : whole));
}
