// The keys a screen answers to, shown on `?`.

import { Dialog } from "@ferrum/kit";

export interface KeyRow {
  keys: string[];
  says: string;
}

export function Shortcuts({ open, onClose, title, rows }: { open: boolean; onClose: () => void; title: string; rows: KeyRow[] }) {
  return (
    <Dialog open={open} onClose={onClose} title={title}>
      <dl className="hd-keys">
        {rows.map((r) => (
          <div key={r.says}>
            <dt>
              {r.keys.map((k) => (
                <kbd key={k} className="kbd">
                  {k}
                </kbd>
              ))}
            </dt>
            <dd>{r.says}</dd>
          </div>
        ))}
      </dl>
      <p className="faint">Keys work when you are not typing in a field.</p>
    </Dialog>
  );
}
