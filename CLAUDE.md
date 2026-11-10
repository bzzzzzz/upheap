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
it's built and why, and [design/decisions.md](design/decisions.md) records the few choices that are costly to
reverse. [design/README.md](design/README.md) indexes every doc and says what each is for.

Rules for the docs:

1. **One home per fact.** What code or a generated file states, no doc repeats: endpoints are in `openapi.json`,
   error codes in `crates/api/src/error.rs`, the schema in the migrations, configuration in README.md. Docs explain
   mechanisms and reasons, and link to the rest.
2. **Status lives in `design/work/` only,** as state, not history.
   [design/work/README.md](design/work/README.md) is the tracker: the tracks in flight with a line each, what's
   next, and ideas, in about sixty lines at most. Each track's progress is in its own doc. Nothing lists what's
   built: the code and product.md say that, and the pull requests are the history.
3. **A track's doc is temporary.** `design/work/<track>.md` holds one piece of work while it's built: what it is,
   its progress, its design, its open questions and its ideas. Nothing outside `work/` links into it but CLAUDE.md
   and the index: code and the permanent docs cite the permanent doc that holds a mechanism, or nothing until one
   does. Once built, the mechanisms worth keeping move into architecture.md or the module docs, what passes rule 4
   into product.md and decisions.md, the ideas left over into the tracker, and the file goes. Git remembers it.
4. **product.md and decisions.md hold the core only.** product.md takes a promise to a user, stated as an outcome,
   that only a product change could break: not how a screen behaves, an edge case or an error state. decisions.md
   takes a choice that had a real alternative someone would propose again, is costly to reverse, and has a reason
   the code can't show: not a name, a label, wording, or anything one pull request can undo. The rest belongs in
   the track's doc, ux.md, architecture.md, a code comment or the pull request's description. When in doubt, leave
   it out, and name every entry added to either doc in the pull request's description.

When asked to write a "doc", write it as a Markdown file in `design/`, or in `design/work/` for unbuilt work, and
add it to `design/README.md`, and to the tracker when it's a track, not as an external document (Claude Docs,
Notion, Google Docs).

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
