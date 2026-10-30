# Ferrum Helpdesk

A native app for [Ferrum](https://github.com/CBFerrum/Ferrum), the open-source, AI-first ERP.

- Tickets with conversations, statuses, satisfaction ratings and resolution details
- Service level agreements on business hours: working hours, holidays, time zones, pausing statuses, breach tracking
- Teams and assignment rules (round robin, least loaded) safe under concurrent tickets
- Knowledge base with categories, publishing and helpful votes
- Warranty claims checked against serial-number deliveries; maintenance schedules and visits
- Reports: ticket summary, SLA compliance, response and resolution times, agent workload, satisfaction

## Install

Workspace administrators install it from **Apps** in Ferrum; it ships in the official Ferrum
server image. Self-hosters building their own image enable the `helpdesk` feature of the server.
Everything it adds is also available to AI agents over Ferrum's MCP server.

## Develop

Clone it next to the platform and point the platform dependencies at your checkout (this file
is git-ignored):

```toml
# .cargo/config.toml
[patch."https://github.com/CBFerrum/Ferrum"]
ferrum-kernel = { path = "../ferrum/crates/kernel" }
ferrum-meta = { path = "../ferrum/crates/meta" }
ferrum-domain = { path = "../ferrum/crates/domain" }
ferrum-calc = { path = "../ferrum/crates/calc" }
```

The tests need PostgreSQL with an ordinary (non-superuser) role — row-level security must apply:

```bash
createuser ferrum --pwprompt && createdb -O ferrum ferrum_helpdesk_test
```

```bash
cargo test
```

`FERRUM_HELPDESK_TEST_DATABASE_URL` overrides the database URL. The app's metadata lives in `packages/` and is embedded
at build time; `app.yaml` is its manifest (see Ferrum's `docs/plan/apps.md`).

## License

AGPL-3.0-only, like the platform.

## Interface

Helpdesk has its own interface at `/helpdesk/` (Ferrum docs/plan/apps-ui.md), in `ui/`: the agent
inbox (your tickets, the unassigned, SLA alerts and every open one, the most pressing first, kept
current as tickets change) and the ticket view (the conversation with replies and internal notes;
status, priority, assignment and SLA beside it). The other records are lists and forms in the
records frame. It builds with Ferrum-Frontend's packages from the sibling checkout
(`../../ferrum/web`), and `ui/dist` is committed and embedded in the crate:

```
cd ui && npm run build    # then commit ui/dist; CI checks it was built from ui/src as it is
```
