# Upheap

A ranked todo list for teams. Items go into a heap with a need, a rough size, impact and due date, and the server
ranks them, explains each position, and keeps the ranking current as time passes. One binary serves the API and the
web app, on SQLite, MySQL or Postgres.

## Components

- `crates/domain`: pure logic (types, item state machine, ranking, due labels, permissions). No IO.
- `crates/store`: SeaORM entities, migrations, repositories.
- `crates/service`: use cases. Each one runs in one transaction with its audit entry.
- `crates/api-types`: the API's request and response types (serde + utoipa) and their conversions from domain types,
  shared by `api` and `cli`. No IO; depends only on `domain`.
- `crates/api`: axum handlers, error mapping, OpenAPI (utoipa), conversions from service views to `api-types`.
- `crates/cli`: the `upheap` command-line client (lib + thin bin), over HTTP with `api-types`. Never depends on `api`,
  `service` or `store` outside its tests.
- `crates/mail`: sending plain-text mail (SMTP, or files and the log for development). No IO beyond that; depends on
  nothing in the workspace.
- `crates/server`: the `upheap-server` binary (config, startup, `openapi` subcommand).
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
2. **Plans, status and ideas live in Upheap; `design/work/` holds designs.** A piece of work to do is a task in
   Upheap, in the heap it fits, with when it's done and what it waits for; an idea, an open question or a thought
   for later is a note there. Upheap ranks them and holds their state. Nothing in the repository lists them, and
   nothing lists what's built: the code and product.md say that, and the pull requests are the history.
3. **A design for unbuilt work is temporary.** `design/work/<name>.md` holds the design of a feature or change
   while it's built: what it is, why, and how it works. Nothing outside `work/` links into it but CLAUDE.md and the
   index: code and the permanent docs cite the permanent doc that holds a mechanism, or nothing until one does.
   Once built, the mechanisms worth keeping move into architecture.md or the module docs, what passes rule 4 into
   product.md and decisions.md, and the file goes. Git remembers it.
4. **product.md and decisions.md hold the core only.** product.md takes a promise to a user, stated as an outcome,
   that only a product change could break: not how a screen behaves, an edge case or an error state. decisions.md
   takes a choice that had a real alternative someone would propose again, is costly to reverse, and has a reason
   the code can't show: not a name, a label, wording, or anything one pull request can undo. The rest belongs in
   a design in `design/work/`, ux.md, architecture.md, a code comment or the pull request's description. When in doubt, leave
   it out, and name every entry added to either doc in the pull request's description.
5. **The repository reads without Upheap.** Not every reader can open the heap, so nothing committed depends on
   it. No doc, code comment, test or commit message names an Upheap item by key, title or link, names the heap it
   is in, or says that something is a task, a note or an idea in Upheap. A design states its stages, what it leaves
   out and what comes later in its own words, and stops there. The pointer goes one way: an item's context and
   comments name the doc and the pull request. Only this file and the index say where plans live, once each. A
   pull request's description may name the items it serves. Upheap as the product a doc describes is something
   else: "the person replies in Upheap" is the feature, "an idea in Upheap" is our planning.

When asked to write a "doc", write it as a Markdown file in `design/`, or in `design/work/` for unbuilt work, and
add it to `design/README.md`, not as an external document (Claude Docs,
Notion, Google Docs).

When you work on an item in Upheap, record progress, findings, decisions and where the work is as comments on the
item (`upheap comment add`), not by rewriting its context. Rewrite an item's context only when the task's
definition did change: what the work is, or what done looks like. Then say in a comment what changed and why.

Never add a plan, progress, what's next, an idea, an open question or a follow-up to a file in `design/work/`. Add it to
Upheap with `/upheap:add`, which picks the heap it fits, when this workspace can use Upheap: the `upheap` command is
installed and signed in. Leave no pointer to it in the file (rule 5). When it can't, name the follow-up in your reply
and in the pull request's description, so the owner can add it.

## Interface changes

Before building a significant change to the web app's design, on wide screens or on a phone, make a mockup in
Claude Docs and wait for the owner to approve it. Significant means a new kind of page, new data on a layout used
many times a session (the list row, the item, Waiting for me), or a change to how a core workflow goes on screen.
Small fixes, copy and changes that follow an approved design or an existing pattern don't need one. Once the
mockup is approved, the design doc or ux.md records what was agreed, and the mockup isn't linked from the repository.

## Git

Pushing to `main` is prohibited. Always work on a branch and open a pull request.
