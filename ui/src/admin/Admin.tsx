// Administration of the support desk: one place for what a support manager sets up, a section at a time. The records
// themselves are the platform's (their forms are still at the records pages); these screens are for what a form
// cannot show well: the held mail, the order of rules, a macro's steps, the survey with its preview.

import { useQuery } from "@tanstack/react-query";
import { Link, useParams } from "react-router";
import { api, docPath, isolate, t } from "@ferrum/client";
import { useShellTitle } from "@ferrum/kit";
import { Addresses } from "./Addresses";
import { Agents } from "./Agents";
import { Agreements } from "./Agreements";
import { Brands } from "./Brands";
import { Inbound } from "./Inbound";
import { MacrosAdmin } from "./MacrosAdmin";
import { Routing } from "./Routing";
import { Survey } from "./Survey";
import { TagsAdmin } from "./TagsAdmin";
import { StatePanel } from "@ferrum/kit";

const SECTIONS = [
  { id: "addresses", get label() { return t("Support addresses"); }, Page: Addresses },
  { id: "brands", get label() { return t("Brands"); }, Page: Brands },
  { id: "agreements", get label() { return t("Service agreements"); }, Page: Agreements },
  { id: "inbound", get label() { return t("Inbound mail"); }, Page: Inbound },
  { id: "agents", get label() { return t("Agent profiles"); }, Page: Agents },
  { id: "tags", get label() { return t("Tags and tag rules"); }, Page: TagsAdmin },
  { id: "routing", get label() { return t("Assignment rules"); }, Page: Routing },
  { id: "macros", get label() { return t("Macros"); }, Page: MacrosAdmin },
  { id: "survey", get label() { return t("Satisfaction survey"); }, Page: Survey },
] as const;

export function AdminRoute() {
  const { section = "addresses" } = useParams();
  useShellTitle(t("Administration"));
  const current = SECTIONS.find((s) => s.id === section);
  // Mail waiting for a person to decide is the one count worth seeing before opening anything.
  const held = useQuery({
    queryKey: ["helpdesk", "held"],
    staleTime: 30_000,
    retry: false,
    queryFn: ({ signal }) => api.get<{ total?: number }>(docPath("InboundLog"), { filters: JSON.stringify([["outcome", "=", "Held"]]), fields: "id", limit: 1, count: true }, signal).then((r) => r.total ?? 0),
  });
  return (
    <div className="hd-admin">
      <div className="page-head">
        <div>
          <p className="crumbs">
            <Link to="/">{t("Inbox")}</Link>
          </p>
          <h1 className="page-title">{t("Administration")}</h1>
        </div>
      </div>
      <div className="hd-admin-body">
        <nav className="hd-queues" aria-label={t("Administration sections")}>
          <ul>
            {SECTIONS.map((s) => (
              <li key={s.id}>
                <Link to={`/admin/${s.id}`} aria-current={s.id === section ? "page" : undefined}>
                  <span>{isolate(s.label)}</span>
                  {s.id === "inbound" && !!held.data && <span className="hd-count">{held.data}</span>}
                </Link>
              </li>
            ))}
          </ul>
        </nav>
        <section className="hd-admin-main" aria-label={current?.label}>
          {current ? (
            <>
              <h2 className="hd-admin-title">{current.label}</h2>
              <current.Page />
            </>
          ) : (
            <StatePanel kind="empty" title={t("There is no such section")} action={<Link to="/admin/addresses">{t("Support addresses")}</Link>} />
          )}
        </section>
      </div>
    </div>
  );
}
