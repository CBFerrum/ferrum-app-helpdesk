# Changelog

## 0.22.0

Needs the same platform as 0.21.0. The package is 0.13.0: Ticket gains a table of time entries (`TicketTime`) and `minutes_spent`, the platform's rollup of their minutes; a report Time Spent. Existing tickets have no entries. Rebuilt interface.

- **Time on tickets.** An entry is the minutes someone spent on a ticket, on a day, billable or not, with a note, written through the ticket's ordinary update (a person or an AI agent alike). An agent's entries are their own: logging for someone else, or changing or removing another person's entry, needs the right to delete tickets (a Support Manager). The total is the platform's rollup. A closed ticket takes no more time. Customers never see it.
- **Screens.** A Time card beside the ticket: the total, the entries, a form to log minutes and a start-and-stop timer kept in the browser (nothing is sent until Log time).
- **Report.** Time Spent: entries, minutes, hours and billable minutes by agent, customer, team, ticket or month, within the reader's teams.

## 0.21.0

Needs the same platform as 0.20.0. The package is 0.12.0: KbArticle gains `language`, `translation_of`, a read-only table of published versions (`KbArticleVersion`) and the action `restore_version`, with a unique key on `(translation_of, language)`. Existing articles have no language, no original and no versions until their next publication. Rebuilt interface.

