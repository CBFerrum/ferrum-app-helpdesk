# Changelog

## 0.10.3

The interface gains the screens that 0.10.2 listed, built with Ferrum-Frontend (its checkout at `gapui/ferrum/web`). Nothing is changed on the server.

- **Administration.** Support addresses show the Brand (a pick from Brands; the old `label` stays as a note called Label), whether Mail hands the address mail (`mail_registered`) and, when it does not, Mail's own reason (`registration_note`). A new Brands section lists and edits brands (name, name the customer sees, signature, header, footer) with a sample reply wrapped in what is typed, in the server's order. A new Service agreements section picks each agreement's Working Calendar from the platform's calendars (empty: the workspace's `helpdesk_hours`, said in words; a calendar that was deleted stays visible instead of being dropped) and links to the agreement's own form for targets and criteria. Mail that fails authentication (Hold Failed, Hold All Unverified, Accept) has a card of its own beside Unknown senders, one plain sentence for each option.
- **The inbound log** shows a held mail's text, as plain text, to the roles that may read it (the field group `held_mail`), tells everyone else it is not theirs to read, says why mail from an unverified sender waits, and says staff alone release held mail.
- **The ticket.** A message whose sender the mail server did not verify (`sender_unverified`) carries a small Sender not verified mark. The two target cards name the calendar they count on (`sla_calendar`, else `helpdesk_hours`), and the reports' note no longer assumes one calendar for all tickets.
- **Reports and help centre.** Help Centre by Article is in the reports list (it has no grouping and no period, and opens at once); the article editor says that views and votes count the portal and the public page together.

Not in this release: the screens were not run against a server; the field group's right is read from whether the server sends the text, not from the group itself; the agreement's targets, criteria and pauses stay in its own form (it has a table of targets); a Brands pick in a new address's form needs the Brand to exist first; 60 new words are in English until translated into Arabic and Hindi.

## 0.10.2

Everything 0.10.0 gave up or worked around that the platform (App API 0.3.4) and Mail (0.20.0) now offer. Server side only: the interface is not rebuilt, and the screens that must follow are listed at the end. Needs platform 0.3.4 and, for mail, Mail 0.20.

- **Hours per agreement are back.** A service agreement names its own working calendar (`ServiceLevelAgreement.calendar`; empty: the workspace's `helpdesk_hours`), and its tickets count on it (`calendar_field` on both targets, through the ticket's `sla_calendar`, set from the agreement that applies and only when the calendar is there, so a calendar an administrator deleted never refuses a ticket's save). A target's clock starts from the moment the ticket was opened (`start_field`, through `sla_start`: a backdated or imported ticket is due by its own opening, and the sweep records the breach once; a reopened ticket's resolution starts again from the reopening, not from the opening). When a workspace is updated the upkeep makes one calendar for each agreement that had its own hours, holidays or time zone before 0.10, or existed before 0.10 (when an agreement with no hours counted round the clock), from what 0.10 kept of them in the agreement's hidden fields (`time_zone_offset`, `working_hours`, `holidays`): named `sla_<its name>`, once for the workspace. The agreement `helpdesk_hours` came from keeps using it (administrators have edited it since). Open tickets keep their elapsed and paused time: the platform counts the time already used on the calendar it ran under and the rest on the agreement's from then on; a round the platform had not yet anchored keeps the due moment it had. What could not be recovered: a fixed UTC offset that no zone without daylight saving has (for example -3:30) becomes UTC and is said in the log; the offsets whole hours and a few others (Kabul, Tehran, Kolkata, Kathmandu, Yangon, Eucla, Darwin, the Marquesas) become the zone that never changes; an agreement made since 0.10 with no hours of its own has nothing to recover and stays on `helpdesk_hours`, as it is indistinguishable from one 0.10 never gave hours to.
- **Inbound mail through Mail's registration.** The hook on `Communication` is gone. Each enabled support address is registered with Mail (`routes::register`), kept in step as addresses are added, changed, disabled and deleted, and by the upkeep for every address a workspace already had (taking back a registration nothing stands behind; `SupportAddress.mail_registered` and `registration_note` say whether Mail took it and why not). Mail hands each message once, with its headers; mail is never handled twice. Threading: the Message-ID it answers, then the ticket Mail threaded it onto, then the secret in the address it was written to (`support+<secret>@…`), and the subject's tag last. Auto-replies, bounces (also `Return-Path: <>` and delivery-status reports) and list mail are recognised from the stored headers. Mail's `x-ferrum-auth` is `unverified` both when a check failed and when the server gave no result, so Helpdesk reads the stored `authentication-results` to tell them apart: a result that failed (DMARC failed, SPF and DKIM both failed, or a DKIM pass for another domain with SPF not passing) from a known customer, or the requester or a copy of the ticket it answers, is held (`unverified_senders`: Hold Failed, the default; Hold All Unverified; Accept; reason Unverified sender); mail with no result is taken and marked `sender_unverified` on the conversation row; released by staff it is not held again. Workspaces whose settings have no value get Hold Failed, so a mail server that adds no results keeps receiving customer replies. Replies go out through `outbound::send` with Reply-To the tagged address, In-Reply-To, References and the support address's account, and the Message-ID Mail gave is stored on the conversation row. A mail's files are copied onto the ticket (Mail keeps its own).
- **The kernel's calls instead of the platform's tables.** Tags (tag rules, macros) through `tag_tx`, the calendars through `ensure_calendar_tx`, files through `attach_files_tx`. Not covered: merging tickets still moves comments, assignments, files, tags and watchers with its own statements (`src/merge.rs`; no call moves a record's tags or files to another record), and tag rules read the ticket's tags to raise no event when nothing is added.
- **The help centre counts its public visitors.** The published list counts views and "was this helpful" votes (`count_views`, `votes`) into the article's `views`, `helpful_count` and `not_helpful_count`, beside the portal's own count; a list made by 0.10.0 gets them once, keeping an administrator's changes. New report Help Centre by Article (views, helpful, not helpful, helpful %), by status and visibility.
- **Small things.** Brands (`SupportBrand`: name, display name, signature, header, footer) on a support address: the name on the Reply-To, the signature after the agent's, the wrapper around the reply; made once from the labels addresses already had. The held mail's body is a field of its own group (`InboundLog.body`, field group `held_mail`: administrators, or a role given it), erased with the sender. A ticket nobody has answered counts `reassign_unanswered_hours` in the working hours of its agreement's calendar. The survey is not sent to an address that opted out of marketing mail or whose processing is restricted (`Kernel::opted_out`).

Screens that must follow: Administration, Support addresses (Brand, Registered With Mail, Registration Problem; the field `label` is no longer the brand) and a Brands list; Administration, Service agreements (Working Calendar, a pick from Admin, Calendars) and the target cards, which say "working time" without naming the calendar; the inbound log (Held Mail body for administrators, the reason Unverified sender, a note that held mail is released by staff only); Support Settings (Mail That Fails Authentication, three options) and the message's Sender Not Verified mark; the report list (Help Centre by Article); the help centre editor (views and votes now include the public page's).

## 0.10.1

The interface gains the screens that 0.9.0 and 0.10.0 listed as still to do, built with Ferrum-Frontend (its checkout at `gapui/ferrum/web`). Nothing is changed on the server. Every screen reads what 0.10.0's routes and fields return; the places that assume the one workspace calendar (`helpdesk_hours`) and the one set of support addresses are named at the end.

- **The ticket page.** Messages say what they are: a public reply (emailed, or portal only), an internal note (never emailed, copied or shown to the customer). The box under the feed makes the kind explicit in words and in the button (Send reply by email, Add reply to the portal, Add internal note, not sent). The copies on a ticket (`cc`) are shown on the page and on each message and can be changed; tags are chips with the colour of their `TicketTag`, added and removed against the platform's tags; the two target cards read `response_state` and `resolution_state` (running, paused, met, met late, breached) with the target in working time, and no longer read `paused_hours`; a ticket that follows up a closed one links to it; the rating and its comment have a card; the assignment rule that placed the ticket is named, with when and how often it moved.
- **The box's helpers.** A saved reply is filled in by the server (`render`) and the preview is its answer, so what is read is what is sent. Macros: a picker with a preview of what each step will change on this ticket as it stands, a warning first when one sends the customer a reply, and `apply_macro` (sent once, with an idempotency key). The agent's signature is shown under a reply that will be emailed and edited in a dialog (`AgentProfile`).
- **Queues.** By tag (coloured) and by target state (first response or resolution: running, paused, met, met late, breached, with counts for the breached and paused), as filters and in the address; bulk tag and bulk macro from a selection (each ticket in its own transaction, the failures kept selected with their reason); the assignment rule that placed a ticket on its row.
- **Administration** (`/admin`): support addresses with what to do with unknown senders; the inbound log with release and discard of held mail; agent profiles; tags and tag rules; assignment rules in the order they are tried (Earlier and Later renumber their priorities) with skill and fallback; a macro editor with its steps; satisfaction survey settings with what they add up to, a preview of the message the workspace has (an example ticket put in) and of the rating page.
- **Help centre.** `/help`: the article list and an editor (category, body with a preview, visibility, state, publish and archive) that says who the article will reach. In the portal: article search, a reader with Did this help, and the articles offered under the subject while a request is written.
- **Reports.** `/reports`: the six questions a desk asks, each opening the platform's report with the grouping and period that answer it. The reports were already in the app's menu.

Not in this release, and why: the held mail's body (the log's `payload` is a hidden field; only sender, subject, outcome and reason are shown); a brand on a support address (the field is `label`, shown as Brand); a private reply as a third kind of message (the server has public replies and internal notes only); the screens were not run against a server, and 406 new words are in English until they are translated into Arabic and Hindi.

Assumes the single workspace calendar: the target cards say "working time" without naming a calendar, and the reports' times are in the workspace calendar's working hours; the agreements' own hours, holidays and offset (`SlaWorkingHour`, `SlaHoliday`, `time_zone_offset`) are not shown anywhere. Assumes mail is registered by a support address (`SupportAddress`, with the Mail account named as text): 0.10.1's server change to how inbound mail is registered and to how working hours attach to agreements will need the screens in Administration, Support addresses and the target cards looked at again.

## 0.10.0

The support desk closes the gaps an audit against Zendesk, Freshdesk, Intercom, Help Scout, Service Cloud and Front found. Server side only: the interface is not rebuilt, and the list of what its screens need is at the end.

- **Customers write to support and it lands in the ticket.** `SupportAddress` (an address, its brand, the team it feeds, the priority and type of what arrives, the Mail account replies go out from) says what an address means; the Mail app receives the mail. Mail arriving for one becomes a new ticket of the team, or a message on the ticket it answers, matched by the Message-ID it answers (In-Reply-To, References) or by the ticket's number and secret in the subject tag `[TKT-2026-000123/secret]` the app puts on outgoing mail, never by subject text alone; a reply to a merged ticket lands on the survivor, and mail on a closed (final) ticket opens a new one that says which it follows up (`related_ticket`). Copies (`cc` on the ticket) are kept and copied on replies; files stay on the message. The sender is matched to a customer by exact address (the customer's or a contact's); an unknown sender is the requester, held for review or ignored as Support Settings say (`unknown_senders`). Auto-replies, bounces, mailing-list mail, mail from a support address itself, mail carrying Helpdesk's own marker and mail over a sender's hourly limit (`inbound_per_hour`, 20) are recognised by their headers and addresses, never turned into tickets or answered, and kept in the Inbound Mail Log (`InboundLog`, administrators; held mail can be released into a ticket or discarded; kept `inbound_log_days`, 90; erased with the sender). Replies go out through Mail when it is installed (a `Communication` on the ticket: In-Reply-To and References from the customer's last message, the ticket's CC, the agent's signature from `AgentProfile`), else through the platform's mail to each address; the wording around the agent's words is the message template `helpdesk.reply`, which the workspace edits (a missing or broken one never stops a reply). Mail arrives through a hook on Mail's `Communication` that reads only the record Mail is saving; see the README for what Mail must still offer.
- **Targets on the platform's clock.** First response and resolution are service targets of the platform (`respond`, `resolve` on the Ticket) on the working calendar `helpdesk_hours`, with daylight saving and dated exceptions, stopped while a ticket is On Hold or waiting for the customer (per agreement), the breach recorded once and raising the ticket's updated event. `src/sla.rs` is gone. Agreements (`ServiceLevelAgreement`) carry over as records and gain a team criterion; the most specific still wins (customer, group, team, type, then the default), a contract's agreement first. The calendar is made, once and when absent, from the default agreement's hours, holidays and offset (the offset as the zone that never changes), else round the clock in UTC; an agreement's own hours, holidays and offset are no longer used. Open tickets keep the clock they had (`sql/001_targets.sql`): started when opened, due when they were due, a missed target stays missed, a response given stays given, a ticket waiting stays stopped. `sla_status`, `response_breached`, `resolution_breached`, `first_response_hours` and `resolution_hours` are kept up to date from the target states (At Risk after `at_risk_percent`, 75, of the time). A priority raised moves the due moment from the save. `paused_hours` is no longer worked out.
- **Tags, private notes.** `TicketTag` records with a colour, `TagRule` conditions that put a tag on a ticket when it is saved (never take one off). Private notes were already never mailed or shown in the portal; they are now also never copied, and a public reply with a private note in the same save still marks the ticket Replied.
- **Routing.** Assignment rules gain a skill (a fixed one, or the ticket's `language`), availability (agents away by the platform's away periods, or off shift by their `AgentProfile.shift_calendar`, are skipped) and a fallback agent. A reopened ticket whose agent cannot take it goes round again (`route_reopened`, on), and a ticket nobody has answered for `reassign_unanswered_hours` goes to the next agent, up to `max_reassignments` times (off by default). The rule's row stays locked, so two tickets at once take turns.
- **Canned replies and macros.** The action Fill In for a Ticket renders a saved reply's merge fields on the server; `SupportMacro` records of steps (set status, priority, team, type, language, assignee or resolution, add or remove a tag, public reply, private note) apply in one save as the person who presses it, all or none.
- **Satisfaction.** With `survey_enabled`, a solved ticket's customer is mailed (`helpdesk.survey`) a signed one-use link (`survey_link_days`, 30) to the action Rate the Help, `survey_delay_hours` after it was solved; the rating and comment are kept once (`rated_at`); a rating at or below `survey_reopen_at_or_below` reopens a resolved ticket, at or below `survey_alert_at_or_below` tells the agent and `survey_alert_to` (`helpdesk.survey_alert`). The portal's rating does the same, and is once.
- **A help centre.** Articles gain a visibility (Internal, Customers Only, Public), an address and a summary; Public, Published ones are the platform's published list `help-centre` (made once for the workspace by the upkeep, the workspace's own to change or take down). Customers in the portal search articles, are offered ones that fit as they start a request (`portal/suggest`), read them (views counted) and say whether one helped (once a day).
- **Reports.** Backlog by age, ticket volume by channel, tag, customer, team or month, reopen rate; satisfaction by agent, team and month, counted from the rating's own date.
- Not done: the interface's own screens; live chat, WhatsApp, text and phone as channels; a Reply-To that routes back and the headers of received mail (Mail's to offer); a per-agreement calendar; counting views and votes from a visitor who is not signed in.

