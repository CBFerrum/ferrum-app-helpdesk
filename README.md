<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/banner-dark.svg">
  <img alt="Ferrum Helpdesk: every customer email becomes a ticket your team answers on time" src="docs/assets/banner-light.svg" width="100%">
</picture>

<br>

[![Version](https://img.shields.io/badge/version-0.10.3-7c3aed?style=flat-square)](CHANGELOG.md)
[![Licence: AGPL-3.0](https://img.shields.io/badge/licence-AGPL--3.0-blue?style=flat-square)](LICENSE)
[![Ferrum platform](https://img.shields.io/badge/runs%20on-Ferrum-2f5bea?style=flat-square)](https://github.com/CBFerrum/Ferrum)
![Rust](https://img.shields.io/badge/Rust-server-b7410e?style=flat-square&logo=rust)
![PostgreSQL](https://img.shields.io/badge/PostgreSQL-data-336791?style=flat-square&logo=postgresql&logoColor=white)
![React](https://img.shields.io/badge/React%20%2B%20TypeScript-interface-149eca?style=flat-square&logo=react&logoColor=white)
![Tests](https://img.shields.io/badge/tests-43%20Rust%20%2B%2021%20UI%20files-16a34a?style=flat-square)

### A support desk where customers email you, your team answers on time, and every answer is written once.

Ferrum Helpdesk is for support teams that outgrew a shared mailbox and do not want a per-agent subscription.
Mail sent to your support addresses becomes tickets, targets count working hours, assignment shares the work by
skill and shift, and the same data set holds the customer's contracts, equipment, warranty and invoices.

[💡 Why](#-why-helpdesk) · [📸 Screenshots](#-screenshots) · [✨ Features](#-features) · [📊 Compare](#-how-it-compares) · [📍 Roadmap](#-roadmap) · [🚀 Install](#-install) · [🔧 Develop](#-develop)

</div>

## 💡 Why Helpdesk

- 📥 **Email that threads properly.** A customer's reply lands on the right ticket by the headers of the mail, a secret
  in the Reply-To, or the ticket tag in the subject, never by subject text alone. Loops, bounces and forged senders are held back.
- ⏱️ **Targets on the platform's clock.** First response and resolution count working hours, time zones with daylight
  saving and dated holidays. They pause while a ticket waits and record each breach once.
- ⚖️ **Fair, skilled routing.** Round robin or least loaded, by skill and language, skipping agents who are away or
  off shift, and stays correct when tickets arrive together.
- 🔗 **One data set with ERP.** Customers, items, serial numbers, support contracts, warranty and invoices are the
  records the rest of the business uses, shown beside the ticket.
- 🏠 **Self-hosted and open source.** AGPL-3.0, on your own server and database, with no per-agent fee.
- 🧩 **Extendable by configuration.** Add fields, forms, automations and reports in Studio. Everything is also
  reachable over REST and MCP under each person's permissions.
- 🌍 **Arabic and Hindi** words and right-to-left layout, beside English.

## 🎯 At a glance

| | |
|---|---|
| **Version** | 0.10.3 (platform `>=0.3.2, <0.4`) |
| **Kind** | Own interface at `/helpdesk/` (agent inbox, ticket workspace, administration, help centre editor, reports) and a page in the customer portal |
| **What it adds** | 30 entities (21 main, 9 child tables) in 2 packages, 3 roles, 9 reports, 3 customer message templates, 1 menu module, 3 notification kinds, 4 seeded ticket types |
| **Depends on** | Ferrum core, shared customers and items |
| **Works with** | Mail (inbound and outbound email, version 0.20 or later), ERP (invoices and orders through a connector), Field Service (visits from tickets), the platform's calendars and service targets |
| **Languages** | English, Arabic (right to left), Hindi; a workspace can change any interface word |
| **Tests** | 43 Rust tests and 21 interface test files |
| **Licence** | AGPL-3.0-only |

## 📸 Screenshots

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-inbox-dark.webp">
  <img alt="Helpdesk inbox" src="docs/assets/screens/helpdesk-inbox-light.webp" width="100%">
</picture>

**📥 Agent inbox**<br><sub>See the tickets assigned to you, the unassigned ones and those close to breaking their service level.</sub>

<table>
<tr>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-unassigned-dark.webp"><img alt="Unassigned tickets" src="docs/assets/screens/helpdesk-unassigned-light.webp"></picture>
<p><b>🙋 Unassigned tickets</b><br><sub>Pick up tickets nobody has taken yet.</sub></p>
</td>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-all-open-dark.webp"><img alt="All open tickets" src="docs/assets/screens/helpdesk-all-open-light.webp"></picture>
<p><b>📂 All open tickets</b><br><sub>See every open ticket across the team and who has it.</sub></p>
</td>
</tr>
<tr>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-ticket-dark.webp"><img alt="A ticket" src="docs/assets/screens/helpdesk-ticket-light.webp"></picture>
<p><b>💬 A ticket</b><br><sub>Read the conversation, reply to the customer or write a note, and change the status or priority.</sub></p>
</td>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-new-ticket-dark.webp"><img alt="New ticket" src="docs/assets/screens/helpdesk-new-ticket-light.webp"></picture>
<p><b>➕ New ticket</b><br><sub>Log a ticket for a customer who contacted you by phone or in person.</sub></p>
</td>
</tr>
<tr>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-ticket-list-dark.webp"><img alt="Tickets list" src="docs/assets/screens/helpdesk-ticket-list-light.webp"></picture>
<p><b>🎫 Tickets</b><br><sub>Browse, search and filter every ticket by status, priority, team and service level.</sub></p>
</td>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-kb-article-list-dark.webp"><img alt="Knowledge base articles" src="docs/assets/screens/helpdesk-kb-article-list-light.webp"></picture>
<p><b>📚 Knowledge base</b><br><sub>Find, write and publish the articles customers and agents rely on.</sub></p>
</td>
</tr>
<tr>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-saved-reply-list-dark.webp"><img alt="Saved replies" src="docs/assets/screens/helpdesk-saved-reply-list-light.webp"></picture>
<p><b>⚡ Saved replies</b><br><sub>Write an answer once and share it with yourself, a team or everyone.</sub></p>
</td>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-service-level-agreement-list-dark.webp"><img alt="Service level agreements" src="docs/assets/screens/helpdesk-service-level-agreement-list-light.webp"></picture>
<p><b>⏱️ Service level agreements</b><br><sub>Set response and resolution targets by priority, customer, group, team or type.</sub></p>
</td>
</tr>
<tr>
<td width="50%" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-report-support-ticket-summary-dark.webp"><img alt="Ticket summary report" src="docs/assets/screens/helpdesk-report-support-ticket-summary-light.webp"></picture>
<p><b>📊 Ticket summary</b><br><sub>Run the ticket summary for a period and read the counts.</sub></p>
</td>
<td width="50%" valign="top">
</td>
</tr>
</table>

<table>
<tr>
<td align="center" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-inbox-phone-dark.webp"><img alt="Inbox on a phone" src="docs/assets/screens/helpdesk-inbox-phone-light.webp" width="260"></picture>
<p><b>📱 Inbox on a phone</b></p>
</td>
<td align="center" valign="top">
<picture><source media="(prefers-color-scheme: dark)" srcset="docs/assets/screens/helpdesk-ticket-list-phone-dark.webp"><img alt="Tickets on a phone" src="docs/assets/screens/helpdesk-ticket-list-phone-light.webp" width="260"></picture>
<p><b>📱 Tickets on a phone</b></p>
</td>
</tr>
</table>

## ✨ Features

### 📥 Email in and out

- **Support Address.** An address customers write to (support@, billing@), with its team, default priority and
  ticket type, the Mail account replies go out from, and an optional brand. One can serve everyone or one per team.
- **Registered with Mail.** Each enabled address is registered with the Mail app and kept in step as addresses
  change. The address shows whether Mail took it and, if not, why. Mail hands each message over once, with its headers.
- **New ticket or a reply.** Mail for an address becomes a new ticket of its team, or a message on the ticket it answers.
- **Threading.** A reply is matched by the Message-ID it answers, then by the ticket's secret in the address it was
  written to (`support+<secret>@...`), then by the ticket number and secret in the subject tag
  `[TKT-2026-000123/secret]`. Subject text alone never matches. A reply to a merged ticket lands on the survivor.
- **Closed tickets.** Mail on a closed ticket opens a new one that says which ticket it follows up.
- **Customer matching.** The sender is matched to a customer by exact address (the customer's or a contact's).
  Unknown senders are handled as Support Settings say: open a ticket, hold for review, or ignore.
- **Loop and noise protection.** Automatic replies, bounces, mailing-list mail, mail from a support address itself,
  mail carrying Helpdesk's own marker and mail over a sender's hourly limit (20 by default) are logged and never
  turned into tickets or answered.
- **Sender authentication.** Mail the mail server could not verify is checked: a failed DMARC, SPF and DKIM result
  from a known customer, the requester or a copy of the ticket it answers is held for staff (Hold Failed, the
  default; Hold All Unverified; or Accept). Mail with no result is taken and marked "Sender not verified" on the message.
- **Inbound Mail Log.** What became of each mail: ticket created, added to ticket, held, ignored or failed, and why.
  Staff open a ticket from held mail or discard it. The text of held mail is readable only by roles given the
  held-mail field group. The log is kept 90 days by default and erased with the sender.
- **Replies that go back out.** Through Mail with a Reply-To that routes back, In-Reply-To and References from the
  conversation, the copies on the ticket, and the support address's account. Without Mail, through the platform's mail.
- **Brands.** A name the customer sees, a display name, a signature, a header and a footer around the reply, set per
  support address. The wording around an agent's reply is an editable message template.
- **Files.** A mail's attachments are copied onto the ticket.
- **CC.** Copies are kept and copied on replies. An internal note never goes to anyone.

### 🎫 Tickets and conversations

- **Ticket** (`TKT-2026-000001`). Subject, description, status (Open, Replied, On Hold, Resolved, Closed), priority
  (Low, Medium, High, Urgent), type, channel (Email, Portal, Phone, Chat, Web Form), customer, contact, team,
  assignee, language, product and serial number.
- **Conversation.** An append-only feed of messages, each from an agent or the customer, public or internal.
  Sent messages cannot be edited or removed.
- **Public reply or internal note.** The box says which it is, in words and on the button: Send reply by email, Add
  reply to the portal, Add internal note. A note is never emailed, copied or shown to the customer, and does not
  answer the customer for the response target.
- **Status moves.** Replying marks a ticket Replied. A customer's reply reopens it. On Hold waits on someone.
  Resolving needs resolution details. A resolved ticket without a customer reply closes by itself after 7 days (a setting).
- **Merge duplicates.** Merge a customer's tickets into one. The conversation moves in time order with each message
  marked with where it came from, with files, comments, assignments, watchers and tags. Duplicates close as merged
  and cannot be unmerged. The survivor's service level is not reset.
- **Follow-up tickets.** A ticket that continues a closed one links to it.
- **Tags.** Coloured tags added by hand or by tag rules that put a tag on every ticket a condition holds for.
- **Custom fields.** Add your own fields and forms to a ticket in Studio.
- **Two agents answering at once.** A write carries the version on screen, so the second agent is told and the first
  agent's words are never overwritten.
- **Personal data.** A person's name and address on their tickets can be erased on request; the conversation stays with the ticket.

### 🖥️ The agent inbox

- **Queues** with counts: yours, unassigned, all open and those close to breaking their service level, the most
  pressing first. Saved filters, who has how many tickets, and the order live in the address so a link opens the same view.
- **Keyboard.** `j` and `k` move, Enter opens, `x` selects, `e` resolves, `r` replies, `a` takes, `?` lists the rest.
- **Bulk actions.** Select tickets to resolve, tag, apply a macro or merge. Each ticket runs in its own transaction and
  failures stay selected with their reason.
- **Ticket workspace.** The whole conversation in one feed with what happened to the ticket between messages, one
  box to answer the customer or write to the team, and a side rail with the service levels, assignee, priority, customer,
  what they are entitled to, their other tickets, linked records and files. Next and previous walk the inbox order.
- **Composer.** `/` finds a saved reply. Files are dropped, pasted or attached. Ctrl or Cmd with Enter sends. What
  is typed is kept until sent and the tab warns before closing on unsent words. The agent's signature is shown under the reply.
- **Refusals in plain words.** A change shows at once and goes back, with the server's reason, if it is refused.

### ⚖️ Routing and workload

- **Support Team** with members, each active or not and with a limit of open tickets. Restrict an agent to their
  teams' tickets with a Support Team scope.
- **Assignment rules.** The enabled rule with the highest priority whose condition matches picks the team and an agent.
  Round Robin or Load Balancing. Rules can need a fixed skill or the ticket's language, and can skip agents who are away or off shift.
- **Agent Profile.** The skills and languages an agent covers, their shift calendar and their signature.
- **Fallback.** When nobody can take it, the rule's fallback agent gets it, else it stays with the team.
- **Reopened tickets** go round again when their agent cannot take them (a setting, on by default).
- **Unanswered tickets** go to the next available agent after a number of hours, up to a limit (off by default).
- **Correct under concurrency.** The rule's row is locked, so two tickets at once take turns.

### ⏱️ Service levels and escalation

- **Service Level Agreement.** Response and resolution hours per priority, counted in working time on the agreement's
  own calendar, else the workspace's `helpdesk_hours`. Chosen by the most specific match: customer, customer group, team, type, then the default.
  A support contract's agreement comes first.
- **Calendars.** Hours, time zone with daylight saving and dated exceptions, from the platform's calendars.
- **Pause.** The clock stops while a ticket is On Hold or Replied, per agreement.
- **Clock start.** From the moment the ticket was opened, so a backdated or imported ticket is due by its own opening.
  A reopened ticket's resolution starts again from the reopening.
- **States.** Running, paused, met, met late or breached for each of first response and resolution. A ticket's
  SLA status is On Track, At Risk (after 75% of the time, a setting), Breached or Fulfilled.
- **Breach recorded once** and raising the ticket's updated event, so automations and webhooks can act.
- **Priority raised.** Moves the due moment from the save.
- **Escalation rules.** At Risk or Breached, by level, once per ticket: assign, move to a team, raise priority to
  High or Urgent, or notify an address.
- **Entitlement.** Support Contract: a customer's period, an optional agreement and an optional number of tickets. A
  ticket is marked Covered, Expired, Exhausted or None. Settings choose to inform or to Hold for Approval: the ticket waits On Hold with no reply to
  the customer until a Support Manager approves or declines it, saying why.

### ⚡ Replies, macros and tags

- **Saved Reply.** Personal, for a team or for everyone, with merge fields such as `{{customer.first_name}}`,
  `{{ticket.number}}` and `{{agent.first_name}}`. The server fills them in, so the preview is exactly what is sent. Uses are counted.
- **Macro.** Several changes in one click: set status, priority, team, assignee, type, language or resolution, add
  or remove a tag, send a public reply, add a private note. All steps apply or none. A picker previews what each step changes
  and warns first when one sends the customer a reply. Applied once, with an idempotency key.
- **Tags and tag rules.** See above.
- **Seeded.** Four ticket types (Question, Incident, Problem, Feature Request), a knowledge base category and three saved replies.

### ⭐ Satisfaction

- **Survey on solve.** When on, a solved ticket's customer is mailed a signed, one-use link (30 days by default) after a delay (1 hour by default) to rate the help 1 to 5 with a comment.
- **Once only.** The rating and comment are kept once. The portal's rating does the same.
- **A low rating can act.** At or below a number it reopens a resolved ticket, and at or below another it tells the agent and a lead.
- **Opt-outs respected.** The survey is not sent to an address that opted out of marketing mail or whose processing is restricted.

### 📚 Knowledge base and help centre

- **Knowledge Base Article** (`KB-00001`) with category (nested categories), content, a summary, an address
  (slug), status (Draft, Published, Archived) and visibility (Internal, Customers Only, Public).
- **Help centre.** Published, Public articles are the platform's published list, made once for the workspace, with a
  page for each article, search and a sitemap. The workspace can change or take it down.
- **In the portal.** Customers search the articles they may read, read them and say whether one helped (once a day).
  Articles that fit are offered under the subject while they write a request.
- **Counts.** Views and helpful votes from the portal and the public page are added together on the article.
- **Editor.** A list of what each article is, who it reaches and how it was received, and an editor with category,
  body, a preview, visibility, state, publish and archive.
- **Roles.** A Knowledge Base Editor role writes and publishes articles without seeing tickets.

### 🧑‍💼 Customer portal

- **Your tickets.** Signed-in customers see the tickets raised from their address, each one's public conversation
  (agents' notes never appear), reply, raise a new ticket, and rate once a ticket is resolved. Files can be attached.
- **My work.** Agents see the open tickets assigned to them on Home, due when their resolution is due.

### 🛡️ Warranty claims and maintenance

- **Warranty Claim** (`WC-2026-00001`). A customer's complaint about an item and serial number. The warranty
  status (Under Warranty, Out of Warranty, Under AMC) follows from when the serial number was delivered, per the
  stock ledger, and the item's warranty period (added to the item). An active maintenance contract covering the item means Under AMC.
- **Create Warranty Claim** from a ticket with an item and customer.
- **Maintenance Schedule** (`MS-2026-00001`). An annual maintenance contract: submitting plans one visit per period
  (weekly, monthly, quarterly, half-yearly, yearly) for each line. Cancelling drops visits not yet made.
- **Maintenance Visit** (`MV-2026-00001`). Carries out exactly one planned row, as scheduled, unscheduled or breakdown, with work done and customer feedback.
- **Linked records.** With ERP, a ticket links to its sales invoice and a maintenance contract to its sales order.

### 📊 Reports

| Report | What it shows |
|---|---|
| **Ticket Summary** | Tickets counted by status, priority and more |
| **SLA Compliance** | Targets met and missed |
| **Response Times** | First response and resolution in working hours |
| **Agent Workload** | Open tickets and work by agent |
| **Customer Satisfaction** | Ratings by agent, team and month, counted from the rating's own date |
| **Backlog by Age** | Open tickets by how long they have waited |
| **Ticket Volume** | By channel, tag, customer, team or month |
| **Reopens** | The reopen rate |
| **Help Centre by Article** | Views, helpful, not helpful and helpful % |

The Reports page asks the six questions a desk asks and opens the platform's report with the grouping and period
that answer each. Reports can be downloaded as CSV there.

### 🛠️ Administration

A section at `/helpdesk/admin` for: support addresses and their brands, the inbound log with release and discard of
held mail, agent profiles, tags and tag rules, assignment rules in the order tried (with Earlier and Later),
macros with their steps, service agreements (working calendar, targets and criteria), and survey settings with a
preview of the message and the rating page.

### ⚙️ Settings

| Setting | What it does |
|---|---|
| Close resolved tickets after (days) | Automatic close with no customer reply (7; 0 never) |
| Tickets outside entitlement | Inform, or Hold for Approval |
| Mail from unknown senders | Open Ticket, Hold for Review or Ignore |
| Mail that fails authentication | Hold Failed, Hold All Unverified or Accept |
| Mail taken from one sender per hour | Limit before mail is logged only (20) |
| Keep the inbound mail log | Days (90) |
| At risk after | Share of the time that has passed (75%) |
| Route reopened tickets | On by default |
| Reassign after unanswered, reassignments per ticket | Hours (off) and a limit (2) |
| Survey | Enabled, delay, link life, reopen at or below, tell the lead at or below, who the lead is |

### 🔔 Notifications

- **Tickets assigned to you,** **Customer replies on your tickets** and **SLA breaches on your tickets**, each by in-app message and email by default.
- **Messages.** The customer reply, the survey and the low-rating alert are message templates a workspace edits under Settings, Messages.
- **Assistant.** The Ferrum assistant knows the statuses and suggests questions such as "Which tickets are about to miss their SLA?"

### 🔐 Roles and privacy

- **Support Agent.** Works tickets, writes articles, logs warranty claims and visits. Sees shared macros and replies and their own.
- **Support Manager.** Everything, including agreements, assignment rules, teams, contracts, addresses and the inbound log.
- **Knowledge Base Editor.** Articles and categories only.
- **Handover.** When a person leaves, their open tickets, team memberships and maintenance assignments can be handed over.
- **Personal data.** Requests for a person's data find their tickets and inbound mail.

## 🧭 How it works

```mermaid
flowchart LR
    M["Customer email"] --> A["Support address"]
    A --> T["Ticket: Open"]
    T -->|"routed by rule"| G["Agent"]
    G -->|"reply"| R["Replied"]
    R -->|"customer writes back"| T
    T -->|"waiting"| H["On Hold, clock pauses"]
    R -->|"Resolve"| S["Resolved"]
    S -->|"no reply in 7 days"| C["Closed"]
    S -->|"low rating"| T
```

Mail turns into a ticket or a message on one. The assignment rule picks a team and an agent. Targets run on working
hours and pause while the ticket waits. A solved ticket can ask for a rating, and a low one reopens it.

```mermaid
flowchart TD
    HD["Helpdesk"] --> MAIL["Mail: inbound and outbound"]
    HD --> CAL["Platform calendars and service targets"]
    HD --> ERP["ERP: customers, items, serial numbers, invoices"]
    HD --> FS["Field Service: visits from a ticket"]
    CUST["Customer"] --> PORTAL["Portal and public help centre"]
    PORTAL --> HD
    HD --> EV["Notifications, webhooks, automations"]
```

Helpdesk uses the platform's engines for calendars, targets, tags and files rather than its own. Field Service reads a
ticket by number to open a visit.

## 🧱 Data model

| Entity | What it is | Key fields or states |
|---|---|---|
| **Ticket** | A customer's request and its conversation | Number, subject, status, priority, channel, customer, team, assignee, SLA states, entitlement, tags |
| Ticket message (child) | One message | Author, body, internal, sent at, attachments, copies, verified sender |
| **Support Address** | An address customers write to | Address, brand, team, default priority and type, Mail account, enabled, registered |
| **Support Brand** | What a customer knows an address by | Name, display name, signature, header, footer |
| **Inbound Log** | What became of each mail | Outcome, reason, sender, ticket |
| **Support Team** | Agents who share a queue | Members with limits |
| **Assignment Rule** | Routes new tickets | Condition, team, round robin or load balancing, skill, availability, fallback |
| **Agent Profile** | An agent's skills, shift and signature | Skills, shift calendar, signature |
| **Service Level Agreement** | Targets per priority | Customer, group, type, team, calendar, pauses |
| **Escalation Rule** | What happens at risk or breach | Level, trigger, actions |
| **Support Contract** | A customer's entitlement | Period, agreement, included tickets |
| **Ticket Type, Ticket Tag, Tag Rule** | Classifying tickets | Name, colour, condition |
| **Saved Reply, Support Macro** | Written once, used often | Scope, body or steps, uses |
| **Knowledge Base Article, Category** | Answers | Status, visibility, slug, views, votes |
| **Warranty Claim** | A warranty complaint | Item, serial, delivery, warranty status |
| **Maintenance Schedule, Maintenance Visit** | Annual maintenance contract and its visits | Items, periodicity, planned and actual dates |
| **Support Settings** | Workspace settings | See Settings above |

| Role | What it can do |
|---|---|
| **Support Agent** | Works tickets, writes articles, logs claims and visits |
| **Support Manager** | Everything in support, including agreements, rules, teams and contracts |
| **Knowledge Base Editor** | Writes and publishes articles |

Reports: nine, listed under Features. Print formats: none ship. Messages: customer reply, survey and low-rating alert.
Upkeep (on a schedule): target checks and escalation, auto-close, reassignment, surveys, planned visits.

## 🔌 Integrations

| With | How | Needs |
|---|---|---|
| **Mail** | Inbound mail by registered support addresses, outbound replies with threading | Mail app 0.20 or later, a mailbox account |
| **ERP** (`helpdesk_erp`) | A ticket's sales invoice and a maintenance contract's sales order; customers, items, serial numbers and stock movements for warranty | ERP installed |
| **Field Service** | A service request takes a ticket's customer, contact, priority and equipment by number | Field Service installed |
| **Platform calendars and service targets** | Working hours, time zones, holidays, targets and breach events | Platform |
| **Platform published lists** | The public help centre and its sitemap | Platform |
| **Webhooks and automations** | Events such as the ticket's updated event on a breach | Platform |
| **MCP** | AI agents work with tickets under each person's permissions | Platform's MCP server |

## 📊 How it compares

Compared with Zendesk, Freshdesk, Intercom, Salesforce Service Cloud and Zammad (open source).

| Capability | Ferrum Helpdesk | Zendesk | Freshdesk | Intercom | Service Cloud | Zammad |
|---|:-:|:-:|:-:|:-:|:-:|:-:|
| Email becomes a ticket, replies thread | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Spam, loop and bounce protection | ✅ | ✅ | ✅ | ✅ | 🟡 | ✅ |
| Customer portal | 🟡¹ | ✅ | ✅ | 🟡 | ✅ | ✅ |
| Live chat widget | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| WhatsApp and SMS | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Phone and voicemail | ❌ | ✅ | ✅ | 🟡 | ✅ | 🟡 |
| Social channels | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Merge duplicate tickets | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Parent, child and linked tickets | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Tags and custom fields | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Custom statuses | ❌ | ✅ | 🟡 | ✅ | ✅ | 🟡 |
| CC and followers | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Saved replies and macros with merge fields | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Triggers and automations on conversation content | 🟡² | ✅ | ✅ | ✅ | ✅ | ✅ |
| Routing by skill, language and shift | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 |
| SLA targets in working hours with holidays and pauses | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| More clocks: next reply, periodic update | ❌ | ✅ | ✅ | 🟡 | ✅ | ✅ |
| Escalation rules | 🟡³ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Entitlement by support contract | ✅ | 🟡 | 🟡 | ❌ | ✅ | ❌ |
| Knowledge base and public help centre | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Article suggestions for agents while replying | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Article translations and versions | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Customer satisfaction survey by mailed link | ✅ | ✅ | ✅ | ✅ | ✅ | 🟡 |
| NPS and CES surveys | ❌ | 🟡 | 🟡 | ✅ | ✅ | ❌ |
| AI: suggested replies, summaries, answer bot | ❌ | ✅ | ✅ | ✅ | ✅ | 🟡 |
| See who else is viewing a ticket | ❌⁴ | ✅ | ✅ | ✅ | 🟡 | ✅ |
| Time tracking on tickets | ❌ | 🟡 | ✅ | ❌ | 🟡 | ✅ |
| Reports | 🟡⁵ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Several brands | 🟡⁶ | ✅ | ✅ | ✅ | ✅ | 🟡 |
| Customer's orders, invoices, equipment and warranty beside the ticket | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🟡 |
| Warranty claims and maintenance visits | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ |
| Same data set as CRM, accounting and stock | ✅ | 🟡 | 🟡 | 🟡 | ✅ | 🟡 |
| Native mobile app | ❌ | ✅ | ✅ | ✅ | ✅ | 🟡 |
| Self-hosted | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Open source | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ |
| No per-agent subscription | ✅ | ❌ | ❌ | ❌ | ❌ | 🟡⁷ |

¹ Signed-in customers raise tickets, reply, follow and rate them, and read articles. There is no sign-in-free web form yet.
² Platform automations and tag rules read ticket fields; there are no string functions on message text yet.
³ Escalation can assign, move a team, raise priority or notify an address.
⁴ A second agent saving over the first is told and loses nothing, but nobody sees who is viewing.
⁵ Nine ready reports, downloadable as CSV; more in the platform's report builder.
⁶ A brand sets name, signature and wrapper per support address; there is no separate portal or help centre per brand.
⁷ Zammad is free to self-host; its hosted service is priced per agent.

### 🏆 Where Helpdesk stands out

- 🔗 **One data set** with customers, equipment, contracts, warranty and invoices, and warranty decided from serial-number deliveries.
- 📥 **Careful email handling**: threading by headers and secrets, loop protection and held mail from unverified senders.
- ⏱️ **Honest clocks**: working hours, daylight saving, pauses, breaches recorded once and events for automations.
- 🧾 **Entitlement by contract**, with an approval hold for work outside it.
- ⚖️ **Skill and shift routing** that stays correct under concurrent tickets.
- 🏠 **Self-hosted, open source and no per-agent fee**, with MCP access for AI agents under each person's permissions.

### 🚧 Where the leaders are still ahead

- 💬 **More channels**: live chat, WhatsApp, SMS, phone and social messages.
- 🤖 **AI** that drafts replies, summarises and answers customers.
- 🧬 **Ticket structure**: custom statuses, parent and child tickets, split a message into a ticket.
- 👀 **Presence and teamwork**: who is viewing, mentions and side conversations.
- 📚 **Help centre depth**: suggestions while replying, translations, article versions, a community forum.
- 📱 **Native mobile apps** and years of marketplace integrations.

> [!NOTE]
> Leader columns are from public product information as of November 2026 and may change.

## 📍 Roadmap

### ✅ Recently shipped

- [x] **0.10.3.** Screens for brands, Mail registration, per-agreement calendars, held mail and mail that fails authentication.
- [x] **0.10.2.** Hours per agreement, inbound mail through Mail's registration, the platform's tags, calendars and files, public help-centre counts, brands.
- [x] **0.10.1.** Screens for the ticket page, macros, queues, administration, the help centre and reports.
- [x] **0.10.0.** Inbound email, targets on the platform's clock, tags, routing by skill and shift, macros, satisfaction surveys, a help centre and new reports.
- [x] **0.9.1 and 0.9.0.** Merge duplicate tickets, with the screens.
- [x] **0.8.0.** Work outside a customer's entitlement held for approval.

### 🔜 Next up

- [ ] **Public web form** that opens a ticket without sign-in, with a rate limit.
- [ ] **Live chat** on the customer's site, because customers expect it.
- [ ] **WhatsApp and SMS** conversations in the inbox.
- [ ] **Article suggestions** while an agent replies, with article translations.
- [ ] **AI help**: a suggested reply, a ticket summary and triage.
- [ ] **Custom statuses** that drive the clock and the queues.
- [ ] **Parent, child and linked tickets**, and a problem-and-incident link.

### 🔭 Later

**Channels**
- [ ] Phone: click to call, call log and voicemail on the ticket.
- [ ] Social messages (Facebook, Instagram, X) into tickets.
- [ ] Merge by replying to a mail.

**Working a ticket**
- [ ] Split a message into a new ticket.
- [ ] Who is viewing or replying now.
- [ ] Mentions on internal notes and side conversations with an outside party.
- [ ] Time tracking and billable hours against a contract.
- [ ] A Merged mark on the inbox rows.

**Service levels**
- [ ] More clocks: next reply and periodic update.
- [ ] Agreements chosen by channel, tag or brand.
- [ ] Escalations that can do anything an automation can.

**Customers**
- [ ] A richer portal: company-wide view, reopen, status words of your own.
- [ ] NPS and CES surveys.
- [ ] Several brands with their own portals and help centres.
- [ ] Community forum.

**Help centre**
- [ ] Article versions.

**Platform**
- [ ] Native mobile app with push.
- [ ] Review of the Arabic and Hindi words by native speakers.

## 🚀 Install

Helpdesk is a native Ferrum app, compiled into the Ferrum server. Workspace administrators install it from **Apps**
(`/store/`); it ships in the official Ferrum server image. Self-hosters building their own image enable the
`helpdesk` feature of the server. The ERP connector (`helpdesk-erp`) is offered where Helpdesk and ERP are both installed.

1. Install Helpdesk. For email, install Mail (0.20 or later) and a mailbox account. Do not set the mailbox's
   "create records" option: Helpdesk's addresses do that.
2. Create a Support Team with its agents, and give people the roles: Support Agent, Support Manager or Knowledge Base Editor.
3. Add a Support Address for each address customers write to, with its team and, if you like, a brand. Check it shows as registered with Mail.
4. Open Support Settings and choose what happens to unknown senders and unverified mail.
5. Set the working hours in the platform's calendars (`helpdesk_hours`) and add service agreements with targets per priority.
6. Add assignment rules, agent profiles (skills, shift and signature), saved replies and macros.
7. Optionally switch on the satisfaction survey and publish articles to the help centre.

## 🔧 Develop

Clone it next to the platform:

```text
work/
├── ferrum/                  https://github.com/CBFerrum/Ferrum (with Ferrum-Frontend cloned into ferrum/web)
└── ferrum-app-helpdesk/
```

Point the platform dependencies at your checkout (this file is git-ignored):

```toml
# .cargo/config.toml
[patch."https://github.com/CBFerrum/Ferrum"]
ferrum-apps = { path = "../ferrum/crates/apps" }
ferrum-domain = { path = "../ferrum/crates/domain" }
ferrum-kernel = { path = "../ferrum/crates/kernel" }
ferrum-meta = { path = "../ferrum/crates/meta" }
```

The tests need PostgreSQL with an ordinary (non-superuser) role, so row-level security applies:

```bash
createuser ferrum --pwprompt && createdb -O ferrum ferrum_helpdesk_test
export FERRUM_HELPDESK_TEST_DATABASE_URL=postgres://ferrum:ferrum@localhost:5432/ferrum_helpdesk_test
cargo fmt -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Its tests also build on [ferrum-app-erp](https://github.com/CBFerrum/ferrum-app-erp) and the Mail app, which Cargo takes
from GitHub. To work on them alongside this app, clone them into the same folder and add to the same file:

```toml
[patch."https://github.com/CBFerrum/ferrum-app-erp"]
ferrum-app-erp = { path = "../ferrum-app-erp" }
```

The interface lives in `ui/` and builds with Ferrum-Frontend's packages, checked out as `../ferrum/web`:

```bash
cd ui
npm run build       # writes ui/dist, checked against the size budget
npm run check
npm run typecheck
npm test
```

With a development server started with `FERRUM_DEV_UI=1`, `ferrum-app dev --ui` serves the pages from Vite behind the
real bar and session, with hot reload.

## 📂 Project layout

```text
app.yaml              the manifest: id, version, roles, shell entries, notifications, assistant notes
connectors/erp/       the connector that adds a ticket's invoice and a contract's order
packages/helpdesk/    entities (support, desk, inbound), roles, messages, seeds, sql/001_targets.sql
packages/helpdesk_erp/ the two link fields
src/                  ticket, inbound, outgoing, mail, routing, hours, macros, tags, merge, survey,
                      help, site (portal), warranty, visits, reports
ui/src/               agent inbox, ticket workspace, portal, help centre, reports, admin/, locales ar and hi
tests/                integration tests against a real PostgreSQL
CHANGELOG.md          what changed in each version
```

## 📜 Changelog

Every release is described in [CHANGELOG.md](CHANGELOG.md).

## 🤝 Contributing

Issues and pull requests are welcome at [github.com/CBFerrum/ferrum-app-helpdesk/issues](https://github.com/CBFerrum/ferrum-app-helpdesk/issues).
Run the checks in [Develop](#-develop) first.

## 📄 License

AGPL-3.0-only. See [LICENSE](LICENSE).

<div align="center">

<sub>Part of <a href="https://github.com/CBFerrum/Ferrum">Ferrum</a>, the open business platform · Made with 🦀 Rust and 🐘 PostgreSQL</sub>

</div>
