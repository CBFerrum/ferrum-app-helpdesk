// The inbox's left side: the queues with how many tickets each holds, the filters a person saved, and
// who has how many, so the load is seen before the next ticket is taken.

import { Link } from "react-router";
import { Icon } from "@ferrum/kit";
import { initials, isFilter, type InboxState, type QueueId, type SavedFilter, START, writeState } from "./inbox";

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
}) {
  const plain = (q: QueueId) => ({ search: writeState({ ...START, queue: q }).toString() });
  const onAFilter = saved.some((f) => isFilter(f, state));
  const most = Math.max(1, ...people.map((p) => p.count));
  return (
    <div className="hd-side">
      <nav className="hd-queues" aria-label="Queues">
        <ul>
          {queues.map((q, i) => (
            <li key={q.id}>
              <Link to={plain(q.id)} aria-current={state.queue === q.id && !onAFilter ? "page" : undefined}>
                <span>{q.label}</span>
                {counts[q.id] !== undefined && <span className="hd-count">{counts[q.id]}</span>}
                <kbd className="kbd" aria-hidden="true" title={`Press ${i + 1}`}>
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
            Saved filters
          </h2>
          <ul className="hd-saved">
            {saved.map((f) => (
              <li key={f.id}>
                <Link to={{ search: new URLSearchParams(f.query).toString() }} aria-current={isFilter(f, state) ? "page" : undefined}>
                  {f.name}
                </Link>
                <button type="button" className="icon-btn icon-btn-sm" aria-label={`Remove the saved filter ${f.name}`} onClick={() => onRemove(f)}>
                  <Icon name="x" size={12} />
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {showMore && (
        <section aria-labelledby="hd-people">
          <h2 id="hd-people" className="hd-side-title">
            Who has what
          </h2>
          {people.length ? (
            <ul className="hd-people">
              {people.map((p) => (
                <li key={p.who}>
                  <button type="button" aria-pressed={state.assignee.toLowerCase() === p.who.toLowerCase()} onClick={() => onPerson(p.who)} title={`Show ${name(p.who)}'s tickets`}>
                    <span className="hd-avatar hd-avatar-sm" aria-hidden="true">
                      {initials(name(p.who))}
                    </span>
                    <span className="hd-person-name">{name(p.who)}</span>
                    <span className="hd-count">{p.count}</span>
                    <span className="hd-bar" style={{ width: `${(p.count / most) * 100}%` }} aria-hidden="true" />
                  </button>
                </li>
              ))}
            </ul>
          ) : (
            <p className="faint">Nobody has an open ticket.</p>
          )}
        </section>
      )}
    </div>
  );
}