## 0.9.1

The interface gains the screens for merging tickets, rebuilt with Ferrum-Frontend 1381367. In the inbox, select two or more tickets and Merge…: choose the ticket that stays (the oldest is offered), read what moves and that the others close as merged and cannot be undone, and confirm. On a ticket, More actions, Merge other tickets into this one… offers the customer's other open tickets to tick. Tickets of two customers, a closed ticket and a ticket into itself are told in plain words before asking, and the server's own refusals are shown as it words them. A merged ticket says Merged into the ticket that stayed, with a link, and has no box to reply in; in the survivor's conversation each message that came from a merged ticket is marked with that ticket's number. Words and names in Arabic and Hindi (23 words and 6 names more; translated by the model, to be reviewed by native speakers).

Not in this release: a Merged mark on the inbox rows, merging by e-mail reply, and the screens were not run against a server.

## 0.9.0

Merge duplicate tickets. The action Merge Tickets Into This One on the ticket that stays takes the numbers of the duplicates, which must be the same customer's (the same contact when there is no customer) and not closed. Their conversation moves in time order, each message marked in `merged_from` with the ticket it came from; their files, comments, assignments, watchers and tags move too, a tag or watcher both had staying once (the platform record merge's own statements). Each duplicate closes as merged (`merged_into`, `merged_at`, `merged_by`), and a note in both conversations says who merged and when. A customer's reply on the portal to a merged ticket lands on the survivor. The survivor's SLA fields are not reset and nothing that moved is sent again. The platform's record merge was not used: it hides the duplicate and does not move a record's own lines. A merge is not undone: a closed ticket is final, and undoing would mean moving messages back out of a conversation that is a record, so it is not offered. Merging by e-mail reply and merging in the inbox's own screens are not built.

