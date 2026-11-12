import { describe, expect, it } from "vitest";
import { periodOf, VIEWS, viewLink } from "./reportviews";

const today = new Date(2026, 10, 12);

describe("the report periods", () => {
  it("count back from today, both days included", () => {
    expect(periodOf("7", today)).toEqual({ from_date: "2026-11-06", to_date: "2026-11-12" });
    expect(periodOf("30", today)).toEqual({ from_date: "2026-10-14", to_date: "2026-11-12" });
    expect(periodOf("month", today)).toEqual({ from_date: "2026-11-01", to_date: "2026-11-12" });
    expect(periodOf("90", new Date(2026, 0, 10)).from_date).toBe("2025-10-13");
  });
});

describe("the support report views", () => {
  it("open the platform's report, run at once, grouped and over the period", () => {
    const targets = VIEWS.find((v) => v.id === "targets")!;
    expect(viewLink(targets, "Team", "30", today)).toBe("/r/support_sla_compliance?run=1&group_by=Team&from_date=2026-10-14&to_date=2026-11-12");
  });

  it("leave the period off the backlog, which is how things stand now", () => {
    const backlog = VIEWS.find((v) => v.id === "backlog")!;
    expect(viewLink(backlog, "Status", "7", today)).toBe("/r/support_backlog?run=1&group_by=Status");
  });

  it("open the article report with no grouping, since it has none", () => {
    expect(viewLink(VIEWS.find((v) => v.id === "articles")!, "", "30", today)).toBe("/r/support_articles?run=1");
  });

  it("cover the six questions and the help centre's articles, each on one of the server's report sources, groupable as it says", () => {
    expect(VIEWS.map((v) => v.report)).toEqual(["support_sla_compliance", "support_response_times", "support_backlog", "support_volume", "support_reopens", "support_customer_satisfaction", "support_articles"]);
    // The values the server's Group By takes (Helpdesk reports.rs GROUPS, and Tag for the volume report).
    const known = ["Status", "Priority", "Team", "Agent", "Ticket Type", "Channel", "Customer", "SLA", "Month", "Tag"];
    for (const v of VIEWS) for (const g of [v.group, ...v.also].filter(Boolean)) expect(known).toContain(g);
    expect(VIEWS.find((v) => v.id === "volume")!.also).toContain("Tag");
  });
});
