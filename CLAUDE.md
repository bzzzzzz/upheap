# Upheap

A ranked todo list for teams. Items go into a heap with a rough size, impact and due date, and the server ranks
them, explains each position, and keeps the ranking current as time passes. One binary serves the API and the
web app, on SQLite or MySQL.

## Components

- `crates/domain`: pure logic (types, item state machine, ranking, due labels, permissions). No IO.
- `crates/store`: SeaORM entities, migrations, repositories.
- `crates/service`: use cases. Each one runs in one transaction with its audit entry.
- `crates/api`: axum handlers, DTOs, error mapping, OpenAPI (utoipa).
- `crates/server`: the `upheap-server` binary (config, startup, `openapi` subcommand).
- `web/`: React + TanStack Query client, with types generated from `openapi.json`.

## Design docs

- [design/engineering-principles.md](design/engineering-principles.md): **must follow.** Read it before writing
  code; where code and principles disagree, fix one of them.
- [design/technical-design.md](design/technical-design.md): architecture, data model, ranking, API.
- [design/requirements.md](design/requirements.md) and [design/offering.md](design/offering.md): what we build and why.
- [design/local-build.md](design/local-build.md): the static, single-user build that runs in the browser.
- [design/app-ux.md](design/app-ux.md): the web app's layout and interactions for phones and wide screens.
- [design/progress.md](design/progress.md): what's done and what's next. Keep it current.

Build, run and check commands are in [README.md](README.md).

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
