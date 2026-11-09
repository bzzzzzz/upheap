# Upheap

A ranked todo list for teams. Items go into a heap with a need, a rough size, impact and due date, and the server
ranks them, explains each position, and keeps the ranking current as time passes. One binary serves the API and the
web app, on SQLite, MySQL or Postgres. A static build runs the same backend in the browser for one person.

## Components

- `crates/domain`: pure logic (types, item state machine, ranking, due labels, permissions). No IO.
- `crates/store`: SeaORM entities, migrations, repositories.
- `crates/service`: use cases. Each one runs in one transaction with its audit entry.
- `crates/api-types`: the API's request and response types (serde + utoipa) and their conversions from domain types,
  shared by `api` and `cli`. No IO; depends only on `domain`.
- `crates/api`: axum handlers, error mapping, OpenAPI (utoipa), conversions from service views to `api-types`.
- `crates/cli`: the `upheap` command-line client (lib + thin bin), over HTTP with `api-types`. Never depends on `api`,
  `service` or `store` outside its tests.
- `crates/server`: the `upheap-server` binary (config, startup, `openapi` subcommand).
- `crates/local`: the backend compiled to WebAssembly, for the static single-user build.
- `web/`: React + TanStack Query client, with types generated from `openapi.json`.

Build, run and check commands are in [README.md](README.md).

## Docs

Read [design/engineering-principles.md](design/engineering-principles.md) before writing code. **Must follow.**
Where code and principles disagree, fix one of them. [design/architecture.md](design/architecture.md) explains how
it's built and why, and [design/decisions.md](design/decisions.md) records the choices that could have gone another
way. [design/README.md](design/README.md) indexes every doc and says what each is for.

Rules for the docs:

1. **One home per fact.** What code or a generated file states, no doc repeats: endpoints are in `openapi.json`,
   error codes in `crates/api/src/error.rs`, the schema in the migrations, configuration in README.md. Docs explain
   mechanisms and reasons, and link to the rest.
2. **Status lives in [design/work/progress.md](design/work/progress.md) only,** as state, not history: one line
   per area, no changelogs, no test counts. Update the row for the area you changed. The pull requests are the
   history.
3. **A design doc for unbuilt work is temporary.** It lives in `design/work/` while the work is built. Once built, the
   mechanisms worth keeping move into architecture.md or the module docs, the choices into decisions.md, its status
   into progress.md, and the file goes. Git remembers it.
4. **Open questions** about built things go in progress.md's one list. A design doc for unbuilt work keeps its own
   until it's built.

When asked to write a "doc", write it as a Markdown file in `design/`, or in `design/work/` for unbuilt work, and
add it to `design/README.md`, not as an external document (Claude Docs, Notion, Google Docs).

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
