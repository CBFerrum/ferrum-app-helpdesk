# Changelog

## 0.7.2

A ticket's times, its service level's due times and the conversation's day dividers are on the workspace's clock, naming its zone when the browser's is another. The customer's portal keeps the customer's own. Rebuilt with Ferrum-Frontend 24c3951, so its records' pages show times on the workspace's clock too, a draft numbered on submit is headed as a draft, and a record of an app that is not installed says so.

## 0.7.1

Rebuilt with Ferrum-Frontend bb60115: a record page reads an empty number or check as the server does, so an action whose condition needs one shows, and a workspace's settings open as their one record.

## 0.7.0

Files on the portal stream: a file sent with a ticket's message is sent by the server straight from storage, whatever its size, instead of being read whole into memory first (Ferrum's SiteResponse::file, FERRUM-OPS-006).

## 0.6.3

Rebuilt with Ferrum-Frontend 00dea07: in a workflow state that names what may change, the other fields show read-only, and the fields the state needs show required.

## 0.6.2

Rebuilt with Ferrum-Frontend daeadda: after an action runs, a record page shows what the action said it did rather than only that it is done.

## 0.6.1

Rebuilt with Ferrum-Frontend 80ff179: a field's label names its control for assistive technology, and a record whose title is a link shows that record's title, not its id.

## 0.6.0

- A ticket opens as one conversation: what the customer wrote, every reply and internal note, and
  what happened to the ticket between them (its status, who took it, its priority, a service level
  missed, an escalation) in a single feed, which can be narrowed to the replies or to the notes. One
  box answers the customer or writes a note for the team. What is typed is kept for the ticket until
  it is sent, and Ctrl or Cmd with Enter sends. Beside it: the two service levels (grey until
  something has happened, then red, green or blue), who has the ticket, the customer with what their
  support contract entitles them to and their other tickets, what is linked to the ticket, and its
  files. R, N, E, A, J and K work a ticket from the keyboard, and ? lists them.
- The inbox has queues with how many tickets each holds (Mine, Unassigned, SLA alerts, My teams, All
  open). Each row shows the start of the last message, how long the ticket has waited, where its
  service level stands and who has it. J and K move, Enter opens, X selects, E resolves, R replies,
  A takes, / searches. Select several to assign them, set their priority or status, or resolve them
  together. A change shows at once and goes back, with the server's reason, if it is refused. The
  queue, order and filters are in the address, so a link opens the same view, and a view can be
  saved by name (for the person who saved it). "Who has what" shows each agent's open tickets.
  Replying to a ticket someone else has asks first and takes it over, and a ticket that changed while
  it was open says so rather than being overwritten.
- Saved replies, Personal, for a Team or for Everyone, with the customer's and the agent's names and
  the ticket's number and subject filled in. Press / in a reply to find one. A reply with something
  that cannot be filled in is held back until it is. The most used come first, and three come to
  start from.
- A reply now reaches the customer by email, through the platform's mail: the subject names the
  ticket, the text keeps its paragraphs, lists, bold and links, and a link takes the customer to the
  ticket. Each reply says whether it went out by email. Until now a reply was only in the
  conversation and in the portal.
- Files go with a reply (drop, paste or attach; up to ten, 20 MB together). They travel with the
  email, and a portal customer can download the files sent to them, never those on a note.
- Tickets keep when the customer and the team last spoke and the start of the last message, and
  messages keep their files and whether they were emailed. Requires the platform's mail for emailed
  replies (without a mailer a reply is added to the conversation and not emailed).

## 0.5.0

- Escalation rules, and support contracts that say what a customer is entitled to.

## 0.4.1

- The interface starts faster: its first download holds the frame and the app's own pages, and
  lists, forms, reports and other apps' screens load when first opened (Ferrum
  docs/plan/apps-ui.md §12.9). The build keeps it within the size budget.

## 0.4.0

- A page of the workspace's portal (Ferrum docs/plan/apps-ui.md §9.1, U7), "Your tickets" at
  /portal/tickets/: the tickets raised from the address a customer signed in with, each with its
  conversation (agents' internal notes never show), replies (which reopen a ticket waiting on them
  or resolved), new tickets (channel Portal, linked to the customer the address belongs to) and a
  satisfaction rating once resolved. /open of a ticket takes portal people there. The endpoints are
  `/api/app/helpdesk/portal/tickets…`; portal people hold no roles, so they answer by the address
  and act as the system after their checks, rate limited.
- Home's My work lists the open tickets assigned to you (`/api/app/helpdesk/work`), due on their
  SLA's resolution date, High and Urgent ones marked.
- The inbox shows no record cards over its rows (they repeat what the row says).
- Requires the platform with portals and `shell.portal` (Ferrum, apps-ui.md U7).

## 0.3.2

- Rebuilt with Ferrum-Frontend a8d4dda: under the platform's bar the records frame no longer shows
  a second bell (the bar has notifications).

## 0.3.1

- Its notifications have kinds each person chooses for (Ferrum's bell, apps-ui.md U5): tickets
  assigned to you, customer replies on your tickets and SLA breaches on them, in the bar and by
  email unless chosen otherwise. Requires the platform with notification kinds (Ferrum 2fccf10).

## 0.3.0

- An interface of its own at /helpdesk/ (Ferrum docs/plan/apps-ui.md U5), in `ui/` (built with
  Ferrum-Frontend's packages, `dist` committed and embedded as `UI`): the agent inbox (your tickets,
  the unassigned, SLA alerts and every open one, the most pressing first, live) and the ticket view
  (the conversation with replies and internal notes; status, priority, assignment and SLA beside
  it). Helpdesk's other records are lists and forms in the records frame with its modules.
- Launcher entry for Support Agents and Managers, New Ticket in the Create menu, and /open of
  tickets (the ticket view) and Helpdesk's other records. Requires the platform 0.3.2 with
  Ferrum-Frontend's `@ferrum/records/frame`.

## 0.2.0

- Helpdesk works on its own: it depends on the platform's core, parties and products instead of
  ERP's selling (Ferrum docs/plan/apps-ui.md §13).
- The connector app `helpdesk-erp` (package `helpdesk_erp`, shipped with Helpdesk, requiring
  Helpdesk and ERP) brings a ticket's sales invoice and a maintenance contract's sales order.
  Warranty claims read deliveries from the stock ledger where ERP keeps one. Requires the platform
  0.3.2.
- Declares where it keeps people's personal data, so Administration → Personal data exports and
  erases it for a person (by email).

## 0.1.0

- First release as a standalone Ferrum app (previously part of the Ferrum repository).