## 0.8.2

Rebuilt with Ferrum-Frontend 45dad7f, the design system as tuned after the landed build was reviewed. The left menu's icons are chosen so that none repeats on more than three rows and each fits its row; their drawings load beside the first page instead of inside it, so the page starts lighter; a row that scrolls sideways fades at the edge where there is more, in every browser and with reduced motion; a printed record leaves out empty fields and keeps its totals with the table; menus and popovers sit inside the page's landmarks for assistive technology; a selection bar folds what does not fit into More on a narrow screen. Nothing else changes.

## 0.8.1

The interface follows the upgraded design system, rebuilt with Ferrum-Frontend 0a47157. Text uses the kit's type roles (nothing under 12px) and controls its height tokens; loading shows skeletons shaped like what is coming; empty, no matches, denied, offline and failed are told apart; status colours come from stored state ids. The interface check now runs the UI lint against an empty baseline. Ticket status, priority and SLA colours come from one map of stored ids.

## 0.8.0

Work outside a customer's entitlement follows the policy the workspace chooses (Ferrum's closure review, FEO-030). Support Settings' Tickets Outside Entitlement is Inform (as before: the ticket is worked, marked Expired, Exhausted or None) or Hold for Approval: a ticket from a customer whose contract has ended or run out of tickets, or who has none, then waits On Hold. No reply goes to the customer and only the decision moves it, though internal notes can be written. A Support Manager approves the work (it opens again) or declines it (it is resolved, saying why), from the ticket's menu; the decision, who made it and why are kept on the ticket and in its history. A save race between two agents already kept the loser's words.

