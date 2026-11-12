// The satisfaction survey: when it is sent, how long its link lasts, what a low rating does, with a preview of the
// message the customer is mailed (the template `helpdesk.survey`, as the workspace has edited it, with an example
// ticket) and of the rating page the link opens. The platform draws the real page; this shows what it asks.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router";
import { api, isolate, t } from "@ferrum/client";
import { Alert, Button, Spinner } from "@ferrum/kit";
import { SettingsCard } from "./Parts";
import type { FieldSpec } from "./form";
import { previewMessage, type SurveySettings, surveyLines } from "./surveywords";

const SAMPLE = () => ({ number: "TKT-2026-000123", subject: t("Printer jam on every page"), name: "Ada", url: "https://support.example.com/rate/example" });

interface TemplateRead {
  template: { languages: Record<string, { email?: { subject: string; body: string } }> };
}

/** The message as the workspace has it: its English email (else the first language that has one). A person who may not read templates is told where they are edited. */
function MessagePreview() {
  const q = useQuery({
    queryKey: ["helpdesk", "survey-template"],
    retry: false,
    staleTime: 60_000,
    queryFn: ({ signal }) => api.get<TemplateRead>("/api/admin/messages/templates/helpdesk.survey", undefined, signal),
  });
  const langs = q.data?.template.languages ?? {};
  const mail = langs.en?.email ?? Object.values(langs).find((l) => l.email)?.email;
  const sample = SAMPLE();
  return (
    <section aria-labelledby="hd-survey-message">
      <h3 id="hd-survey-message" className="hd-subhead">
        {t("The message the customer is mailed")}
      </h3>
      {q.isLoading ? (
        <Spinner label={t("Loading the message")} />
      ) : q.error || !mail ? (
        <p className="faint">
          {t("The wording could not be shown here: it is edited under")} <Link to="/customize/messages/helpdesk.survey">{t("Message templates")}</Link>.
        </p>
      ) : (
        <>
          <p>
            <strong>{t("Subject")}:</strong> {isolate(previewMessage(mail.subject, sample).replace(/&amp;/g, "&").replace(/&lt;/g, "<").replace(/&gt;/g, ">"))}
          </p>
          {/* The template's own HTML, with an example ticket in it, in a frame that runs nothing and loads nothing of the page's. */}
          <iframe className="hd-preview-frame" title={t("The survey message, with an example ticket")} sandbox="" srcDoc={previewMessage(mail.body, sample)} />
          <p className="faint">{t("An example ticket is put in. The wording is edited under")} <Link to="/customize/messages/helpdesk.survey">{t("Message templates")}</Link>.</p>
        </>
      )}
    </section>
  );
}

/** What the page the link opens asks: the action `rate` of the ticket (a rating from 1 to 5 and a comment). Nothing here can be sent. */
function RatingPreview() {
  return (
    <section aria-labelledby="hd-survey-page">
      <h3 id="hd-survey-page" className="hd-subhead">
        {t("The rating page the link opens")}
      </h3>
      <div className="hd-rating-sample" role="group" aria-label={t("Preview of the rating page: nothing here is sent")}>
        <strong>{t("Rate the help")}</strong>
        <p>{t("How did we do? (1 to 5)")}</p>
        <p className="faint">{t("1 is poor, 5 is excellent")}</p>
        <div className="row">
          {[1, 2, 3, 4, 5].map((n) => (
            <Button key={n} size="sm" disabled aria-label={t("{n} of 5", { n })}>
              {n}
            </Button>
          ))}
        </div>
        <label className="field-label" htmlFor="hd-sample-comment">
          {t("Comment")}
        </label>
        <textarea id="hd-sample-comment" className="textarea" rows={2} disabled value="" readOnly />
        <Button variant="primary" disabled>
          {t("Rate the help")}
        </Button>
      </div>
      <p className="faint">{t("An illustration of what the platform's page asks, from the ticket's Rate the Help action. It is the platform that draws the real page, and a link works once.")}</p>
    </section>
  );
}

export function Survey() {
  const spec: FieldSpec[] = [
    { name: "survey_enabled", label: t("Ask for a rating when a ticket is solved"), kind: "check" },
    { name: "survey_delay_hours", label: t("Send it after (hours)"), kind: "int", min: 0, max: 720, hint: t("0 sends it as soon as the ticket is solved.") },
    { name: "survey_link_days", label: t("The link lasts (days)"), kind: "int", min: 1, max: 90 },
    { name: "survey_reopen_at_or_below", label: t("Reopen the ticket at a rating of or below"), kind: "int", min: 0, max: 5, hint: t("0 means never. A closed ticket is final and stays closed.") },
    { name: "survey_alert_at_or_below", label: t("Tell the lead at a rating of or below"), kind: "int", min: 0, max: 5, hint: t("0 means never. The agent is told as well.") },
    { name: "survey_alert_to", label: t("The lead told about low ratings"), kind: "email" },
  ];
  return (
    <div className="stack">
      <SettingsCard title={t("Satisfaction survey")} spec={spec}>
        {(d) => {
          const num = (k: string) => (String(d[k] ?? "").trim() === "" ? null : Number(d[k]));
          const live: SurveySettings = {
            survey_enabled: !!d.survey_enabled,
            survey_delay_hours: num("survey_delay_hours"),
            survey_link_days: num("survey_link_days"),
            survey_reopen_at_or_below: num("survey_reopen_at_or_below"),
            survey_alert_at_or_below: num("survey_alert_at_or_below"),
            survey_alert_to: String(d.survey_alert_to ?? "").trim() || null,
          };
          return (
            <Alert tone="info">
              <ul className="hd-lines">
                {surveyLines(live).map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
            </Alert>
          );
        }}
      </SettingsCard>
      <div className="hd-preview-pair">
        <MessagePreview />
        <RatingPreview />
      </div>
    </div>
  );
}

