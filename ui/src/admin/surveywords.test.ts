import { describe, expect, it } from "vitest";
import { previewMessage, surveyLines } from "./surveywords";

describe("what the survey settings add up to", () => {
  it("says only that it is off, when it is", () => {
    expect(surveyLines({ survey_enabled: false, survey_reopen_at_or_below: 2 })).toEqual(["The survey is off: no one is asked to rate their help."]);
  });

  it("says when the link goes, how long it lasts, and what a low rating does", () => {
    const lines = surveyLines({ survey_enabled: true, survey_delay_hours: 24, survey_link_days: 30, survey_reopen_at_or_below: 2, survey_alert_at_or_below: 1, survey_alert_to: "lead@acme.test" });
    expect(lines).toEqual([
      "The customer is mailed a link 24 hours after the ticket is solved.",
      "The link works once, for 30 days, and needs no sign-in.",
      "A rating of 2 or below reopens a ticket that is only solved (a closed one stays closed).",
      "A rating of 1 or below tells the agent and lead@acme.test.",
    ]);
  });

  it("says plainly when a low rating does nothing", () => {
    const lines = surveyLines({ survey_enabled: true, survey_delay_hours: 0 });
    expect(lines[0]).toBe("The customer is mailed a link as soon as the ticket is solved.");
    expect(lines.slice(2)).toEqual(["A low rating does not reopen the ticket.", "A low rating tells no one."]);
  });
});

describe("the survey message with an example ticket", () => {
  const sample = { number: "TKT-2026-000123", subject: "Printer <jam>", name: "Ada", url: "https://x.test/rate" };
  it("puts the ticket in, keeps every part of the message, and never lets the ticket's words become markup", () => {
    const body = '<p>Hello{% if doc.contact_name %} {{ doc.contact_name }}{% endif %}, <b>{{ doc.number }}</b> ({{ doc.subject }}) <a href="{{ values.url|safe }}">rate</a> {{ doc.unknown }}</p>';
    expect(previewMessage(body, sample)).toBe('<p>Hello Ada, <b>TKT-2026-000123</b> (Printer &lt;jam&gt;) <a href="https://x.test/rate">rate</a> {{ doc.unknown }}</p>');
  });
});
