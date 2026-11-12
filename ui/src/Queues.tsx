// The inbox's left side: the queues with how many tickets each holds, the filters a person saved, and
// who has how many, so the load is seen before the next ticket is taken.

import { Link } from "react-router";
import { Icon } from "@ferrum/kit";
import { initials, isFilter, type InboxState, type QueueId, type SavedFilter, START, writeState } from "./inbox";
import { colourOf } from "./tagwords";
import type { CSSProperties } from "react";
import { t, isolate } from "@ferrum/client";

export interface QueueEntry {
  id: QueueId;
  label: string;
}

export function Queues({
  state,
  queues,
  counts,
  saved,
  onRemove,
  people,
  name,
  onPerson,
  showMore,
  tags,
  onTag,
  targets,
  onTarget,
}: {
  state: InboxState;
  /** The queues this person has (My teams only for those in a team). */
  queues: QueueEntry[];
  counts: Partial<Record<QueueId, number>>;
  saved: SavedFilter[];
  onRemove: (f: SavedFilter) => void;
  /** Each agent and how many open tickets they have, the most first. */
  people: { who: string; count: number }[];
  name: (email: string) => string;
  onPerson: (email: string) => void;
  /** Whether the saved filters and the people are shown (narrow screens fold them away). */
  showMore: boolean;
  /** The workspace's tags with their colours: a click narrows the queue to the tickets that carry one. */
  tags: { tag: string; colour?: string | null }[];
  onTag: (tag: string) => void;
  /** The targets that need an eye, each with how many open tickets are in that state. */
  targets: { id: string; label: string; count?: number }[];
  onTarget: (id: string) => void;
}) {
  const plain = (q: QueueId) => ({ search: writeState({ ...START, queue: q }).toString() });
  const onAFilter = saved.some((f) => isFilter(f, state));
  const most = Math.max(1, ...people.map((p) => p.count));
  return (
    <div className="hd-side">
      <nav className="hd-queues" aria-label={t("Queues")}>
        <ul>
          {queues.map((q, i) => (
            <li key={q.id}>
              <Link to={plain(q.id)} aria-current={state.queue === q.id && !onAFilter ? "page" : undefined}>
                <span>{isolate(q.label)}</span>
                {counts[q.id] !== undefined && <span className="hd-count">{counts[q.id]}</span>}
                <kbd className="kbd" aria-hidden="true" title={t("Press {value}", { value: i + 1 })}>
                  {i + 1}
                </kbd>
              </Link>
            </li>
          ))}
        </ul>
      </nav>
      {showMore && saved.length > 0 && (
        <section aria-labelledby="hd-saved">
          <h2 id="hd-saved" className="hd-side-title">
            {t("Saved filters")}
          </h2>
          <ul className="hd-saved">
            {saved.map((f) => (
              <li key={f.id}>
                <Link to={{ search: new URLSearchParams(f.query).toString() }} aria-current={isFilter(f, state) ? "page" : undefined}>
                  {isolate(f.name)}
                </Link>
                <button type="button" className="icon-btn icon-btn-sm" aria-label={t("Remove the saved filter {name}", { name: f.name })} onClick={() => onRemove(f)}>
                  <Icon name="x" size={12} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {showMore && targets.length > 0 && (
        <section aria-labelledby="hd-targets">
          <h2 id="hd-targets" className="hd-side-title">
            {t("By target")}
          </h2>
          <ul className="hd-saved">
            {targets.map((x) => (
              <li key={x.id}>
                <button type="button" className="hd-side-link" aria-pressed={state.target === x.id} onClick={() => onTarget(x.id)}>
                  <span>{isolate(x.label)}</span>
                  {x.count !== undefined && <span className="hd-count">{x.count}</span>}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {showMore && tags.length > 0 && (
        <section aria-labelledby="hd-tagsq">
          <h2 id="hd-tagsq" className="hd-side-title">
            {t("By tag")}
          </h2>
          <ul className="hd-saved">
            {tags.map((x) => (
              <li key={x.tag}>
                <button type="button" className="hd-side-link" aria-pressed={state.tag.toLowerCase() === x.tag.toLowerCase()} onClick={() => onTag(x.tag)}>
                  <span className="hd-tag-dot" aria-hidden="true" style={colourOf(x.colour) ? ({ "--tag": colourOf(x.colour) } as CSSProperties) : undefined} />
                  <span>{isolate(x.tag)}</span>
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {showMore && (
        <section aria-labelledby="hd-people">
          <h2 id="hd-people" className="hd-side-title">
            {t("Who has what")}
          </h2>
          {people.length ? (
            <ul className="hd-people">
              {people.map((p) => (
                <li key={p.who}>
                  <button type="button" aria-pressed={state.assignee.toLowerCase() === p.who.toLowerCase()} onClick={() => onPerson(p.who)} title={t("Show {value}'s tickets", { value: name(p.who) })}>
                    <span className="hd-avatar hd-avatar-sm" aria-hidden="true">
                      {isolate(initials(name(p.who)))}
                    </span>
                    <span className="hd-person-name">{isolate(name(p.who))}</span>
                    <span className="hd-count">{p.count}</span>
                    <span className="hd-bar" style={{ width: `${(p.count / most) * 100}%` }} aria-hidden="true" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="faint">{t("Nobody has an open ticket.")}</p>
          )}
        </section>
      )}
    </div>
  );
}
