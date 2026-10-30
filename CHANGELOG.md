# Changelog

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
