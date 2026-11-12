// A ticket's conversation as one feed: the customer's words on the left, the team's on the right,
// internal notes tinted and labelled, and what happened to the ticket as one grey line between them.

import { Fragment, type Ref } from "react";
import { formatMoment, t, isolate } from "@ferrum/client";
import { Icon, Markdown, StatePanel } from "@ferrum/kit";
import { counts, dayKey, dayLabel, type EventItem, type FeedFilter, type FeedItem, fileSize, filterFeed, type MessageItem } from "./feed";
import { initials } from "./inbox";

const when = (at: string) => formatMoment(at, { day: "numeric", month: "short", hour: "numeric", minute: "2-digit" });
const full = (at: string) => formatMoment(at, { dateStyle: "medium", timeStyle: "medium" });

const ICONS: Record<EventItem["icon"], string> = { status: "refresh", person: "user", priority: "up", clock: "calendar", alert: "alert", star: "check" };

function Message({ m, address }: { m: MessageItem; address: string | null }) {
  return (
    <li className="hd-msg" data-side={m.side} data-internal={m.internal || undefined}>
      <span className="hd-avatar" aria-hidden="true">
        {isolate(initials(m.email ?? m.author))}
      </span>
      <article className="hd-bubble">
        <header className="hd-bubble-head">
          <strong>{isolate(m.author)}</strong>
          {m.internal && <span className="hd-note-chip">{t("Internal note")}</span>}
          {m.mergedFrom && <span className="hd-from-chip">{t("From {n}, merged", { n: m.mergedFrom })}</span>}
          <time dateTime={m.at} title={full(m.at)} className="faint">
            {isolate(when(m.at))}
          </time>
        </header>
        <div className="hd-bubble-body">
          <Markdown source={m.body} />
        </div>
        {m.attachments.length > 0 && (
          <ul className="hd-files" aria-label={t("Files")}>
            {m.attachments.map((f) => (
              <li key={f.id}>
                <a className="hd-file" href={`/files/${f.id}`} download={f.file_name}>
                  <Icon name="file" size={14} />
                  <span className="hd-file-name">{isolate(f.file_name)}</span>
                  {f.size ? <span className="faint">{isolate(fileSize(f.size))}</span> : null}
                </a>
              </li>
            ))}
          </ul>
        )}
        {m.side === "agent" && !m.internal && (
          <p className="hd-sent faint" title={m.emailed && address ? t("Emailed to {address}", { address }) : undefined}>
            {m.emailed ? t("Emailed to the customer") : t("Not sent by email. The customer can read it in the portal.")}
          </p>
        )}
      </article>
    </li>
  );
}

function Event({ e }: { e: EventItem }) {
  return (
    <li className="hd-event" data-tone={e.tone}>
      <span className="hd-event-icon" aria-hidden="true">
        <Icon name={ICONS[e.icon]} size={12} />
      </span>
      <div className="hd-event-body">
        <span>
          {isolate(e.text)}
          {e.who && <span className="faint"> · {isolate(e.who)}</span>}
        </span>
        <time dateTime={e.at} title={full(e.at)} className="faint">
          {isolate(when(e.at))}
        </time>
        {e.detail && <p className="hd-event-detail">{isolate(e.detail)}</p>}
      </div>
    </li>
  );
}

const FILTERS: { id: FeedFilter; label: string }[] = [
  { id: "all", get label() { return t("All"); } },
  { id: "replies", get label() { return t("Replies"); } },
  { id: "notes", get label() { return t("Notes"); } },
];

export function Conversation({
  items,
  filter,
  onFilter,
  address,
  endRef,
}: {
  items: FeedItem[];
  filter: FeedFilter;
  onFilter: (f: FeedFilter) => void;
  /** The ticket's contact address, which an email went to. */
  address: string | null;
  endRef: Ref<HTMLLIElement>;
}) {
  const n = counts(items);
  const shown = filterFeed(items, filter);
  const today = new Date();
  return (
    <section className="hd-feed-wrap" aria-label={t("Conversation")}>
      <div className="hd-feed-tools">
        <div className="segmented" role="group" aria-label={t("Show")}>
          {FILTERS.map((f) => (
            <button key={f.id} type="button" aria-pressed={filter === f.id} onClick={() => onFilter(f.id)}>
              {isolate(f.label)}
              {f.id !== "all" && <span className="faint">{f.id === "replies" ? n.replies : n.notes}</span>}
            </button>
          ))}
        </div>
      </div>
      {shown.length === 0 && filter === "notes" ? (
        <StatePanel kind="empty" title={t("No internal notes yet")}>
          {t("Switch the box below to Internal note to leave something only your team sees.")}
        </StatePanel>
      ) : (
        <ol className="hd-feed">
          {shown.map((item, i) => (
            <Fragment key={item.key}>
              {(i === 0 || dayKey(item.at) !== dayKey(shown[i - 1].at)) && item.at && (
                <li className="hd-day">
                  <span>{isolate(dayLabel(item.at, today))}</span>
                </li>
              )}
              {item.kind === "message" ? <Message m={item} address={address} /> : <Event e={item} />}
            </Fragment>
          ))}
          <li ref={endRef} aria-hidden="true" className="hd-feed-end" />
        </ol>
      )}
    </section>
  );
}