## 0.7.13

The reports it defines in code (their titles and parameters) have names in Arabic and Hindi too, with its models' and manifest's (304 each in ui/src/locales/names). Translated by the model, to be reviewed by native speakers.

## 0.7.12

Rebuilt with Ferrum-Frontend c59c9a2: the names of its models, fields and manifest in Arabic and Hindi (303 each in ui/src/locales/names, which the platform serves to every page of the workspace, so a record's labels, the launcher and the bar read in the person's language), and the sentences its pages still showed in English (split around a link or a figure, built in code from a template or word by word) whole and translated (270 words each in ui/src/locales). Translated by the model, to be reviewed by native speakers.

## 0.7.11

Rebuilt with Ferrum-Frontend 155518c: the sentences its pages built in code (counts, dates, what a toast or a screen reader says) go through t() too, as whole sentences where code chose a word, and carry built-in Arabic and Hindi (219 words each in ui/src/locales, translated by the model and to be reviewed by native speakers).

## 0.7.10

The assistant knows what its records mean in its pages (contributes.shell.assistant): their states, what counts and where figures come from, with questions it suggests asking there.

## 0.7.9

Rebuilt with Ferrum-Frontend 5b22e7b: its own words carry built-in Arabic and Hindi (188 words each, ui/src/locales, translated by the model and to be reviewed by native speakers), fetched for the page's language only. The labels it keeps in objects, its dialogs, its page titles and the words it keeps in variables go through t() as well, and the names and sizes it shows as they are keep their order right to left. Its pages are drawn once their words are in, and its entry carries only the parts of the kit it uses.

