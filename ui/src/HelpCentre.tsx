// The help centre's articles, for the people who write them: a list with what each is, who it reaches and how
// it was received, and an editor with a preview of the page. An article is Draft, Published or Archived and is Internal,
// for Customers, or Public; saying who it will reach is part of the screen. The public list and page are the
// platform's (the published list `help-centre`): this edits the articles they show and does not draw them.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { Link, useNavigate, useParams } from "react-router";
import { api, ApiError, bootstrap, docPath, errorMessage, formatMoment, isolate, newKey, t } from "@ferrum/client";
import { Alert, Badge, Button, confirmDialog, ListSkeleton, Markdown, ReaderSkeleton, StatePanel, toast, useShellTitle } from "@ferrum/kit";
import { useDebounced } from "@ferrum/kit";
import { Control, type Row, useRights } from "./admin/Parts";
import { bodyOf, draftOf, type FieldSpec, problems } from "./admin/form";
import { helpfulShare, reach, STATUS_TONES, STATUSES, VISIBILITIES } from "./articles";
import { LoadFailed } from "./states";

const FIELDS = "title,category,status,visibility,summary,published_at,views,helpful_count,not_helpful_count,slug";

const SPEC = (): FieldSpec[] => [
  { name: "title", label: t("Title"), kind: "text", required: true, max: 200 },
  { name: "category", label: t("Category"), kind: "link", to: "KbCategory", required: true },
  { name: "visibility", label: t("Who can read it"), kind: "select", required: true, options: VISIBILITIES, optionLabels: { Internal: t("Agents only"), "Customers Only": t("Customers signed in to the portal"), Public: t("Everyone, on the public help centre") } },
  { name: "status", label: t("State"), kind: "select", required: true, options: STATUSES, optionLabels: { Draft: t("Draft"), Published: t("Published"), Archived: t("Archived") } },
  { name: "summary", label: t("Summary"), kind: "text", max: 300, hint: t("One line, shown in search results and in the list of articles.") },
  { name: "content", label: t("Article"), kind: "markdown", required: true, rows: 18, hint: t("Markdown works.") },
];

