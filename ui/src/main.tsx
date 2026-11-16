// Helpdesk's own interface (Ferrum docs/plan/apps-ui.md §13, U5) at /helpdesk/: the agent inbox on
// its home, each ticket's whole conversation in its workspace, and the rest of Helpdesk's records
// (knowledge base, warranty claims, maintenance, SLAs, teams) as lists and forms in the records
// frame, with Helpdesk's modules in the sidebar under the platform's bar. The same release draws
// its page of the workspace's portal, "Your tickets" at /portal/tickets/ (§9.1, U7).

import { useEffect } from "react";
import { Link, Outlet, useRouteError } from "react-router";
import { bootstrap, isolate, shell, t } from "@ferrum/client";
import { StatePanel } from "@ferrum/kit";
import { mountApp, RequireAuth, setFrameMode } from "@ferrum/records/frame";
import { LoadFailed } from "./states";
import { PortalNewTicket, PortalTicket, PortalTickets } from "./Portal";
import "./styles.css";

/** A page of the records frame, loaded when first opened. */
const page = (name: "FormView" | "ListView" | "ReportView") => () =>
  import("./records").then((m) => ({ Component: m[name] }));

/** Helpdesk's packages: the sidebar shows their modules. */
const HELPDESK_PACKAGES = ["helpdesk", "helpdesk_erp"];

function RouteError() {
  return <LoadFailed error={useRouteError()} onRetry={() => window.location.reload()} />;
}

function NotFound({ back = t("Go to the inbox") }: { back?: string }) {
  return (
    <StatePanel kind="empty" title={t("Page not found")} action={<Link to="/">{isolate(back)}</Link>} />
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
      { path: "articles", lazy: () => import("./PortalHelp").then((m) => ({ Component: m.PortalArticles })) },
      { path: "articles/:id", lazy: () => import("./PortalHelp").then((m) => ({ Component: m.PortalArticle })) },
      { path: "*", get element() { return <NotFound back={t("Go to your tickets")} />; } },
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
      // The inbox and the workspace are Helpdesk's two big screens: the first script holds only the frame.
      { index: true, lazy: () => import("./Home").then((m) => ({ Component: m.HelpdeskHome })) },
      { path: "t/:id", lazy: () => import("./Ticket").then((m) => ({ Component: m.TicketRoute })) },
      { path: "reports", lazy: () => import("./Reports").then((m) => ({ Component: m.Reports })) },
      { path: "help", lazy: () => import("./HelpCentre").then((m) => ({ Component: m.ArticleList })) },
      { path: "help/:id", lazy: () => import("./HelpCentre").then((m) => ({ Component: m.ArticleEditor })) },
      { path: "admin/:section?", lazy: () => import("./admin/Admin").then((m) => ({ Component: m.AdminRoute })) },
      { path: "e/:entity", lazy: page("ListView") },
      { path: "e/:entity/:id", lazy: page("FormView") },
      { path: "r/:name", lazy: page("ReportView") },
      { path: "*", element: <NotFound /> },
    ],
  },
];

mountApp({
  catalogs: { ar: () => import("./locales/ar.json"), hi: () => import("./locales/hi.json") },
  ...(onPortal ? { route: PORTAL, routes: portalRoutes } : { route: "helpdesk", routes: agentRoutes }),
});