## 0.7.8

Rebuilt with Ferrum-Frontend 6537d65: an action's link, code or token shown once is kept in a dialog to copy, and on a page that reads right to left, English left untranslated keeps its own order. Its own words go through the kit's t() (185 places), so a workspace's own words for its language reach them, and Admin, Interface words lists them under it (dist/words.json).

## 0.7.7

Rebuilt with Ferrum-Frontend d5d98f2: a workspace's own words for its interface (Admin, Interface words) reach its pages as they load; an attachment field that holds a picture shows it; and a value another record already has is said so, where the form said the record had been changed by someone else. Its own screens' margins, paddings, borders, alignment and insets go by the line's start and end, so they mirror right to left as the kit's do; what it places by coordinates (boards, timelines, charts, handles) stays as it was.

## 0.7.6

Rebuilt with Ferrum-Frontend 63280d3: right to left, its pages mirror as they read (icons that point along the line, margins and alignment by the line's start and end, arrow keys along it), and the records pages' words are in Arabic and Hindi.

## 0.7.5

Rebuilt with Ferrum-Frontend a64416a: a new record's box whose default is 1 shows ticked, as it is saved.

## 0.7.4

Rebuilt with Ferrum-Frontend 922fcfb: an action's dialog says every refusal, the errors on what the action made (another record's rows) among them, and an action with parameters and a confirmation asks once.

## 0.7.3

Rebuilt with Ferrum-Frontend a76295e: its pages show times on a person's own clock when they chose one on their Account page, while today and due dates stay on the workspace's day.

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
