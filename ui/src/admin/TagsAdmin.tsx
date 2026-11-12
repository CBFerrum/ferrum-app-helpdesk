// Tags and the rules that put them on tickets. A tag is a lowercase word with a colour; a rule's condition is
// checked each time a ticket is saved and adds its tag when it holds. A rule never takes a tag off: a person does.

import { Badge } from "@ferrum/kit";
import { isolate, t } from "@ferrum/client";
import { TagChip } from "../Tags";
import { AdminTable, type Column } from "./Parts";
import type { FieldSpec } from "./form";

export function TagsAdmin() {
  const tag: FieldSpec[] = [
    { name: "tag_name", label: t("Tag"), kind: "text", required: true, max: 40, hint: t("One lowercase word: letters, digits, dashes and underscores. It is kept as written in lower case.") },
    { name: "colour", label: t("Colour"), kind: "colour", hint: t("Shown as a dot beside the tag, as #rrggbb.") },
    { name: "description", label: t("What it is for"), kind: "textarea", rows: 2 },
  ];
  const rule: FieldSpec[] = [
    { name: "rule_name", label: t("Rule"), kind: "text", required: true, max: 140 },
    { name: "tag", label: t("Tag it adds"), kind: "link", to: "TicketTag", required: true },
    {
      name: "condition",
      label: t("When"),
      kind: "code",
      required: true,
      rows: 3,
      hint: t("A condition on the ticket, for example doc.priority == 'Urgent', or doc.subject.contains('refund'). It is checked when the ticket is saved; one that does not compile is refused."),
    },
    { name: "enabled", label: t("On"), kind: "check" },
  ];
  const tagCols: Column[] = [
    {
      key: "tag",
      label: t("Tag"),
      cell: (r) => (
        <ul className="hd-tags">
          <TagChip tag={String(r.tag_name ?? "")} colour={String(r.colour ?? "")} />
        </ul>
      ),
    },
    { key: "colour", label: t("Colour"), cell: (r) => (r.colour ? isolate(String(r.colour)) : <span className="faint">{t("None")}</span>) },
    { key: "description", label: t("What it is for"), cell: (r) => isolate(String(r.description ?? "")) },
  ];
  const ruleCols: Column[] = [
    { key: "rule", label: t("Rule"), cell: (r) => isolate(String(r.rule_name ?? "")) },
    { key: "tag", label: t("Adds"), cell: (r) => isolate(r._titles?.tag ?? "") },
    { key: "when", label: t("When"), cell: (r) => <code>{isolate(String(r.condition ?? ""))}</code> },
    { key: "on", label: t("State"), cell: (r) => <Badge tone={r.enabled === false ? "neutral" : "green"}>{r.enabled === false ? t("Off") : t("On")}</Badge> },
  ];
  return (
    <div className="stack">
      <h2 className="hd-admin-h">{t("Tags")}</h2>
      <AdminTable entity="TicketTag" noun={t("tag")} fields="tag_name,colour,description" sort="tag_name asc" spec={tag} columns={tagCols} empty={t("No tags yet. Agents can still add a word to a ticket; a tag here gives it a colour.")} />
      <h2 className="hd-admin-h">{t("Tag rules")}</h2>
      <AdminTable
        entity="TagRule"
        noun={t("tag rule")}
        fields="rule_name,tag,condition,enabled"
        sort="rule_name asc"
        spec={rule}
        columns={ruleCols}
        defaults={{ enabled: true }}
        empty={t("No tag rules yet.")}
      />
    </div>
  );
}
