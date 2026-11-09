// Work on a ticket outside the customer's entitlement, where the workspace holds it for approval
// (Support Settings): a Support Manager approves or declines it, saying why.

import { type FormEvent, useState } from "react";
import { Button, Dialog, Field } from "@ferrum/kit";
import { t } from "@ferrum/client";

export type Decision = "approve" | "decline";

export function DecisionDialog({ decision, onClose, onSubmit }: { decision: Decision | null; onClose: () => void; onSubmit: (reason: string) => Promise<void> }) {
  const [reason, setReason] = useState("");
  const [busy, setBusy] = useState(false);
  const submit = async (e: FormEvent) => {
    e.preventDefault();
    if (!reason.trim()) return;
    setBusy(true);
    await onSubmit(reason.trim());
    setBusy(false);
    setReason("");
  };
  return (
    <Dialog
      open={!!decision}
      onClose={onClose}
      title={decision === "approve" ? t("Approve work outside the entitlement") : t("Decline work outside the entitlement")}
      footer={
        <>
          <Button onClick={onClose}>{t("Cancel")}</Button>
          <Button variant="primary" form="hd-entitlement" type="submit" loading={busy} disabled={!reason.trim()}>
            {decision === "approve" ? t("Approve") : t("Decline")}
          </Button>
        </>
      }
    >
      <form id="hd-entitlement" onSubmit={submit}>
        <Field
          label={t("Why")}
          htmlFor="hd-entitlement-reason"
          required
          hint={decision === "approve" ? t("Why this customer is helped though their contract does not cover it") : t("Why it is not worked: the ticket is resolved with this")}
        >
          <input id="hd-entitlement-reason" className="input" maxLength={500} value={reason} onChange={(e) => setReason(e.target.value)} autoFocus />
        </Field>
      </form>
    </Dialog>
  );
}
