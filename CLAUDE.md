# Upheap

A ranked todo list for teams. Items go into a heap with a need, a rough size, impact and due date, and the server
ranks them, explains each position, and keeps the ranking current as time passes. One binary serves the API and the
web app, on SQLite or MySQL.

## Components

- `crates/domain`: pure logic (types, item state machine, ranking, due labels, permissions). No IO.
- `crates/store`: SeaORM entities, migrations, repositories.
- `crates/service`: use cases. Each one runs in one transaction with its audit entry.
- `crates/api-types`: the API's request and response types (serde + utoipa) and their conversions from domain types,
  shared by `api` and `cli`. No IO; depends only on `domain`.
- `crates/api`: axum handlers, error mapping, OpenAPI (utoipa), conversions from service views to `api-types`.
- `crates/server`: the `upheap-server` binary (config, startup, `openapi` subcommand).
- `web/`: React + TanStack Query client, with types generated from `openapi.json`.

## Design docs

- [design/engineering-principles.md](design/engineering-principles.md): **must follow.** Read it before writing
  code; where code and principles disagree, fix one of them.
- [design/technical-design.md](design/technical-design.md): architecture, data model, ranking, API.
- [design/requirements.md](design/requirements.md) and [design/offering.md](design/offering.md): what we build and why.
- [design/local-build.md](design/local-build.md): the static, single-user build that runs in the browser.
- [design/app-ux.md](design/app-ux.md): the web app's layout and interactions for phones and wide screens.
- [design/cli.md](design/cli.md): the `upheap` command-line client for everyday item and tag work.
- [design/cli-tracker.md](design/cli-tracker.md): the CLI's build, step by step, with what's done.
- [design/cloud-run-postgres.md](design/cloud-run-postgres.md): Postgres support and deploying on Cloud Run with Neon.
- [design/progress.md](design/progress.md): what's done and what's next. Keep it current.

When asked to write a "doc", write it as a Markdown file in `design/` and add it to this list, not as an
external document (Claude Docs, Notion, Google Docs).

Build, run and check commands are in [README.md](README.md).

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
