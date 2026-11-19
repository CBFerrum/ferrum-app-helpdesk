// The help centre's articles, for the people who write them: a list with what each is, who it reaches and how
// it was received, and an editor with a preview of the page. An article is Draft, Published or Archived and is Internal,
// for Customers, or Public; saying who it will reach is part of the screen. The public list and page are the
// platform's (the published list `help-centre`): this edits the articles they show and does not draw them.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { Link, useNavigate, useParams, useSearchParams } from "react-router";
import { api, ApiError, bootstrap, docPath, errorMessage, formatMoment, isolate, newKey, t } from "@ferrum/client";
import { Alert, Badge, Button, confirmDialog, ListSkeleton, Markdown, ReaderSkeleton, StatePanel, toast, useShellTitle } from "@ferrum/kit";
import { useDebounced } from "@ferrum/kit";
import { Control, type Row, useRights } from "./admin/Parts";
import { bodyOf, draftOf, type FieldSpec, problems } from "./admin/form";
import { helpfulShare, reach, STATUS_TONES, STATUSES, VISIBILITIES } from "./articles";
import { LoadFailed } from "./states";

const FIELDS = "title,category,status,visibility,summary,published_at,views,helpful_count,not_helpful_count,slug,language,translation_of";

const SPEC = (): FieldSpec[] => [
  { name: "title", label: t("Title"), kind: "text", required: true, max: 200 },
  { name: "category", label: t("Category"), kind: "link", to: "KbCategory", required: true },
  { name: "visibility", label: t("Who can read it"), kind: "select", required: true, options: VISIBILITIES, optionLabels: { Internal: t("Agents only"), "Customers Only": t("Customers signed in to the portal"), Public: t("Everyone, on the public help centre") } },
  { name: "status", label: t("State"), kind: "select", required: true, options: STATUSES, optionLabels: { Draft: t("Draft"), Published: t("Published"), Archived: t("Archived") } },
  { name: "summary", label: t("Summary"), kind: "text", max: 300, hint: t("One line, shown in search results and in the list of articles.") },
  { name: "language", label: t("Language"), kind: "text", max: 35, hint: t("A tag such as en or hi-IN. Empty: the workspace's own language.") },
  { name: "translation_of", label: t("Translation of"), kind: "link", to: "KbArticle", hint: t("Leave empty for an original.") },
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
                    {a.language ? <span className="faint">{` ${String(a.language)}`}</span> : null}
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
  // A translation starts from its original: `/help/new?of={id}`.
  const of = useSearchParams()[0].get("of");
  const rights = useRights("KbArticle");
  const record = useQuery({
    queryKey: ["helpdesk", "article", id],
    enabled: !isNew,
    queryFn: ({ signal }) => api.get<Row>(docPath("KbArticle", id), { titles: true }, signal),
  });
  const original = useQuery({
    queryKey: ["helpdesk", "article", of ?? ""],
    enabled: isNew && !!of,
    queryFn: ({ signal }) => api.get<Row>(docPath("KbArticle", of ?? ""), { titles: true }, signal),
  });
  if (rights.loaded && !rights.read) return <StatePanel kind="denied" />;
  if (!isNew && record.error) {
    if (record.error instanceof ApiError && record.error.status === 404) return <StatePanel kind="empty" title={t("This article no longer exists")} action={<Link to="/help">{t("Back to the articles")}</Link>} />;
    return <LoadFailed error={record.error} onRetry={() => void record.refetch()} />;
  }
  if (!isNew && !record.data) return <ReaderSkeleton paragraphs={4} />;
  if (isNew && of && !original.data && !original.error) return <ReaderSkeleton paragraphs={4} />;
  const from = isNew && of && original.data ? original.data : null;
  // Keyed by the version, so that what the server now holds replaces a stale draft only by the person's own reload.
  return <Editor key={`${id}:${record.data?.version ?? 0}:${from?.id ?? ""}`} record={isNew ? null : (record.data as Row)} original={from} canWrite={isNew ? rights.create : rights.write} />;
}

/** A published version, as the article keeps it. */
interface Version {
  id?: string;
  version_no: number;
  published_at?: string | null;
  published_by?: string | null;
  title?: string | null;
}