- **Translations.** An article can translate another, one level, one per language (the database's unique key). The portal's search and suggestions, and agents' suggestions, offer one article per family, in the reader's language (or its base or variant), else the original; an article's page lists its other published languages. The public help centre is one list for every language until the public side knows a visitor's language.
- **Published versions.** Publishing, or changing a published article's title, summary or text, keeps a version (numbered upwards, the latest 50). Restore a Published Version writes one back as an ordinary edit; nothing is removed.
- **Screens.** The editor takes a language and an original, starts a translation from `/help/new?of=…`, and shows Translations and Published versions with Restore; the portal's article page shows "Also in".

## 0.20.0

Needs the same platform as 0.19.0. The package is 0.11.0: Ticket gains the hidden, read-only action `suggest_articles` (a query). No database change. Rebuilt interface.

- **Article suggestions while replying.** Articles in the composer offers up to eight published articles that fit the ticket's subject and the customer's latest words (or the words typed), Internal ones included, as far as the agent may read articles. Open one in a new tab, or insert a link to one a customer may read: the public help centre's page for a Public article, the portal's for a Customers Only one. An Internal article cannot be linked.
- **One search.** The portal's article search and the agents' suggestions are one query (`help::search`); the portal still sees only Public and Customers Only articles.

## 0.19.0

Needs the same platform as 0.18.0. No database change (the package stays 0.10.0). Rebuilt interface.

- **Related tickets** beside a ticket: its parent, its children and the tickets linked to it either way, each with its status. Make a child ticket (for the same customer and contact), link or unlink a ticket by its number, move the ticket under a parent or out of it. The server's refusals (a parent with open children, a child under a resolved parent) are shown in its words.
- **Split into a new ticket** on a customer's public message; a message split off shows "Split into" with the new number.

## 0.18.0

Needs the same platform as 0.17.0. The package is 0.10.0: Ticket gains `parent_ticket` and a `links` table (`TicketLink`), a message gains a read-only `split_into`, and Ticket a `split_message` action. Columns are added empty. No interface change yet (the screens come next).

- **Parent and child.** A ticket can name a parent through the ordinary create and update; one level only (a child has no children). A parent cannot be resolved or closed while a child is open, and an open ticket cannot be put under a resolved parent: the parent's row lock orders the two. A customer's reply reopening a child is never refused.
- **Linked tickets.** A link is one row on the ticket where it was made, found from the other with the list filter `["links.ticket", "=", id]`. Not to itself, not twice, at most 50.
- **Split a message.** Split a Message Into a New Ticket makes a ticket of a customer's public message, as the person, for the same customer and contact, linked to the original; the message stays and is marked with the new number, once.
- **Merge** carries children and links to the surviving ticket, dropping a link that would name it twice or from itself; merging a parent into its own child is refused.

## 0.17.0

Needs the same platform as 0.16.0. The package is 0.9.0: an escalation rule may have a condition, and Ticket has a read-only `last_customer_message` (filled at each ticket's next save). No interface change.

- **Escalation by condition.** A rule's trigger can be Condition: a CEL condition on the ticket alone decides (reopened twice, a late next reply, a language, a customer). With At Risk or Breached, a condition narrows the rule. A condition that does not compile is refused when the rule is saved. A rule still fires once per ticket, chosen again under the ticket's row lock.
- **What the escalation changed.** The ticket's `updated` event from an escalation carries `was`, so a workspace's automation can tell an escalation from any other change and tell a role, message the customer or apply a macro.
- **The customer's latest words** are a field, `last_customer_message`, for tag rules, assignment rules, escalation conditions and automations. A trigger on what the customer wrote is the platform's automation calling Apply a Macro through the gate (the README gives the recipe).

## 0.16.0

Needs the same platform as 0.15.0. The package is 0.8.0: two optional hours on an agreement's targets and ten read-only fields on Ticket (added empty), with two more of the platform's service targets. Rebuilt interface.

- **Next reply and customer update.** An agreement may set, per priority, Next Reply Time (each later message from the customer answered within so much working time) and Update the Customer Every (the longest a customer goes without a public message while the ticket is Open or On Hold). Both run on the platform's service targets on the agreement's calendar, are shown beside the ticket, breach once and raise the ticket's `updated` event, count in the SLA status, and are counted in SLA Compliance (Next Reply Late, Update Missed). An agreement without them is as before; open tickets take up new hours at the next upkeep.
- **Known limit.** On a ticket kept On Hold, a second missed update in the same round moves the due moment but is not counted again: the platform records one breach per round.
- The inbox's due moment is the earlier of the next reply and the resolution once the ticket has been answered.

## 0.15.0

Needs the same platform as 0.14.1. The package is 0.7.0: one new master, `TicketStatus`, and one new field on Ticket, `custom_status` (added empty: an existing ticket has none). Rebuilt interface.

- **A workspace's own statuses.** A Support Manager adds statuses such as "Waiting on supplier" (On Hold) or "Fixed in next release" (Resolved), each inside one of four stages: Open, Replied, On Hold, Resolved. Choosing one on a ticket (by a person, a macro's Set Status step or an agent through `helpdesk.ticket.update`) moves the ticket to its stage; a change of stage it no longer fits (a customer's reply reopening the ticket) takes it off; a ticket closed from Resolved keeps the Resolved-stage status it had. Queues, service targets, the portal and reports keep reading the stage, so nothing that worked changes. A status's stage cannot change while tickets have it, and the five stage names cannot be used as a status's name.
- **Screens.** Administration, Statuses; the ticket's header and the inbox show the workspace status; the details beside a ticket offer it; resolving offers "Resolved as"; a macro's Set Status lists the workspace's statuses. The ticket summary report groups by Workspace Status.

## 0.14.1

Needs the same platform as 0.14.0. No database change. No interface change.

- **Every action says what done means.** The fourteen actions (merge, macro, rating, warranty claim, outside-entitlement decisions, saved replies, helpful votes, maintenance visits, held mail) carry a `contract:`, so a process or an AI agent finds in the catalog what each one promises and the shape of what it returns. The package is 0.6.0.
- **Fix:** Create Maintenance Visit on a schedule took a `schedule_detail` parameter it never declared; it is declared now.

## 0.14.0

Needs Ferrum 0.4.0 (apps' upkeep runs one workspace at a time, on whichever replica is free), and builds against Mail (Comms) 0.27.0. No database or interface change.

**Upkeep is one workspace's.** `maintenance(k, tenant)` does Helpdesk's upkeep (hours, mailboxes, SLAs, surveys, escalation, auto-close) for the one workspace the platform hands it; the list of workspaces and the installation check are the platform's.

## 0.13.4

Needs the same platform as 0.13.3, and builds against Mail (Comms) 0.26.0. No database change. No interface change.

- **Mail 0.26.** Registering a support address with Mail takes the kernel now, so that Mail can ask the platform whether the workspace proved its Mail Domain. A support address on an outgoing email account's domain registers as before; one on a Mail Domain that is only a setting, not a proven claim, is refused with the reason on the address.

## 0.13.3

Needs the same platform as 0.13.2. No database change. No interface change.

- **The workspace's day, not UTC's.** Routing skipped agents away on the UTC day (the database's `current_date`), a new ticket's support contract was looked up on the UTC date of its opening, and a warranty claim was resolved on the UTC day. A workspace east or west of UTC routed to an agent away that day, or passed over one back from leave, for part of every day; a ticket opened on a contract's first or last day was found uncovered; a claim closed in the evening west of UTC was resolved on tomorrow's date. All three follow the workspace's day now.

## 0.13.2

Needs the same platform as 0.13.1, and builds against Mail (Comms) 0.25.0. No database change. No interface change.

- **No marker on Helpdesk's mail.** Mail 0.25 keeps no `x-ferrum-` header a sender wrote, so `X-Ferrum-Helpdesk` never
  reached Helpdesk on a received mail. That marker was also never proof: anyone could write it to have their mail
  dropped. Received mail is not checked for it any more, and Helpdesk's replies no longer carry it.
- **Helpdesk's own reply coming back is still refused.** A reply forwarded back to a support address, or echoed by
  a list, keeps the Message-ID Mail gave it. Mail already holds that message, so it takes the copy as a duplicate and
  never hands it to Helpdesk. This holds even when the reply went out from an account that is not a support address,
  such as a brand's sender. A suite now shows it.
- **Its own reply under another Message-ID is refused too.** A relay that gives mail a Message-ID of its own (Amazon SES
  does) sent Helpdesk's reply back unrecognised. It was taken in as the customer's message, reopened the ticket, and
  put the brand's sender among the ticket's copies. Mail from an address Helpdesk's replies go out from is now its own,
  as mail from a support address is. That covers each support address's Mail account and Mail's default outgoing
  account. It is logged as "Own address" and never taken in. One limit is known: when Mail has no outgoing account,
  replies go through the server's own mailer under the server's sender, which Helpdesk cannot know, so that echo is
  still taken in.

## 0.13.1

Needs the same platform as 0.13.0. No database change. Interface only, `ui/dist` rebuilt with Ferrum-Frontend `06bd1a2`.

- A customer's tickets in the portal say when they were opened and when each message came on the workspace's clock
  (or the person's own); it was the browser's. The reports' periods count back from the workspace's today.
- The Arabic and Hindi names of what 0.13.0 removed (paused hours, the agreements' offset, working hours and
  holidays) are gone.
- The two routes to the screens apps no longer have (`apps/:app/settings`, `x/:app/:page`) are gone.
- The page starts through the frontend's `mountApp`; the
  interface lint has no baseline file any more.

## 0.13.0

Helpdesk 0.5.0. Nothing is launched, so nothing is kept any more for what an older release left in a workspace. The package's SQL changed (`helpdesk/001_targets.sql` is gone; `helpdesk/002_mail_tokens.sql` has no `kind` column): a cluster set up before this is set up again from empty. No interface change.

- **One secret in a ticket's mail.** It is 32 hexadecimal digits made from the ticket with Helpdesk's key in the vault and found by its hash. A secret of any other shape in a reply's address or subject tag is malformed and finds nothing, so the mail is a ticket of its own. 0.10's secret kept on the ticket (`mail_token`) and 0.11's made with the server's key (`mail_token_hash`, the upkeep that recorded them, `helpdesk_mail_token.kind`) are no longer read. A reply still finds its ticket by the mail it answers, the secret in its address or the subject's tag.
- **Calendars by name.** An agreement's hidden hours, holidays and offset from before 0.10 (`time_zone_offset`, `SlaWorkingHour`, `SlaHoliday`) are gone, with what made calendars of them. An agreement names one of the platform's calendars; `helpdesk_hours` is made round the clock in UTC when a ticket needs it and none exists, and is the administrator's from then on.
- **No once-only conversions.** Brands made from addresses' labels, counting added to an older help centre and their record (`SupportSettings.upkeep_done`), the probes for a Mail older than 0.20, the targets backfill for 0.9's tickets and the ticket's `paused_hours` are gone.
- **A mail without Mail's verdict is not verified.** Mail stamps every message it receives (`x-ferrum-auth`); one without the stamp counts as unproven, as one without an authentication result does, where it used to count as verified for mail from before Mail 0.20.

## 0.12.2

A ticket's customer is found as ERP's portal finds the customers an address belongs to (`ferrum_app_erp::portal::customers_of`), in place of a copy of Helpdesk's own that had missed two of ERP's rules. A customer a web shop made at a guest checkout under the address counted as the address's, though anyone may type anyone's address there; so did a disabled customer one of its contacts names. A ticket raised on the portal or mailed in from such an address was that customer's, under its contract, agreements and assignment rules, and the sender counted as a known customer, so their mail opened a ticket where Support Settings ignore or hold mail from unknown senders. Now such a ticket has no customer and such mail is an unknown sender's. Where an address has several customers, the oldest by its own email is still taken before a contact's. ERP is now a dependency of the app, not of its tests only; it need not be installed, since the customers and contacts it reads are the platform's. No database or interface change.

## 0.12.1

Helpdesk no longer names the server's key (`FERRUM_SECRET_KEY`), which the platform is making its own. The secrets 0.11 put in the mail it sent (and the hash it kept of each) are checked through Helpdesk's own handle, which gives their fingerprints under each old key the platform still holds (`Vault::old_fingerprints`, contexts `helpdesk mail token` and `helpdesk mail token hash`, on the platform's main from 0ce3017): never to make a secret, only to find the ticket a reply answers. Such a secret does not lapse (a customer may answer months later), so upkeep still records each one's hash for its ticket, once (`helpdesk_mail_token`); once every ticket is recorded, nothing needs the old key, and a reply carrying it still finds its ticket after the platform lets the key go. Needs that platform (0.3.8 from 0ce3017). No database or interface change.

## 0.12.0

Helpdesk 0.4.0. Needs a platform with the vault (Ferrum `8e6b1f0` or later). A database change
(`helpdesk/002_mail_tokens.sql`: the table `helpdesk_mail_token`) is applied at startup; nothing existing moves. No
interface change.

- **The secret in a ticket's mail comes from Helpdesk's own key.** 0.11 made it with the server's key
  (`settings.secret_key`), which the platform is making its own. It is now made from the ticket with a key derived from
  Helpdesk's key in the workspace's vault (32 hexadecimal digits, as before), the same each time while that key's version
  stands, and kept nowhere. What finds the ticket by it is a SHA-256 of the workspace and the secret, recorded when the
  ticket is made and again as mail carrying it goes (`helpdesk_mail_token`): after Helpdesk's key is rotated (`ferrum task vault_rotate level:owner …
  owner:helpdesk`) the next mail carries a new secret, and mail sent before still finds its ticket by its own. The
  ticket's `mail_token_hash` (0.11's keyed hash) is no longer written.
- **Mail already sent keeps finding its ticket.** A secret 0.11 made with the server's key is still matched with that
  key, and upkeep records its hash for every ticket 0.11 kept one for (with each retired key too), once, so that it keeps
  matching when the server's key is no longer the apps' to use. The secret releases before 0.11.0 kept in the ticket's
  `mail_token` column is still read for it. The build before this one, should it run on the same database, matches the
  secrets it made as before, and the mail this release sends by the mail it answers.
- **When the vault is closed** (an installation with nothing to open its root with), mail about a ticket goes without a
  secret, as on a server without a key: a customer's answer finds the ticket by the mail it answers. No secret is made
  another way.
- **A sandbox** has keys of its own: the hashes copied with it cover the live workspace's id, so the secrets in the live
  workspace's mail find nothing in the copy; the copy's own mail carries its own secrets.
- **A workspace moved to another installation**: its recorded hashes travel with it, so mail already sent keeps finding
  its tickets there; the secrets made from then on come from the new installation's vault.
- **For other apps' tests**: Helpdesk takes its vault handle as it registers, so register it as the server does,
  `hooks.app("helpdesk", &[], ferrum_app_helpdesk::register)`; called outside that, the kernel refuses to boot.
  `inbound::ticket_token` is now asynchronous and says when the vault gives no secret.
- **Tests.** Every suite opens the vault and registers the apps as the server does. A new part of the inbound suite: a
  secret is made with Helpdesk's key, is not 0.11's, and is kept only as a hash; 0.11's secret finds its ticket by the
  server's key and, once upkeep has recorded it (once), without it, by address and by tag; a replica whose vault is
  closed sends the reply without a secret, and the answer finds the ticket by the mail it answers.

## 0.11.0

Helpdesk 0.3.0. A ticket no longer carries the secret that lets a reply into it.

- **The secret in a ticket's mail is made, not kept.** The secret in the subject tag (`[TKT-2026-000123/…]`) and in the
  Reply-To (`support+…@`) of the mail sent about a ticket was a field of the ticket (`mail_token`), so whoever could
  read tickets, through the API, an export, a report or the audit trail, could write a reply into any of them as its
  customer. It is now made from the ticket with the server's key (the same each time, 32 letters and digits) and kept
  nowhere. The ticket keeps a keyed hash of it (`mail_token_hash`, hidden), by which a reply written to the tagged
  address finds it; a subject tag is checked against the ticket whose number it gives. After the server's key changes,
  mail sent before still finds its ticket by its tag (the key being rotated out makes its secret) and by the mail it
  answers. A server without a key (development only) gives no secret: replies are then matched by the mail they
  answer.
- **Upgrading.** Nothing to do. Mail sent before carries the secrets the tickets kept: their column stays in the
  database, no longer a field (so no longer shown), and a reply carrying one still finds its ticket. A ticket keeps the
  hash from its next save on, which every reply that is mailed makes first. The reply's template (`helpdesk.reply`)
  names the secret as one of its values, `values.token`; a workspace's own copy that names `doc.mail_token` keeps
  sending as before, and when it is next edited the check says the ticket has no such field, to be written
  `values.token`. A server of the release before, still running during the upgrade, gives the tickets it makes a kept
  secret again, which this release still reads.
- **What still carries it:** the mail itself, and the Mail app's copy of what was sent (its subject and Reply-To), which
  whoever may read the sent mail reads.
- **Tests.** Every suite's kernel has a server key, as a production server must.

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

Rebuilt with Ferrum-Frontend 9e7190c, the design system as tuned after the landed build was reviewed. The left menu's icons are chosen so that none repeats on more than three rows and each fits its row; their drawings load beside the first page instead of inside it, so the page starts lighter; a row that scrolls sideways fades at the edge where there is more, in every browser and with reduced motion; a printed record leaves out empty fields and keeps its totals with the table; menus and popovers sit inside the page's landmarks for assistive technology; a selection bar folds what does not fit into More on a narrow screen. Nothing else changes.

## 0.8.1

The interface follows the upgraded design system, rebuilt with Ferrum-Frontend 76c4fad. Text uses the kit's type roles (nothing under 12px) and controls its height tokens; loading shows skeletons shaped like what is coming; empty, no matches, denied, offline and failed are told apart; status colours come from stored state ids. The interface check now runs the UI lint against an empty baseline. Ticket status, priority and SLA colours come from one map of stored ids.

## 0.8.0

Work outside a customer's entitlement follows the policy the workspace chooses (Ferrum's closure review, FEO-030). Support Settings' Tickets Outside Entitlement is Inform (as before: the ticket is worked, marked Expired, Exhausted or None) or Hold for Approval: a ticket from a customer whose contract has ended or run out of tickets, or who has none, then waits On Hold. No reply goes to the customer and only the decision moves it, though internal notes can be written. A Support Manager approves the work (it opens again) or declines it (it is resolved, saying why), from the ticket's menu; the decision, who made it and why are kept on the ticket and in its history. A save race between two agents already kept the loser's words.

## 0.7.13

The reports it defines in code (their titles and parameters) have names in Arabic and Hindi too, with its models' and manifest's (304 each in ui/src/locales/names). Translated by the model, to be reviewed by native speakers.

## 0.7.12

Rebuilt with Ferrum-Frontend 43d7779: the names of its models, fields and manifest in Arabic and Hindi (303 each in ui/src/locales/names, which the platform serves to every page of the workspace, so a record's labels, the launcher and the bar read in the person's language), and the sentences its pages still showed in English (split around a link or a figure, built in code from a template or word by word) whole and translated (270 words each in ui/src/locales). Translated by the model, to be reviewed by native speakers.

## 0.7.11

Rebuilt with Ferrum-Frontend 046a3a2: the sentences its pages built in code (counts, dates, what a toast or a screen reader says) go through t() too, as whole sentences where code chose a word, and carry built-in Arabic and Hindi (219 words each in ui/src/locales, translated by the model and to be reviewed by native speakers).

## 0.7.10

The assistant knows what its records mean in its pages (contributes.shell.assistant): their states, what counts and where figures come from, with questions it suggests asking there.

## 0.7.9

Rebuilt with Ferrum-Frontend 389b2af: its own words carry built-in Arabic and Hindi (188 words each, ui/src/locales, translated by the model and to be reviewed by native speakers), fetched for the page's language only. The labels it keeps in objects, its dialogs, its page titles and the words it keeps in variables go through t() as well, and the names and sizes it shows as they are keep their order right to left. Its pages are drawn once their words are in, and its entry carries only the parts of the kit it uses.

## 0.7.8

Rebuilt with Ferrum-Frontend 1f5981d: an action's link, code or token shown once is kept in a dialog to copy, and on a page that reads right to left, English left untranslated keeps its own order. Its own words go through the kit's t() (185 places), so a workspace's own words for its language reach them, and Admin, Interface words lists them under it (dist/words.json).

## 0.7.7

Rebuilt with Ferrum-Frontend 8a1c2e7: a workspace's own words for its interface (Admin, Interface words) reach its pages as they load; an attachment field that holds a picture shows it; and a value another record already has is said so, where the form said the record had been changed by someone else. Its own screens' margins, paddings, borders, alignment and insets go by the line's start and end, so they mirror right to left as the kit's do; what it places by coordinates (boards, timelines, charts, handles) stays as it was.

## 0.7.6

Rebuilt with Ferrum-Frontend 9d80f4f: right to left, its pages mirror as they read (icons that point along the line, margins and alignment by the line's start and end, arrow keys along it), and the records pages' words are in Arabic and Hindi.

## 0.7.5

Rebuilt with Ferrum-Frontend 204b713: a new record's box whose default is 1 shows ticked, as it is saved.

## 0.7.4

Rebuilt with Ferrum-Frontend 6cfda39: an action's dialog says every refusal, the errors on what the action made (another record's rows) among them, and an action with parameters and a confirmation asks once.

## 0.7.3

Rebuilt with Ferrum-Frontend b39fffb: its pages show times on a person's own clock when they chose one on their Account page, while today and due dates stay on the workspace's day.

## 0.7.2

A ticket's times, its service level's due times and the conversation's day dividers are on the workspace's clock, naming its zone when the browser's is another. The customer's portal keeps the customer's own. Rebuilt with Ferrum-Frontend d350579, so its records' pages show times on the workspace's clock too, a draft numbered on submit is headed as a draft, and a record of an app that is not installed says so.

## 0.7.1

Rebuilt with Ferrum-Frontend 20d08e5: a record page reads an empty number or check as the server does, so an action whose condition needs one shows, and a workspace's settings open as their one record.

## 0.7.0

Files on the portal stream: a file sent with a ticket's message is sent by the server straight from storage, whatever its size, instead of being read whole into memory first (Ferrum's SiteResponse::file, FERRUM-OPS-006).

## 0.6.3

Rebuilt with Ferrum-Frontend ae13c69: in a workflow state that names what may change, the other fields show read-only, and the fields the state needs show required.

## 0.6.2

Rebuilt with Ferrum-Frontend daeadda: after an action runs, a record page shows what the action said it did rather than only that it is done.

## 0.6.1

Rebuilt with Ferrum-Frontend 89b7cc7: a field's label names its control for assistive technology, and a record whose title is a link shows that record's title, not its id.

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

- Rebuilt with Ferrum-Frontend db924b9: under the platform's bar the records frame no longer shows
  a second bell (the bar has notifications).

## 0.3.1

- Its notifications have kinds each person chooses for (Ferrum's bell, apps-ui.md U5): tickets
  assigned to you, customer replies on your tickets and SLA breaches on them, in the bar and by
  email unless chosen otherwise. Requires the platform with notification kinds (Ferrum d22938f).

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
