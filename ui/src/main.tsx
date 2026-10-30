// Helpdesk's own interface (Ferrum docs/plan/apps-ui.md §13, U5) at /helpdesk/: the agent inbox on
// its home, each ticket's conversation in the ticket view, and the rest of Helpdesk's records
// (knowledge base, warranty claims, maintenance, SLAs, teams) as lists and forms in the records
// frame, with Helpdesk's modules in the sidebar under the platform's bar.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, Link, RouterProvider, useRouteError } from "react-router";
import { routeBase, shell } from "@ferrum/client";
import { ConfirmHost, EmptyState, ErrorState, Toaster } from "@ferrum/kit";
import { FormView, ListView, ReportView } from "@ferrum/records";
import { AppSettingsPage, RequireAuth, ScreenPage, setFrameMode } from "@ferrum/records/frame";
import { HelpdeskHome } from "./Home";
import { TicketPage } from "./Ticket";
import "./styles.css";

/** Helpdesk's packages: the sidebar shows their modules. */
const HELPDESK_PACKAGES = ["helpdesk", "helpdesk_erp"];

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 } },
});

function RouteError() {
  return <ErrorState error={useRouteError()} onRetry={() => window.location.reload()} />;
}

function NotFound() {
  return (
    <EmptyState title="Page not found">
      <Link to="/">Go to the inbox</Link>
    </EmptyState>
  );
}

/** Every page tells the shell it started. */
function Started() {
  useEffect(() => shell().ready(), []);
  return null;
}

setFrameMode("app", HELPDESK_PACKAGES);

const router = createBrowserRouter(
  [
    {
      path: "/",
      element: (
        <>
          <Started />
          <RequireAuth />
        </>
      ),
      errorElement: <RouteError />,
      children: [
        { index: true, element: <HelpdeskHome /> },
        { path: "t/:id", element: <TicketPage /> },
        { path: "e/:entity", element: <ListView /> },
        { path: "e/:entity/:id", element: <FormView /> },
        { path: "r/:name", element: <ReportView /> },
        { path: "apps/:app/settings", element: <AppSettingsPage /> },
        { path: "x/:app/:page", element: <ScreenPage /> },
        { path: "*", element: <NotFound /> },
      ],
    },
  ],
  { basename: routeBase("helpdesk") },
);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster />
      <ConfirmHost />
    </QueryClientProvider>
  </StrictMode>,
);
