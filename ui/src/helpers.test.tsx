// The helpers under a ticket's box: the saved reply as the server renders it, the macro with what it will
// change, and the signature. Data is put in the query cache as the routes answer it (Helpdesk `replies.rs`,
// `macros.rs`), so the screens draw as they would after loading.

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router";
import { describe, expect, it } from "vitest";
import { Composer } from "./Composer";
import { MacroPicker } from "./MacroPicker";
import { ReplyPicker } from "./ReplyPicker";
import { SignatureLine } from "./Signature";

const ME = "priya@acme.test";

function draw(node: React.ReactNode, seed: (qc: QueryClient) => void = () => {}) {
  const qc = new QueryClient();
  qc.setQueryData(["me"], { tenant: "t", user: { id: "u1", email: ME, full_name: "Priya Nair", is_admin: false, roles: [] } });
  seed(qc);
  return renderToStaticMarkup(
    <QueryClientProvider client={qc}>
      <MemoryRouter>{node}</MemoryRouter>
    </QueryClientProvider>,
  );
}

const current = { status: "Open", priority: "High", assigned_to: null, team: "Billing", ticket_type: null, language: null, tags: ["vip"], name: (e: string) => e };

describe("the macro picker", () => {
  const macros = [
    { id: "m1", macro_name: "Refund approved", scope: "Everyone", uses: 4 },
    { id: "m2", macro_name: "Close quietly", scope: "Team", uses: 1 },
  ];
  const seed = (qc: QueryClient) => {
    qc.setQueryData(["helpdesk", "macros"], macros);
    qc.setQueryData(["helpdesk", "macro", "m1"], { id: "m1", macro_name: "Refund approved", scope: "Everyone", steps: [{ step: "Set Status", value: "Resolved" }, { step: "Add Tag", value: "refund" }, { step: "Public Reply", value: "Your refund is on its way." }] });
    qc.setQueryData(["helpdesk", "macro", "m2"], { id: "m2", macro_name: "Close quietly", scope: "Team", steps: [{ step: "Set Priority", value: "High" }] });
  };
  const open = (o = {}) => draw(<MacroPicker ticket={{ id: "t1", number: "TKT-1", current }} open onClose={() => {}} onApplied={() => {}} {...o} />, seed);

  it("lists the macros, previews what the first will change, and says it sends a reply", () => {
    const html = open();
    expect(html).toContain("Refund approved");
    expect(html).toContain("Close quietly");
    expect(html).toContain("Status: Open becomes Resolved");
    expect(html).toContain("Adds the tag refund");
    expect(html).toContain("Sent to the customer");
    expect(html).toContain("This macro sends the customer a reply");
    expect(html).toContain("Apply and send the reply");
    expect(html).toContain("Every step applies, or none does.");
  });

  it("says when a step changes nothing", () => {
    const html = draw(<MacroPicker ticket={{ id: "t1", current: { ...current, status: "Resolved" } }} open onClose={() => {}} onApplied={() => {}} />, seed);
    expect(html).toContain("Status is already Resolved: no change");
  });

  it("says plainly when there are no macros", () => {
    const html = draw(<MacroPicker ticket={{ id: "t1", current }} open onClose={() => {}} onApplied={() => {}} />, (qc) => qc.setQueryData(["helpdesk", "macros"], []));
    expect(html).toContain("No macros yet");
  });
});

describe("the saved reply picker", () => {
  const replies = [{ id: "r1", title: "Thanks", body: "Hi {{customer.first_name}}", scope: "Everyone", uses: 2 }];
  it("shows the reply as the server filled it in for the ticket", () => {
    const html = draw(<ReplyPicker ticket="t1" draft="" onPick={() => {}} onClose={() => {}} />, (qc) => {
      qc.setQueryData(["helpdesk", "replies"], replies);
      qc.setQueryData(["helpdesk", "render", "r1", "t1"], "Hi Ada");
    });
    expect(html).toContain("For this ticket");
    expect(html).toContain("Hi Ada");
    expect(html).not.toContain("{{");
  });

  it("waits for the server to fill it in, saying so", () => {
    const html = draw(<ReplyPicker ticket="t1" draft="" onPick={() => {}} onClose={() => {}} />, (qc) => {
      qc.setQueryData(["helpdesk", "replies"], replies);
    });
    expect(html).toContain("For this ticket");
    expect(html).toContain("Filling it in");
  });
});

describe("the signature", () => {
  it("is shown under a reply that will be emailed, with where to change it", () => {
    const html = draw(<SignatureLine email={ME} onEdit={() => {}} />, (qc) => qc.setQueryData(["helpdesk", "profile", ME], { id: "p1", version: 1, agent: ME, signature: "Priya, **Acme support**" }));
    expect(html).toContain("Your signature is added to emailed replies");
    expect(html).toContain("Acme support");
    expect(html).toContain("Change your signature");
  });

  it("offers to write one when there is none", () => {
    expect(draw(<SignatureLine email={ME} onEdit={() => {}} />, (qc) => qc.setQueryData(["helpdesk", "profile", ME], null))).toContain("Write a signature");
  });

  it("is in the box for a reply and not for a note, and the macros button only where the workspace has macros", () => {
    const box = (o = {}) =>
      draw(<Composer ticket="t1" mode="reply" onMode={() => {}} inputRef={{ current: null }} rootRef={null} to={{ name: "Ada", email: "ada@shop.test" }} cc={[]} assignee={null} agent={ME} onSignature={() => {}} replies conflict={null} onSend={async () => true} {...o} />, (qc) =>
        qc.setQueryData(["helpdesk", "profile", ME], { id: "p1", version: 1, agent: ME, signature: "Priya" }));
    expect(box()).toContain("Your signature is added");
    expect(box({ mode: "note" })).not.toContain("Your signature is added");
    expect(box()).not.toContain(">Macros<");
    expect(box({ onMacros: () => {} })).toContain(">Macros<");
  });
});
