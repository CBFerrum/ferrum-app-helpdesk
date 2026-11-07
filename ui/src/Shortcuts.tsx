// The keys a screen answers to, shown on `?`.

import { Dialog } from "@ferrum/kit";
import { t, isolate } from "@ferrum/client";

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
                  {isolate(k)}
                </kbd>
              ))}
            </dt>
            <dd>{isolate(r.says)}</dd>
          </div>
        ))}
      </dl>
      <p className="faint">{t("Keys work when you are not typing in a field.")}</p>
    </Dialog>
  );
}
