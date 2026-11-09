# Upheap

A ranked todo list for teams. Items go into a heap with a need, a rough size, impact and due date, and the server
ranks them, explains each position, and keeps the ranking current as time passes. One binary serves the API and the
web app, on SQLite, MySQL or Postgres. A static build runs the same backend in the browser for one person.

## Components

- `crates/domain`: pure logic (types, item state machine, ranking, due labels, permissions). No IO.
- `crates/store`: SeaORM entities, migrations, repositories.
- `crates/service`: use cases. Each one runs in one transaction with its audit entry.
- `crates/api`: axum handlers, DTOs, error mapping, OpenAPI (utoipa).
- `crates/server`: the `upheap-server` binary (config, startup, `openapi` subcommand).
- `crates/local`: the backend compiled to WebAssembly, for the static single-user build.
- `web/`: React + TanStack Query client, with types generated from `openapi.json`.

Build, run and check commands are in [README.md](README.md).

## Docs

Read [design/engineering-principles.md](design/engineering-principles.md) before writing code. **Must follow.**
Where code and principles disagree, fix one of them. [design/technical-design.md](design/technical-design.md) is
the architecture. [design/README.md](design/README.md) indexes every doc and says what each is for.

Rules for the docs:

1. **One home per fact.** What code or a generated file states, no doc repeats: endpoints are in `openapi.json`,
   error codes in `crates/api/src/error.rs`, the schema in the migrations, configuration in README.md. Docs explain
   mechanisms and reasons, and link to the rest.
2. **Status lives in [design/progress.md](design/progress.md) only,** as state, not history: one line per area, no
   changelogs, no test counts. Update the row for the area you changed. The pull requests are the history.
3. **A design doc for unbuilt work is temporary.** It lives in `design/` while the work is built. Once built, the
   reasons worth keeping move into the architecture or the module docs, its status into progress.md, and the file
   goes. Git remembers it.
4. **Open questions** about built things go in progress.md's one list. A design doc for unbuilt work keeps its own
   until it's built.

When asked to write a "doc", write it as a Markdown file in `design/` and add it to `design/README.md`, not as an
external document (Claude Docs, Notion, Google Docs).

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
