import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Reports } from "./Reports";

describe("the reports menu", () => {
  const html = renderToStaticMarkup(
    <QueryClientProvider client={new QueryClient()}>
      <MemoryRouter>
        <Reports />
      </MemoryRouter>
    </QueryClientProvider>,
  );

  it("asks the six questions of a desk, each opening a report", () => {
    for (const q of ["Targets met and missed", "Time to respond and to resolve", "Backlog by age", "Volume by channel and tag", "Reopen rate", "Satisfaction by agent and team"]) expect(html).toContain(q);
    expect(html).toContain("/r/support_backlog?run=1&amp;group_by=Status");
    expect(html).toContain("/r/support_volume?run=1&amp;group_by=Tag");
    expect(html).toContain("from_date=");
  });

  it("lets the period be chosen, and says the backlog is as it stands now", () => {
    expect(html).toContain("Last 30 days");
    expect(html).toContain("As it stands now");
  });
});
