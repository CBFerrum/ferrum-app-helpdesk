// Brands: what a customer knows a support address by. A brand has a name (on the Reply-To), a signature that ends
// every public reply sent under it after the agent's own, and a header and footer that wrap the reply outside the
// message template's own wording. The dialog shows a sample reply wrapped in what is typed, in the order the
// server puts it together (header, reply, agent's signature, brand's signature, footer).

import { isolate, t } from "@ferrum/client";
import { Markdown } from "@ferrum/kit";
import { type Column, AdminTable } from "./Parts";
import type { Draft, FieldSpec } from "./form";

/** A reply as the customer would see it under the brand typed so far. Nothing here is sent. */
export function BrandPreview({ draft }: { draft: Draft }) {
  const text = (k: string) => String(draft[k] ?? "").trim();
  const name = text("display_name") || text("brand_name") || t("Your brand");
  return (
    <section className="hd-card" aria-labelledby="hd-brand-preview">
      <h3 id="hd-brand-preview">{t("A reply under this brand")}</h3>
      <p className="faint">
        {t("From")}: <bdi>{isolate(name)}</bdi> &lt;support@example.com&gt;
      </p>
      <div className="hd-brand-mail">
        {text("header") && <Markdown source={text("header")} />}
        <p>{t("Hello Ada, thank you for writing. The printer is fixed; please tell us if it jams again.")}</p>
        <p className="faint">{t("The agent's own signature comes here.")}</p>
        {text("signature") && <Markdown source={text("signature")} />}
        {text("footer") && <Markdown source={text("footer")} />}
      </div>
      <p className="faint">{t("The wording around it is the message template helpdesk.reply, edited under Message templates. Nothing here is sent.")}</p>
    </section>
  );
}

export function Brands() {
  const spec: FieldSpec[] = [
    { name: "brand_name", label: t("Name"), kind: "text", required: true, max: 80 },
    { name: "display_name", label: t("Name the customer sees"), kind: "text", max: 80, hint: t("On the reply address, for example Acme Support. Empty: the name above.") },
    { name: "signature", label: t("Signature"), kind: "markdown", max: 2000, rows: 3, hint: t("Markdown. It ends each public reply, after the agent's own signature.") },
    { name: "header", label: t("Header"), kind: "markdown", max: 4000, rows: 3, hint: t("Markdown. Placed before each public reply, outside the message template's own wording.") },
    { name: "footer", label: t("Footer"), kind: "markdown", max: 4000, rows: 3, hint: t("Markdown. Placed after each public reply, outside the message template's own wording.") },
  ];
  const columns: Column[] = [
    { key: "name", label: t("Brand"), cell: (r) => isolate(String(r.brand_name ?? "")) },
    { key: "display", label: t("Name the customer sees"), cell: (r) => (r.display_name ? isolate(String(r.display_name)) : <span className="faint">{t("The brand's name")}</span>) },
    { key: "wrap", label: t("Signature, header, footer"), cell: (r) => [r.signature && t("Signature"), r.header && t("Header"), r.footer && t("Footer")].filter(Boolean).join(", ") || <span className="faint">{t("None")}</span> },
  ];
  return (
    <AdminTable
      entity="SupportBrand"
      noun={t("brand")}
      fields="brand_name,display_name,signature,header,footer"
      sort="brand_name asc"
      spec={spec}
      columns={columns}
      extra={(draft) => <BrandPreview draft={draft} />}
      note={<p className="faint">{t("A support address names its brand. Replies to that address's tickets carry the brand's name, signature, header and footer.")}</p>}
      empty={t("No brand yet. Add one, then choose it on a support address.")}
    />
  );
}