function Editor({ record, original, canWrite }: { record: Row | null; original?: Row | null; canWrite: boolean }) {
  const qc = useQueryClient();
  const navigate = useNavigate();
  const spec = SPEC();
  const start = original
    ? { visibility: String(original.visibility ?? "Customers Only"), status: "Draft", translation_of: original.id, category: String(original.category ?? "") }
    : { visibility: "Customers Only", status: "Draft" };
  const titles: Record<string, string> = record?._titles ?? (original ? { translation_of: String(original.title ?? ""), category: original._titles?.category ?? "" } : {});
  const [draft, setDraft] = useState(() => draftOf(spec, record, start));
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
      .map((f) => <Control key={f.name} f={f} draft={draft} set={set} id={`hd-art-${f.name}`} error={shown ? bad[f.name] : undefined} disabled={!canWrite} title={titles[f.name]} />);
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
          {controls(["title", "category", "visibility", "status", "summary", "language", "translation_of", "content"])}
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
              <p className="faint">{t("Views and votes count the portal and the public help centre together.")}</p>
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
          {record && <Translations record={record} canWrite={canWrite} />}
          {record && <Versions record={record} canWrite={canWrite} />}
        </div>
      </div>
    </form>
  );
}

/** An article's translations (on an original), or the original it translates. */
function Translations({ record, canWrite }: { record: Row; canWrite: boolean }) {
  const list = useQuery({
    queryKey: ["helpdesk", "translations", record.id],
    enabled: !record.translation_of,
    queryFn: ({ signal }) =>
      api.get<{ rows: Row[] }>(docPath("KbArticle"), { filters: JSON.stringify([["translation_of", "=", record.id]]), fields: "title,language,status", sort: "language asc", limit: 100 }, signal).then((r) => r.rows),
  });
  return (
    <section className="hd-card" aria-labelledby="hd-art-translations">
      <h3 id="hd-art-translations">{t("Translations")}</h3>
      {record.translation_of ? (
        <p>
          {t("A translation of")} <Link to={`/help/${String(record.translation_of)}`}>{isolate(record._titles?.translation_of ?? t("its original"))}</Link>
        </p>
      ) : list.error ? (
        <p className="faint">{t("They could not be loaded.")}</p>
      ) : !list.data ? (
        <p className="faint" role="status">{t("Loading the translations")}</p>
      ) : !list.data.length ? (
        <p className="faint">{t("No translations yet.")}</p>
      ) : (
        <ul aria-labelledby="hd-art-translations">
          {list.data.map((x) => (
            <li key={x.id}>
              <Link to={`/help/${x.id}`}>{isolate(String(x.title ?? ""))}</Link> <span className="faint">{isolate([x.language, x.status].filter(Boolean).join(" · "))}</span>
            </li>
          ))}
        </ul>
      )}
      {canWrite && !record.translation_of && <Link to={`/help/new?of=${record.id}`}>{t("Add a translation")}</Link>}
    </section>
  );
}

/** What was published each time, newest first; an editor can bring a version back. */
function Versions({ record, canWrite }: { record: Row; canWrite: boolean }) {
  const qc = useQueryClient();
  const versions = [...((record.versions as Version[] | undefined) ?? [])].sort((a, b) => b.version_no - a.version_no);
  const restore = async (n: number) => {
    const ok = await confirmDialog({
      title: t("Restore version {n}?", { n }),
      message: t("The article's title, summary and text become version {n}'s. If it is published, customers see it at once.", { n }),
      confirmLabel: t("Restore it"),
    });
    if (!ok) return;
    try {
      await api.post(docPath("KbArticle", record.id, "actions", "restore_version"), { version: n });
      toast(t("Version {n} restored", { n }));
      void qc.invalidateQueries({ queryKey: ["helpdesk", "article", record.id] });
      void qc.invalidateQueries({ queryKey: ["helpdesk", "articles"] });
    } catch (e) {
      toast(errorMessage(e), "error");
    }
  };
  return (
    <section className="hd-card" aria-labelledby="hd-art-versions">
      <h3 id="hd-art-versions">{t("Published versions")}</h3>
      {!versions.length ? (
        <p className="faint">{t("Nothing has been published yet.")}</p>
      ) : (
        <ul aria-labelledby="hd-art-versions">
          {versions.map((v) => (
            <li key={v.id ?? v.version_no}>
              <strong>{t("Version {n}", { n: v.version_no })}</strong>{" "}
              <span className="faint">
                {isolate([v.published_at ? formatMoment(v.published_at, { dateStyle: "medium", timeStyle: "short" }) : null, v.published_by, v.title].filter(Boolean).join(" · "))}
              </span>{" "}
              {canWrite && (
                <Button size="sm" onClick={() => void restore(v.version_no)}>
                  {t("Restore")}
                </Button>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
