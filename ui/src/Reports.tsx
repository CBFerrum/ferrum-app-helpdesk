// The support reports as the questions a desk asks: each card opens the platform's report with the grouping and period
// that answer it, and the other ways to group it are a click away. The reports themselves are Helpdesk's, drawn
// by the platform (so they can be downloaded as CSV there); this is the menu in front of them.

import { Link } from "react-router";
import { isolate, t } from "@ferrum/client";
import { useShellTitle } from "@ferrum/kit";
import { useRemembered } from "./hooks";
import { PERIODS, type PeriodId, VIEWS, viewLink } from "./reportviews";

export function Reports() {
  useShellTitle(t("Reports"));
  const [period, setPeriod] = useRemembered<PeriodId>("helpdesk:report-period", "30");
  const today = new Date();
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <p className="crumbs">
            <Link to="/">{t("Inbox")}</Link>
          </p>
          <h1 className="page-title">{t("Reports")}</h1>
          <p className="page-sub">{t("What the desk is asking of itself. Each opens the report with the period chosen here.")}</p>
        </div>
        <div className="page-actions">
          <label className="field-label" htmlFor="hd-period">
            {t("Period")}
          </label>
          <select id="hd-period" className="select" value={period} onChange={(e) => setPeriod(e.target.value as PeriodId)}>
            {PERIODS.map((p) => (
              <option key={p.id} value={p.id}>
                {isolate(p.label)}
              </option>
            ))}
          </select>
        </div>
      </div>
      <ul className="hd-views" data-ferrum-nocard>
        {VIEWS.map((v) => (
          <li key={v.id} className="hd-card">
            <h2 className="hd-admin-h">{v.title}</h2>
            <p>{v.question}</p>
            {v.note && <p className="faint">{v.note}</p>}
            <p className="row">
              <Link className="btn btn-primary btn-md" to={viewLink(v, v.group, period, today)}>
                {v.group ? t("Open by {group}", { group: v.group }) : t("Open the report")}
              </Link>
              {!v.period && <span className="faint">{t("As it stands now")}</span>}
            </p>
            {v.also.length > 0 && (
              <p className="row" aria-label={t("Other ways to group it")}>
                {v.also.map((g) => (
                  <Link key={g} className="chip" to={viewLink(v, g, period, today)}>
                    {isolate(g)}
                  </Link>
                ))}
              </p>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
