// The help centre as it draws once its articles are in the query cache, in the shape the routes answer them
// (Helpdesk `help.rs`: the portal's search, suggest, article and vote; the KbArticle record for the editor).

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter, Route, Routes } from "react-router";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { ArticleEditor, ArticleList } from "./HelpCentre";
import { PortalArticle, PortalArticles, Suggestions } from "./PortalHelp";

function draw(path: string, routes: React.ReactNode, seed: (qc: QueryClient) => void, perms = { read: "all", write: "all", create: "all", delete: "all" }) {
  const qc = new QueryClient();
  qc.setQueryData(["me"], { tenant: "t", user: { id: "u1", email: "kb@acme.test", full_name: "Kay", is_admin: false, roles: [] } });
  qc.setQueryData(["meta"], { entities: [{ name: "KbArticle", permissions: perms }] });
  seed(qc);
  return renderToStaticMarkup(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>{routes}</Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  (globalThis as { matchMedia?: unknown }).matchMedia = () => ({ matches: true, addEventListener() {}, removeEventListener() {} });
});
afterEach(() => {
  delete (globalThis as { matchMedia?: unknown }).matchMedia;
});

const article = { id: "a1", version: 3, title: "Reset your password", category: "c1", status: "Published", visibility: "Public", summary: "Four steps", content: "## Steps\n\nPress **Forgot password**.", views: 41, helpful_count: 7, not_helpful_count: 3, slug: "reset-your-password", published_at: "2026-11-05T09:00:00Z", _titles: { category: "Accounts" } };

describe("the article list", () => {
  const routes = <Route path="/help" element={<ArticleList />} />;
  it("shows each article with its state, who can read it and how it was received", () => {
    const html = draw("/help", routes, (qc) => qc.setQueryData(["helpdesk", "articles", "", ""], [article, { ...article, id: "a2", title: "Draft idea", status: "Draft", visibility: "Internal" }]));
    expect(html).toContain("Reset your password");
    expect(html).toContain('href="/help/a1"');
    expect(html).toContain("Accounts");
    expect(html).toContain("Published");
    expect(html).toContain("41 views");
    expect(html).toContain("7 of 10 found it helpful");
    expect(html).toContain("New article");
  });

  it("says in words when there are none, and offers the first to someone who may write", () => {
    const html = draw("/help", routes, (qc) => qc.setQueryData(["helpdesk", "articles", "", ""], []));
    expect(html).toContain("No articles yet");
    expect(html).toContain("Write the first");
    expect(draw("/help", routes, (qc) => qc.setQueryData(["helpdesk", "articles", "", ""], []), { read: "all", write: "none", create: "none", delete: "none" })).not.toContain("Write the first");
  });

  it("tells a person with no right to the articles", () => {
    expect(draw("/help", routes, () => {}, { read: "none", write: "none", create: "none", delete: "none" })).toContain("denied");
  });
});

describe("the article editor", () => {
  const routes = <Route path="/help/:id" element={<ArticleEditor />} />;
  const seed = (qc: QueryClient) => qc.setQueryData(["helpdesk", "article", "a1"], article);
  it("edits the article beside a preview of how it reads, and says who it reaches", () => {
    const html = draw("/help/a1", routes, seed);
    expect(html).toContain("Reset your password");
    expect(html).toContain("How it reads");
    expect(html).toContain("Press");
    expect(html).toContain("Published for everyone");
    expect(html).toContain("Archive");
    expect(html).toContain("7 of 10 found it helpful");
    expect(html).toContain("reset-your-password");
  });

  it("offers to publish a draft, and says no customer sees it", () => {
    const html = draw("/help/a1", routes, (qc) => qc.setQueryData(["helpdesk", "article", "a1"], { ...article, status: "Draft", published_at: null }));
    expect(html).toContain(">Publish<");
    expect(html).toContain("A draft: only agents see it");
    expect(html).toContain("Not published yet");
  });

  it("leaves it to be read by someone who may not change it", () => {
    const html = draw("/help/a1", routes, seed, { read: "all", write: "none", create: "none", delete: "none" });
    expect(html).toContain("You can read this article but not change it.");
    expect(html).not.toContain(">Publish<");
  });

  it("starts a new one as a draft for customers", () => {
    const html = draw("/help/new", routes, () => {});
    expect(html).toContain("New article");
    expect(html).toContain("A draft: only agents see it");
  });
});

describe("the portal's articles", () => {
  it("searches the articles a customer may read", () => {
    const html = draw("/articles", <Route path="/articles" element={<PortalArticles />} />, (qc) => qc.setQueryData(["helpdesk", "portal", "articles", ""], [{ id: "a1", title: "Reset your password", summary: "Four steps", category: "Accounts" }]));
    expect(html).toContain("Search the help articles");
    expect(html).toContain("Reset your password");
    expect(html).toContain('href="/articles/a1"');
    expect(html).toContain("Raise a ticket");
  });

  it("says when nothing is published yet", () => {
    expect(draw("/articles", <Route path="/articles" element={<PortalArticles />} />, (qc) => qc.setQueryData(["helpdesk", "portal", "articles", ""], []))).toContain("No articles yet");
  });

  it("reads an article and asks whether it helped", () => {
    const html = draw("/articles/a1", <Route path="/articles/:id" element={<PortalArticle />} />, (qc) => qc.setQueryData(["helpdesk", "portal", "article", "a1"], { id: "a1", title: "Reset your password", content: "Press **Forgot password**.", category: "Accounts" }));
    expect(html).toContain("Reset your password");
    expect(html).toContain("Did this help?");
    expect(html).toContain("Yes, it helped");
    expect(html).toContain("Still stuck?");
  });
});

describe("suggestions while a customer writes", () => {
  const draws = (subject: string, seed: (qc: QueryClient) => void = () => {}) => draw("/", <Route path="/" element={<Suggestions subject={subject} />} />, seed);
  it("offers the articles that fit the subject", () => {
    const html = draws("My printer is jammed", (qc) => qc.setQueryData(["helpdesk", "portal", "suggest", "My printer is jammed"], [{ id: "a9", title: "Clearing a paper jam", summary: "Open the rear tray" }]));
    expect(html).toContain("These articles may already answer it");
    expect(html).toContain("Clearing a paper jam");
    expect(html).toContain('href="/articles/a9"');
  });

  it("says nothing before there is a subject, and nothing when nothing fits", () => {
    expect(draws("")).toBe("");
    expect(draws("hi")).toBe("");
    expect(draws("Something odd", (qc) => qc.setQueryData(["helpdesk", "portal", "suggest", "Something odd"], []))).toBe("");
  });
});
