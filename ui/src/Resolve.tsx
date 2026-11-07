// Finishing a ticket: the server wants to know how it was resolved, and the customer usually wants
// to be told. The same dialog serves one ticket in its workspace and many from the inbox.

import { type FormEvent, useEffect, useState } from "react";
import { Alert, Button, Dialog, Field } from "@ferrum/kit";
import { t } from "@ferrum/client";

export interface Resolution {
  resolution: string;
  /** Also send the words to the customer as a reply. */
  reply: boolean;
}

export function ResolveDialog({
  open,
  status,
  what,
  tell,
  onClose,
  onSubmit,
}: {
  open: boolean;
  status: "Resolved" | "Closed";
  /** What is being finished, for the title: "TKT-2026-000123", "3 tickets". */
  what: string;
  /** Whom the words can also be sent to; without it the choice is not offered. */
  tell?: string | null;
  onClose: () => void;
  /** Resolves true when it is done, which closes the dialog; false keeps it for another try. */
  onSubmit: (r: Resolution) => Promise<boolean>;
}) {
  const [text, setText] = useState("");
  const [reply, setReply] = useState(true);
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    if (open) {
      setText("");
      setBusy(false);
    }
  }, [open]);
  const word = status === "Closed" ? "Close" : "Resolve";
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!text.trim() || busy) return;
    setBusy(true);
    const done = await onSubmit({ resolution: text.trim(), reply: !!tell && reply });
    setBusy(false);
    if (done) onClose();
  };
  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={`${word} ${what}`}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-resolve-form" loading={busy} disabled={!text.trim()}>
            {word}
          </Button>
        </>
      }
    >
      <form id="hd-resolve-form" className="stack" onSubmit={submit}>
        {status === "Closed" && <Alert tone="warning">{t("A closed ticket is final. It cannot be reopened, and nothing more can be written on it.")}</Alert>}
        <Field label={t("How it was resolved")} htmlFor="hd-resolution" required hint={t("Kept with the ticket, and shown to the customer once it is resolved.")}>
          <textarea id="hd-resolution" className="textarea" rows={4} value={text} onChange={(e) => setText(e.target.value)} />
        </Field>
        {tell && (
          <label className="hd-check">
            <input type="checkbox" checked={reply} onChange={(e) => setReply(e.target.checked)} />
            {t("Also send this to {tell} as a reply", { tell })}
          </label>
        )}
      </form>
    </Dialog>
  );
}