export function ArticleList() {
  useShellTitle(t("Help centre"));
  const rights = useRights("KbArticle");
  const [status, setStatus] = useState("");
  const [text, setText] = useState("");
  const q = useDebounced(text.trim(), 300);
  const list = useQuery({
    queryKey: ["helpdesk", "articles", status, q],
    queryFn: ({ signal }) =>
      api
        .get<{ rows: Row[] }>(docPath("KbArticle"), { fields: FIELDS, sort: "title asc", limit: 200, titles: true, search: q || undefined, filters: status ? JSON.stringify([["status", "=", status]]) : undefined }, signal)
        .then((r) => r.rows),
  });
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <p className="crumbs">
            <Link to="/">{t("Inbox")}</Link>
          </p>
          <h1 className="page-title">{t("Help centre")}</h1>
          <p className="page-sub">{t("Articles for agents and customers. Published, public ones are on the help centre's public list.")}</p>
        </div>
        {rights.create && (
          <Link className="btn btn-primary btn-md" to="/help/new">
            {t("New article")}
          </Link>
        )}
      </div>
      <div className="hd-toolbar">
        <input className="input hd-search" type="search" aria-label={t("Search articles")} placeholder={t("Search articles")} value={text} onChange={(e) => setText(e.target.value)} />
        <select className="select" aria-label={t("State")} value={status} onChange={(e) => setStatus(e.target.value)}>
          <option value="">{t("Any state")}</option>
          {STATUSES.map((s) => (
            <option key={s}>{s}</option>
          ))}
        </select>
      </div>
      {list.error ? (
        <LoadFailed error={list.error} onRetry={() => void list.refetch()} />
      ) : !list.data ? (
        <ListSkeleton rows={6} />
      ) : !list.data.length ? (
        q || status ? (
          <StatePanel kind="no-matches" title={t("No article matches")} clearLabel={t("Clear the filters")} onClear={() => (setText(""), setStatus(""))} />
        ) : (
          <StatePanel kind="empty" title={t("No articles yet")} action={rights.create ? <Link to="/help/new">{t("Write the first")}</Link> : undefined} />
        )
      ) : (
        <div className="hd-table-wrap">
          <table className="table hd-table">
            <thead>
              <tr>
                <th scope="col">{t("Article")}</th>
                <th scope="col">{t("Category")}</th>
                <th scope="col">{t("State")}</th>
                <th scope="col">{t("Who can read it")}</th>
                <th scope="col">{t("Read")}</th>
              </tr>
            </thead>
            <tbody>
              {list.data.map((a) => (
                <tr key={a.id}>
                  <td>
                    <Link to={`/help/${a.id}`}>{isolate(String(a.title ?? ""))}</Link>
                  </td>
                  <td>{isolate(a._titles?.category ?? "")}</td>
                  <td>
                    <Badge tone={STATUS_TONES[a.status as keyof typeof STATUS_TONES] ?? "neutral"}>{isolate(String(a.status ?? ""))}</Badge>
                  </td>
                  <td>{isolate(String(a.visibility ?? ""))}</td>
                  <td className="faint">
                    {a.status === "Published" ? [t("{n} views", { n: Number(a.views ?? 0) }), helpfulShare(Number(a.helpful_count), Number(a.not_helpful_count))].filter(Boolean).join(" · ") : ""}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

export function ArticleEditor() {
  const { id = "" } = useParams();
  const isNew = id === "new";
  const rights = useRights("KbArticle");
  const record = useQuery({
    queryKey: ["helpdesk", "article", id],
    enabled: !isNew,
    queryFn: ({ signal }) => api.get<Row>(docPath("KbArticle", id), { titles: true }, signal),
  });
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  if (!isNew && record.error) {
    if (record.error instanceof ApiError && record.error.status === 404) return <StatePanel kind="empty" title={t("This article no longer exists")} action={<Link to="/help">{t("Back to the articles")}</Link>} />;
    return <LoadFailed error={record.error} onRetry={() => void record.refetch()} />;
  }
  if (!isNew && !record.data) return <ReaderSkeleton paragraphs={4} />;
  // Keyed by the version, so that what the server now holds replaces a stale draft only by the person's own reload.
  return <Editor key={`${id}:${record.data?.version ?? 0}`} record={isNew ? null : (record.data as Row)} canWrite={isNew ? rights.create : rights.write} />;
}

function Editor({ record, canWrite }: { record: Row | null; canWrite: boolean }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const spec = SPEC();
  const [draft, setDraft] = useState(() => draftOf(spec, record, { visibility: "Customers Only", status: "Draft" }));
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);
  const [shown, setShown] = useState(false);
  const [key] = useState(newKey);
  useShellTitle(String(draft.title || t("New article")));
  const bad = problems(spec, draft);
  const set = (name: string, v: string | boolean) => setDraft((d) => ({ ...d, [name]: v }));
  const dirty = record ? Object.keys(bodyOf(spec, draft, record)).length > 0 : true;
  const slug = bootstrap()?.workspace.slug;

  const save = async (status?: string) => {
    setShown(true);
    const next = status ? { ...draft, status } : draft;
    if (busy || Object.keys(problems(spec, next)).length) return;
    setBusy(true);
    setRefusal(null);
    try {
      const body = bodyOf(spec, next, record);
      const saved = record ? await api.patch<Row>(docPath("KbArticle", record.id), body, { ifMatch: Number(record.version) }) : await api.post<Row>(docPath("KbArticle"), body, { idempotencyKey: key });
      void qc.invalidateQueries({ queryKey: ["helpdesk", "articles"] });
      qc.setQueryData(["helpdesk", "article", saved.id], saved);
      toast(status === "Published" ? t("Published") : status === "Archived" ? t("Archived") : t("Saved"));
      if (!record) navigate(`/help/${saved.id}`, { replace: true });
    } catch (e) {
      setRefusal(e instanceof ApiError && (e.status === 409 || e.status === 412) ? t("Someone else changed this article while you had it open. Reload to see their change; what you wrote is still here until you do.") : errorMessage(e));
    }
    setBusy(false);
  };
  const archive = async () => {
    if (await confirmDialog({ title: t("Archive this article?"), message: t("Customers no longer find it. Agents still can, and it can be published again."), confirmLabel: t("Archive it") })) void save("Archived");
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    void save();
  };
  const published = record?.status === "Published";
  const controls = (names: string[]) =>
    spec
      .filter((f) => names.includes(f.name))
      .map((f) => <Control key={f.name} f={f} draft={draft} set={set} id={`hd-art-${f.name}`} error={shown ? bad[f.name] : undefined} disabled={!canWrite} title={record?._titles?.[f.name]} />);
  return (
    <form className="stack" onSubmit={submit} noValidate>
      <div className="page-head">
        <div>
          <p className="crumbs">
            <Link to="/help">{t("Help centre")}</Link>
          </p>
          <h1 className="page-title">{record ? isolate(String(record.title ?? "")) : t("New article")}</h1>
        </div>
        {canWrite && (
          <div className="page-actions">
            <Button type="submit" loading={busy} disabled={!dirty}>
              {t("Save")}
            </Button>
            {draft.status !== "Published" && (
              <Button variant="primary" disabled={busy} onClick={() => void save("Published")}>
                {t("Publish")}
              </Button>
            )}
            {published && (
              <Button disabled={busy} onClick={() => void archive()}>
                {t("Archive")}
              </Button>
            )}
          </div>
        )}
      </div>
      {refusal && <Alert tone="error">{isolate(refusal)}</Alert>}
      {!canWrite && <Alert tone="info">{t("You can read this article but not change it.")}</Alert>}
      <Alert tone="info">{reach(String(draft.status), String(draft.visibility))}</Alert>
      <div className="hd-article">
        <div className="stack">
          {controls(["title", "category", "visibility", "status", "summary", "content"])}
        </div>
        <div className="stack">
          <section className="hd-card" aria-labelledby="hd-art-preview">
            <h3 id="hd-art-preview">{t("How it reads")}</h3>
            <h2 className="hd-admin-h">{isolate(String(draft.title || t("Untitled")))}</h2>
            {draft.summary && <p className="faint">{isolate(String(draft.summary))}</p>}
            {String(draft.content).trim() ? <Markdown source={String(draft.content)} /> : <p className="faint">{t("Nothing written yet.")}</p>}
          </section>
          {record && (
            <section className="hd-card" aria-labelledby="hd-art-facts">
              <h3 id="hd-art-facts">{t("About this article")}</h3>
              <p>
                {record.published_at ? t("Published {when}", { when: formatMoment(String(record.published_at), { dateStyle: "medium", timeStyle: "short" }) }) : t("Not published yet")}
              </p>
              <p>{t("{n} views", { n: Number(record.views ?? 0) })}</p>
              <p>{helpfulShare(Number(record.helpful_count), Number(record.not_helpful_count)) ?? t("Nobody has said whether it helped yet.")}</p>
              {record.slug ? (
                <p>
                  {t("Address")}: <code>{isolate(String(record.slug))}</code>
                  {slug && published && record.visibility === "Public" && <span className="faint"> {t("on the public list /public/{slug}/lists/help-centre", { slug })}</span>}
                </p>
              ) : (
                <p className="faint">{t("It gets its address when it is published.")}</p>
              )}
            </section>
          )}
        </div>
      </div>
    </form>
  );
}
