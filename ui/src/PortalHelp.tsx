// The help centre in the customer's portal: search the articles they may read, read one and say whether it helped,
// and the articles offered while they write a request. Everything comes from /api/app/helpdesk/portal/…, which
// answers only Published articles that are Public or for Customers Only. A view is counted when one is read, and
// a vote once a day for each article.

import { useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { Link, useParams } from "react-router";
import { api, ApiError, errorMessage, isolate, t } from "@ferrum/client";
import { Button, Icon, ListSkeleton, Markdown, ReaderSkeleton, Spinner, StatePanel, toast, useDebounced, useShellTitle } from "@ferrum/kit";
import { suggestQuery } from "./articles";
import { LoadFailed } from "./states";

const BASE = "/api/app/helpdesk/portal";

export interface ArticleSummary {
  id: string;
  title: string;
  summary?: string | null;
  category?: string | null;
}
interface ArticleFull extends ArticleSummary {
  content: string;
  views?: number;
}

function Hits({ articles }: { articles: ArticleSummary[] }) {
  return (
    <ul className="hd-tickets" data-ferrum-nocard>
      {articles.map((a) => (
        <li key={a.id}>
          <Link className="hd-ticket" to={`/articles/${a.id}`}>
            <span className="hd-ticket-main">
              <span className="hd-ticket-subject">{isolate(a.title)}</span>
              {a.summary && <span className="faint">{isolate(a.summary)}</span>}
            </span>
            {a.category && <span className="faint">{isolate(a.category)}</span>}
          </Link>
        </li>
      ))}
    </ul>
  );
}

export function PortalArticles() {
  useShellTitle(t("Help articles"));
  const [text, setText] = useState("");
  const q = useDebounced(text.trim(), 300);
  const found = useQuery({
    queryKey: ["helpdesk", "portal", "articles", q],
    retry: false,
    queryFn: ({ signal }) => api.get<{ articles: ArticleSummary[] }>(`${BASE}/articles`, q ? { q } : {}, signal).then((r) => r.articles),
  });
  return (
    <div className="stack hd-portal">
      <div className="page-head">
        <div>
          <p className="crumbs">
            <Link to="/">{t("Your tickets")}</Link>
          </p>
          <h1 className="page-title">{t("Help articles")}</h1>
          <p className="page-sub">{t("Answers to common questions. Not what you need? Raise a ticket.")}</p>
        </div>
        <Link className="btn btn-primary btn-md" to="/new">
          {t("New ticket")}
        </Link>
      </div>
      <input className="input" type="search" aria-label={t("Search the help articles")} placeholder={t("Search the help articles")} value={text} onChange={(e) => setText(e.target.value)} />
      {found.error ? (
        <LoadFailed error={found.error} onRetry={() => void found.refetch()} />
      ) : !found.data ? (
        <ListSkeleton rows={4} />
      ) : !found.data.length ? (
        q ? (
          <StatePanel kind="no-matches" title={t("No article matches")} action={<Link to="/new">{t("Raise a ticket")}</Link>}>
            {t("Nothing matches “{q}”.", { q })}
          </StatePanel>
        ) : (
          <StatePanel kind="empty" title={t("No articles yet")} />
        )
      ) : (
        <Hits articles={found.data} />
      )}
    </div>
  );
}

export function PortalArticle() {
  const { id = "" } = useParams();
  const [voted, setVoted] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [told, setTold] = useState<string | null>(null);
  // Reading counts a view, so it is read once per visit and not again on every focus.
  const a = useQuery({
    queryKey: ["helpdesk", "portal", "article", id],
    retry: false,
    staleTime: Infinity,
    queryFn: ({ signal }) => api.get<ArticleFull>(`${BASE}/articles/${id}`, {}, signal),
  });
  useShellTitle(a.data?.title ?? t("Help articles"));
  const vote = async (helpful: boolean) => {
    if (busy || voted !== null) return;
    setBusy(true);
    try {
      await api.post(`${BASE}/articles/${id}/vote`, { helpful });
      setVoted(helpful);
    } catch (e) {
      // Once a day for each article: the server's own words say so.
      setTold(errorMessage(e));
      if (e instanceof ApiError && e.status === 429) setVoted(helpful);
      else toast(errorMessage(e), "error");
    } finally {
      setBusy(false);
    }
  };
  if (a.error) {
    if (a.error instanceof ApiError && a.error.status === 404) {
      return (
        <StatePanel kind="empty" title={t("This article is not available")} action={<Link to="/articles">{t("Search the help articles")}</Link>}>
          {t("It may have been taken down, or it is not for customers.")}
        </StatePanel>
      );
    }
    return <LoadFailed error={a.error} onRetry={() => void a.refetch()} />;
  }
  if (!a.data) return <ReaderSkeleton paragraphs={4} />;
  return (
    <article className="stack hd-portal">
      <p className="crumbs">
        <Link to="/articles">{t("Help articles")}</Link>
        {a.data.category && (
          <>
            <Icon name="chevronRight" size={12} />
            <span>{isolate(a.data.category)}</span>
          </>
        )}
      </p>
      <h1 className="page-title">{isolate(a.data.title)}</h1>
      <div className="card">
        <div className="card-body">
          <Markdown source={a.data.content} />
        </div>
      </div>
      <section aria-labelledby="hd-helpful" className="hd-card">
        <h3 id="hd-helpful">{t("Did this help?")}</h3>
        {voted === null ? (
          <div className="row">
            <Button loading={busy} disabled={busy} onClick={() => void vote(true)}>
              {t("Yes, it helped")}
            </Button>
            <Button disabled={busy} onClick={() => void vote(false)}>
              {t("No")}
            </Button>
          </div>
        ) : (
          <p role="status">{told ? isolate(told) : t("Thank you. We count every answer.")}</p>
        )}
        <p className="faint">
          {t("Still stuck?")} <Link to="/new">{t("Raise a ticket")}</Link>
        </p>
      </section>
    </article>
  );
}

/** The articles that fit what a customer is writing, shown under the subject so that an answer can come before a ticket does. */
export function Suggestions({ subject }: { subject: string }) {
  const q = useDebounced(suggestQuery(subject), 500);
  const found = useQuery({
    queryKey: ["helpdesk", "portal", "suggest", q],
    enabled: !!q,
    retry: false,
    staleTime: 60_000,
    queryFn: ({ signal }) => api.get<{ articles: ArticleSummary[] }>(`${BASE}/suggest`, { subject: q }, signal).then((r) => r.articles),
  });
  // Suggestions are a help and never a wall: a failure, or nothing found, shows nothing and the request goes on.
  if (!q || found.error || (found.data && !found.data.length)) return null;
  return (
    <section className="hd-suggest" aria-labelledby="hd-suggest-h" aria-live="polite">
      <h3 id="hd-suggest-h">{t("These articles may already answer it")}</h3>
      {found.isLoading ? (
        <Spinner label={t("Looking for articles")} />
      ) : (
        <ul>
          {found.data?.map((a) => (
            <li key={a.id}>
              <Link to={`/articles/${a.id}`} target="_blank" rel="noopener">
                {isolate(a.title)}
              </Link>
              {a.summary && <span className="faint"> {isolate(a.summary)}</span>}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
