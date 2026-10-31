# Changelog

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
