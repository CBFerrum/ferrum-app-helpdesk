// Helpdesk's own interface (Ferrum docs/plan/apps-ui.md §13, U5) at /helpdesk/: the agent inbox on
// its home, each ticket's conversation in the ticket view, and the rest of Helpdesk's records
// (knowledge base, warranty claims, maintenance, SLAs, teams) as lists and forms in the records
// frame, with Helpdesk's modules in the sidebar under the platform's bar. The same release draws
// its page of the workspace's portal, "Your tickets" at /portal/tickets/ (§9.1, U7).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrictMode, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { createBrowserRouter, Link, Outlet, RouterProvider, useRouteError } from "react-router";
import { bootstrap, routeBase, shell } from "@ferrum/client";
import { ConfirmHost, EmptyState, ErrorState, Toaster } from "@ferrum/kit";
import { RequireAuth, setFrameMode } from "@ferrum/records/frame";
import { HelpdeskHome } from "./Home";
import { PortalNewTicket, PortalTicket, PortalTickets } from "./Portal";
import { TicketPage } from "./Ticket";
import "./styles.css";

/** A page of the records frame, loaded when first opened. */
const page = (name: "FormView" | "ListView" | "ReportView" | "AppSettingsPage" | "ScreenPage") => () =>
  import("./records").then((m) => ({ Component: m[name] }));

/** Helpdesk's packages: the sidebar shows their modules. */
const HELPDESK_PACKAGES = ["helpdesk", "helpdesk_erp"];

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: false, refetchOnWindowFocus: false, staleTime: 30_000 } },
});

function RouteError() {
  return <ErrorState error={useRouteError()} onRetry={() => window.location.reload()} />;
}

function NotFound({ back = "Go to the inbox" }: { back?: string }) {
  return (
    <EmptyState title="Page not found">
      <Link to="/">{back}</Link>
    </EmptyState>
  );
}

/** Every page tells the shell it started. */
function Started() {
  useEffect(() => shell().ready(), []);
  return null;
}

/** The portal's page, when the platform serves this release there. */
const PORTAL = "portal/tickets";
const onPortal = bootstrap()?.app?.entry === PORTAL;

if (!onPortal) setFrameMode("app", HELPDESK_PACKAGES);

const portalRoutes = [
  {
    path: "/",
    element: (
      <main className="hd-portal-page">
        <Started />
        <Outlet />
      </main>
    ),
    errorElement: <RouteError />,
    children: [
      { index: true, element: <PortalTickets /> },
      { path: "new", element: <PortalNewTicket /> },
      { path: "t/:id", element: <PortalTicket /> },
      { path: "*", element: <NotFound back="Go to your tickets" /> },
    ],
  },
];

const agentRoutes = [
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
      { path: "e/:entity", lazy: page("ListView") },
      { path: "e/:entity/:id", lazy: page("FormView") },
      { path: "r/:name", lazy: page("ReportView") },
      { path: "apps/:app/settings", lazy: page("AppSettingsPage") },
      { path: "x/:app/:page", lazy: page("ScreenPage") },
      { path: "*", element: <NotFound /> },
    ],
  },
];

const router = onPortal
  ? createBrowserRouter(portalRoutes, { basename: routeBase(PORTAL) })
  : createBrowserRouter(agentRoutes, { basename: routeBase("helpdesk") });

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
      <Toaster />
      <ConfirmHost />
    </QueryClientProvider>
  </StrictMode>,
);
