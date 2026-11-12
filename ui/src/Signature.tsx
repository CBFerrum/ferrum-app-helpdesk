// The agent's signature: what is added under their words on every emailed reply (`AgentProfile.signature`,
// Markdown). Shown under the reply box, and changed in a dialog, so nobody sends a reply without knowing
// what it will end with.

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { type FormEvent, useState } from "react";
import { api, docPath, errorMessage, isolate, t } from "@ferrum/client";
import { Alert, Button, Dialog, Field, Markdown } from "@ferrum/kit";

export interface Profile {
  id: string;
  version: number;
  agent: string;
  signature?: string | null;
}

const KEY = (email: string) => ["helpdesk", "profile", email.toLowerCase()];

/** The signed-in agent's profile, if they have one. A person who may not read profiles has no signature to show. */
export function useProfile(email: string | undefined) {
  return useQuery({
    queryKey: KEY(email ?? ""),
    enabled: !!email,
    staleTime: 60_000,
    queryFn: ({ signal }) =>
      api
        .get<{ rows: Profile[] }>("/api/v1/AgentProfile", { filters: JSON.stringify([["agent", "=", (email ?? "").toLowerCase()]]), fields: "agent,signature,version", limit: 1 }, signal)
        .then((r) => r.rows[0] ?? null),
  });
}

export function SignatureLine({ email, onEdit }: { email: string; onEdit: () => void }) {
  const q = useProfile(email);
  if (q.isLoading) return null;
  const sig = q.data?.signature?.trim();
  return (
    <details className="hd-signature">
      <summary>{sig ? t("Your signature is added to emailed replies") : t("You have no signature")}</summary>
      {sig ? <Markdown source={sig} /> : <p className="faint">{t("Replies end with your words only.")}</p>}
      <Button size="sm" onClick={onEdit}>
        {sig ? t("Change your signature") : t("Write a signature")}
      </Button>
    </details>
  );
}

export function SignatureDialog({ email, open, onClose }: { email: string; open: boolean; onClose: () => void }) {
  const qc = useQueryClient();
  const q = useProfile(open ? email : undefined);
  const [text, setText] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const value = text ?? q.data?.signature ?? "";
  const close = () => {
    setText(null);
    setProblem(null);
    onClose();
  };
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (busy || q.isLoading) return;
    setBusy(true);
    setProblem(null);
    try {
      const signature = value.trim() || null;
      if (q.data) await api.patch(docPath("AgentProfile", q.data.id), { signature }, { ifMatch: Number(q.data.version) });
      else await api.post(docPath("AgentProfile"), { agent: email.toLowerCase(), signature });
      await qc.invalidateQueries({ queryKey: KEY(email) });
      close();
    } catch (err) {
      setProblem(errorMessage(err));
    } finally {
      setBusy(false);
    }
  };
  return (
    <Dialog
      open={open}
      onClose={close}
      title={t("Your signature")}
      footer={
        <>
          <Button onClick={close}>{t("Cancel")}</Button>
          <Button variant="primary" type="submit" form="hd-signature-form" loading={busy} disabled={q.isLoading}>
            {t("Save")}
          </Button>
        </>
      }
    >
      <form id="hd-signature-form" className="stack" onSubmit={(e) => void submit(e)}>
        {problem && <Alert tone="error">{isolate(problem)}</Alert>}
        <Field label={t("Signature")} htmlFor="hd-signature-text" hint={t("Markdown works. It is added under your words on every reply that is emailed, never on an internal note.")}>
          <textarea id="hd-signature-text" className="textarea" rows={5} maxLength={2000} value={value} onChange={(e) => setText(e.target.value)} />
        </Field>
        {value.trim() && (
          <div>
            <h3 className="hd-subhead">{t("As the customer will read it")}</h3>
            <Markdown source={value} />
          </div>
        )}
      </form>
    </Dialog>
  );
}
