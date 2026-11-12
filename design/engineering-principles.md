# Engineering principles

How we write code here. **Must follow.** Where code and a principle disagree, fix one of them; don't leave them
disagreeing. The shared rules are taken from the maintainer's engineering knowledge base (`engineering/` in the
dotfiles, commit `edb2c95`); every section of it applies here, so none is left out. This project's own rules, and
the shared rules it overrides, are in the second part. [architecture.md](architecture.md) says how the code is
built, and [CLAUDE.md](../CLAUDE.md#docs) holds the rules for the docs.

## Shared rules

### General

1. **Simple first.** Write the most direct code that meets the requirement. Add an abstraction when there is a
   second real use for it, not a guessed one.
2. **One source of truth.** Each fact lives in one place: a rule in the core, a type in the API contract, the schema
   in the migrations, a command in the README. Everything else is derived or generated from it, and generated files
   are checked in and checked for drift.
3. **Make invalid states unrepresentable.** Use types to rule out bad data rather than checking for it later. An
   order that shipped has a `shipped_at`; an order that hasn't can't have one.
4. **Fail loudly and early.** Validate at the boundary and reject bad input with a clear, stable error. Never
   silently coerce or guess.
5. **Small, reviewable changes.** One concern per change. A refactor goes in a separate change from the behaviour
   it makes room for.
6. **Boring technology.** Prefer well-known, maintained libraries (`stack.md` in the base). Every dependency justifies
   its weight and is pinned through the lockfile.
7. **Document the why.** Code says what it does. Comments and design docs say why, especially where the obvious
   approach was rejected ([CLAUDE.md](../CLAUDE.md#docs)).
8. **Delete dead code.** Version control remembers it. Don't comment it out, and don't leave a flag on that nobody
   turns off.

### Boundaries

1. **Dependencies point inward.** The core holds the rules and depends on nothing else in the project; storage,
   HTTP and the UI depend on it and never on each other. Module or crate boundaries enforce this, not convention.
2. **The core is pure.** No IO, no clock, no randomness in the rules. Time is passed in, so every computation is
   reproducible and every test can pin it.
3. **Functions don't reach for what they need.** Only functions whose job is IO read configuration, the
   environment, files, the clock or the database. Everything else receives values, or a repository or client, from
   its caller.
4. **Gather the environment early.** The entry point reads configuration once, builds the connections and the
   clock, and passes them down. Operations never decide where they connect.
5. **Be explicit about needs.** A required input that's missing is an error. Don't guess a default or carry on with
   partial input.
6. **Nothing to do is success.** Repeating a write that is already applied succeeds and says so. Fail only when the
   work is needed and can't be done.
7. **Inputs and outputs stay separate.** Request types, core types, storage entities and response types are
   different types even when their fields match today, because they change for different reasons. Convert at the
   layer boundaries.
8. **Behaviour lives with its data,** at that data's layer. A core type gets methods for the rules, never for IO or
   HTTP concerns.
9. **Functions do what they say and nothing else.** A ranking function doesn't write, a getter doesn't mutate, a
   validator doesn't log an audit entry. A side effect shows in the name or the signature.

### Data

1. **The schema lives in the migrations.** Migrations only go forward, are never edited once released, and each one
   is safe on a populated database.
2. **Store UTC everywhere.** Timezones apply only at the edges, where the client says which one it is in.
3. **Optimistic concurrency on every mutable row.** A stale version gets a conflict error, never a silent overwrite.
4. **Prefer archiving to deleting** for anything a person made. An archive column or a state transition keeps the
   history and the references intact. `DELETE` is for plumbing that isn't domain data, such as ended sessions and
   expired tokens, and even then with the reason written down.
5. **A write and its record commit together.** If the product keeps an audit log, the change and its entry are one
   transaction; if the entry fails, the change fails.
6. **Portable SQL when the product is self-hosted.** Nothing that works on one database and not on the others the
   product supports, and CI runs on all of them. A product that runs on one database it controls can use that
   database's features; its architecture.md says so.
7. **Write down the size assumption** behind any whole-collection read or in-memory computation, and what changes
   when it no longer holds.
8. **Every file written for a program to read carries its format version:** an export or a backup, an import, a
   configuration file, a stored document or blob, a file another tool consumes. A top-level `version` field from the
   first release, so a breaking change to the schema can tell old files from new and convert or refuse them, with
   a message naming the version it found. An additive change keeps the version; a breaking one bumps it, and the
   reader keeps reading the old ones or says plainly that it can't. An API has the same rule through its versioned
   path ([The contract](#the-contract)).

### Errors

1. **Typed errors.** Each layer has an error type whose variants say what happened and carry the safe context, not a
   string. A catch-all error type is for the entry point and tests.
2. **Keep the source.** Wrap an error only to add context (which record, which step), and keep the original as its
   source. Never wrap just to change the type. A message describes only its own layer.
3. **Classify at the edge.** Operations return errors as they are. One place at the boundary maps them to what the
   caller sees, exhaustively, so an unmapped variant doesn't compile or fails a test. The categories: the caller's
   fault, not allowed, a conflict with a rule or a stale version, a dependency unavailable, and unexpected.
4. **Unexpected means a bug.** The caller gets a request ID and nothing else; the details go to the log.
5. **A warning is not an error.** Something non-fatal to report goes in the result. Only errors fail a request.
6. **Log a failure once,** where it is detected, at a level matching its severity. Callers that propagate it don't
   log it again.

### Security

1. **Deny by default.** Every operation checks a permission before doing anything. An operation without a check
   fails the access-control test.
2. **Don't leak existence.** Someone who can't see a thing gets "not found", not "forbidden".
3. **Scope every query** to what the caller may see. An ID from the request is never trusted on its own.
4. **Never log secrets:** no passwords, tokens, cookies or client secrets in logs, errors or audit diffs.
5. **Vetted cryptography only:** argon2id for passwords, PKCE for authorization codes, random tokens stored as
   hashes, except a secret that must be shown again, as an invite link's
   ([architecture.md](architecture.md#data-model)). Never roll our own.
6. **Insecure defaults are bugs.** Secure cookies, CSRF protection and strict CORS are on unless explicitly turned
   off, since someone else will run this.
7. **Text from other people is shown, never run.** Names and titles come from other users: escape them for the
   medium they go to, whether HTML, a terminal or a shell.
8. **Limit sign-in attempts** per account, so a password can't be guessed, and tell a locked-out user how long.

### Testing

1. **Test behaviour, not implementation.** Tests go through public functions and endpoints.
2. **Every bug gets a regression test** before the fix.
3. **Real databases, no mocks.** Integration tests run against the databases the product supports: a fast local one
   on every run, the rest in CI.
4. **Fast and hermetic.** Tests take an injected clock, and an injected random source when they check random
   values, use no network, and don't depend on order. Each test makes its own data, so tests run in parallel.
5. **Property tests for invariants:** determinism, stability under edits that shouldn't reorder, no cycles.
6. **A table test for access control:** every endpoint as each role and against each read-only state, with the
   expected statuses from the permission table, so an endpoint added without a row fails.
7. **The contract is tested:** the API document and the generated client types are checked in, and CI fails when
   either changes without being regenerated.
8. **End to end for the golden paths only,** against the real binary, at every width the product supports.
   Everything else is tested lower down.
9. **Testable by design.** Operations take plain values, repositories, a clock and a random source from their
   caller, so tests use plain values and fakes instead of patching globals.

### Operations

1. **One binary, one configuration.** Configure through environment variables, with every option documented and
   validated at startup.
2. **Fail at startup, not at runtime.** Bad configuration, an unreachable database or a failed migration stops the
   process with a clear message.
3. **Observable by default:** health and readiness endpoints, structured logs, and a request ID on every response
   so a user can quote it.
4. **Structured logging only,** with the level set from the environment. Each request gets a span with its request
   ID, user and route. Logs are diagnostics, never a substitute for a response.
5. **Log inside the operation.** The function that does the work logs it; callers log only what the callee can't
   know. No generic started/finished wrappers.
6. **Log outcomes, not just events.** A substantial operation logs its start and its result, with what it produced,
   such as `rows_switched=12`. IDs are fields, not text, so one query finds every event for a record.
7. **Shut down gracefully.** On a stop signal, drain in-flight requests.
8. **Reproducible releases.** Builds come from a tagged commit with a locked dependency tree. Release notes call out
   migrations and anything an operator must do.

### Dependencies and tooling

1. **The formatter and the linter are law.** CI runs both with warnings as errors. A lint is allowed only locally,
   with a comment saying why.
2. **Versions are declared once,** at the workspace root, and the lockfile is committed.
3. **Optional parts go behind features or build flags,** not runtime switches nobody turns off.
4. **Generated files are checked in** and regenerated in CI, which fails on a difference.
5. **A new dependency is a reviewed change.** The pull request says what it's for and why the standard library or
   an existing dependency doesn't do.

### Changes

1. **Every pull request passes every check** the repository defines: formatter, linters, type checks, test suites,
   drift checks.
2. **The docs change with the code.** A pull request that makes a doc wrong fixes the doc in the same change, by the
   rules in [CLAUDE.md](../CLAUDE.md#docs).
3. **A pull request description says what changed and why,** how it works where that isn't obvious, what's left for
   later, and how it was checked.
4. **Read before you move.** Read the module you're changing end to end, and understand its layer, before moving or
   renaming anything.

### Rust

Two terms: **flows** are the functions that decide what happens and own what the caller sees, such as HTTP handlers
and use cases. **Operations** are functions that do one job and return a result.

#### Tooling

1. **Clippy and rustfmt are law.** CI runs `cargo clippy --workspace --all-targets --locked -- -D warnings` and
   `cargo fmt --all --check`. rustfmt uses edition 2024 and a maximum width of 120. Allow a lint only locally, with
   a comment explaining why.
2. **Workspace dependencies.** Versions are declared once, in the root `Cargo.toml`; member crates use
   `{ workspace = true }`. `Cargo.lock` is committed and CI builds with `--locked`.
3. **Features for optional parts:** a database driver, profiling, a build that leaves a subsystem out. Not runtime
   flags.
4. **No `unsafe`.** `#![forbid(unsafe_code)]` in every crate. A dependency that expands to `unsafe` in your code,
   as some proc macros do, is out.

#### Naming and docs

1. **Name types after what they hold.** No catch-alls: `Info`, `Data`, `Details`, `Context`. A type that is hard to
   name is doing too much. Explicit but not verbose: `OrderPatch`, not `Patch` or `UpdateOrderRequestBodyPatch`.
2. **Functions start with a verb** that shows whether a call is cheap or does real work: `load_policy`, not
   `policy`. Exceptions: simple getters keep the noun (`order.state()`); predicates read as questions
   (`is_closed`, `can_reopen`); constructors say what they build (`new`, `with_parts`); conversions follow `to_`,
   `as_` and `into_`.
3. **Doc comments say what the name can't:** units, when a field is `None`, what a return value means, when a
   function fails, and what it touches outside its arguments. Skip them when they would only repeat the name.
   Public items in the core and the use-case layer are documented.
4. **Module docs say what the module owns** and cite, by section name, the permanent design doc that holds the
   mechanism they implement. Never a doc in `design/work/`: it goes once the work is built.

#### Structure and types

1. **One concept, one type.** Don't keep two structs for the same thing.
2. **Inputs and outputs stay separate:** request types, core types, entities and response types, converted at the
   layer boundaries ([Boundaries](#boundaries)).
3. **Newtypes for IDs and units:** `OrderId`, `Timestamp`, not bare `Uuid` or `i64`, so arguments can't be swapped.
4. **Enums for closed sets** (states, permissions, audit actions, error codes) with exhaustive `match`, so a new
   variant breaks the build where it matters. No `_ =>` arm over a set you own.
5. **Dependencies point inward,** enforced by crate boundaries: the core crate depends on nothing in the workspace,
   and the storage and HTTP crates never depend on each other.
6. **Shared API types in their own crate** when more than one binary speaks the API, with no IO and no dependency
   beyond the core, so a client never links the server.

#### Inputs

1. **Functions don't reach for what they need.** Only IO functions read config, environment variables, files, the
   clock or the database. Everything else receives values, or a repository or client, from its caller.
2. **Gather the environment early.** The binary reads config once, builds the pool, the clients and the clock, and
   passes them down through state. A library crate with a thin `main` takes a `Deps` struct, so tests pass fakes.
3. **Be explicit about needs.** A missing required input is an error, never a guessed default.
4. **Nothing to do is success.** A write that is already applied returns `Ok` and says so.

#### Function bodies

1. **Read top to bottom.** Branches return early so the main path stays unindented.
2. **Don't repeat work across arms.** When arms differ only in a value, compute the value in the `match` and do the
   work once.
3. **At most one level of shallow nesting.** Anything deeper or longer is a private function waiting to be named.
4. **Concurrent work in three phases:** dispatch every job, collect the results, interpret them together.
5. These are defaults; bend them when the linear shape genuinely doesn't fit.

#### Errors

1. **Typed errors with `thiserror`,** one enum per crate or layer, whose variants say what happened and carry the
   safe context: `OrderError::ReopenWindowExpired { order_id, closed_at }`, not a string. `anyhow` only in `main`
   and tests.
2. **No `unwrap` or `expect` in non-test code,** except on a proven invariant, each with a comment. Propagate with
   `?`.
3. **Keep the source.** Wrap only to add context, keep the original as `#[source]`, and let `Display` describe
   only its own layer.
4. **Classify at the edge,** in one exhaustive `match` from error variant to status and stable code, so an
   unmapped variant doesn't compile ([Errors](#errors)).
5. **A warning is not an error.** Return it in the result.

#### Async

1. **tokio is the runtime. Keep async at the edges:** the core is synchronous. Code that must also run on another
   executor, such as a WebAssembly build, uses only what every executor has, such as task-locals, and the rest stays
   behind features.
2. **Never block inside async code;** CPU-heavy or blocking work goes to `spawn_blocking`.
3. **Borrow, don't clone, in hot paths.** Elsewhere, clarity beats micro-optimisation.

#### Logging and tracing

1. **Structured `tracing` only,** levels from `RUST_LOG`. Each request gets a span carrying its request ID, user
   and route.
2. **Log inside the operation,** outcomes as well as events, at `INFO` for substantial work with what it produced
   (`rows_switched=12`), and once per failure ([Operations](#operations)).
3. **IDs are fields, not text:** `order_id = %id`, never formatted into the message, so one query finds every event
   for a record. Keep values lazy: fields or format arguments, never a string built before the macro.
4. **Never log secrets.** No default `#[instrument]` argument recording: `skip_all` and record chosen fields.
5. **Short, stable span names** (`rank_items`, `load_orders`). Debug-only spans are `DEBUG`.

#### Layout

1. **Source is a tree, not a flat list.** Group modules by feature or layer, and split a directory when it gets
   crowded: `api/src/orders/` with its handlers, `api-types/src/orders.rs` with its types.
2. **Code goes in the lowest crate that can own it.** A rule that needs no IO belongs in the core, not in a handler.
3. **A binary is a library with a thin `main`:** `main` gathers the environment and calls `run`, which returns the
   exit code, so the whole program is testable.
4. **Read before you move.** Read the module end to end and understand its layer before moving or renaming types.

#### Tests

1. **Unit tests in `#[cfg(test)] mod tests`** in the same file; integration tests in the crate's `tests/`; shared
   helpers and fakes in a test-only module.
2. **Async tests use `#[tokio::test]` and return `Result<()>`.**
3. **Arrange, Act, Assert,** in that order, visibly.
4. **Assertions use `googletest` matchers,** so a failure shows both sides.
5. **Tracing in tests is initialised once per test binary,** through a `std::sync::Once` in the shared harness, with
   the level from `RUST_LOG`. Not `ctor`: it expands to `unsafe`, which `forbid(unsafe_code)` rejects.
6. **Property tests with `proptest`** for invariants; **table tests** for access control and anything with a matrix
   of cases.
7. **Integration tests run the real stack:** the real router on in-memory SQLite, served on a local port when the
   thing under test is a client, checking stdout, stderr and the exit code.

### TypeScript and React

#### Language

1. **Strict mode, no `any`.** `strict: true` and `noUncheckedIndexedAccess`. `unknown` plus narrowing at the
   boundaries.
2. **Never hand-write API types.** Generate them from the API document, check them in, and regenerate in CI; a diff
   fails the build. Code imports the generated types, and nothing else describes the API.
3. **ESLint and Prettier in CI,** with no warnings. The type check runs for every build mode the app has.
4. **One source for each shared value:** a label, a breakpoint, a colour token or a storage key is a named export
   in one module, imported where used.

#### State

1. **Server state lives in TanStack Query.** Don't copy it into component state or a global store. Local state is
   for UI only: what's open, what's typed, what's focused.
2. **The URL is the state** for anything a person would link to or reload into: the current record, the view, the
   filters, the selected tab. Each gets its own path or search parameter, so Back works and links can be shared.
3. **Optimistic updates only where the server agrees.** Roll back on an error, and always re-render from the
   server's response, since it may differ from what was sent.
4. **A failed refresh keeps the data on screen** and says it may be stale; it never blanks a page that was
   showing something.
5. **An unsaved draft survives a refetch, a remount and someone else's save.** Keep the draft apart from the
   server value and merge on purpose.

#### Components

1. **Keep components thin.** No business logic: a component that computes something about a record is doing the
   server's work ([Boundaries](#boundaries)). Clients render what the server returns and show
   or hide actions only from what the server says is allowed.
2. **One component per behaviour,** reused at every width. The phone and the wide screen differ in presentation,
   not in components ([One app at every width](#one-app-at-every-width)).
3. **Behaviour from React Aria Components, looks from Tailwind.** Don't reimplement a dialog, a menu or a list's
   keyboard handling.
4. **Design tokens in `@theme`,** never a raw colour in a class. Light and dark come from the tokens, not from
   `dark:` pairs.
5. **Accessibility is part of done:** semantic HTML, full keyboard support, visible focus, labelled controls,
   `prefers-reduced-motion` honoured for every animation.
6. **A comment that cites a design doc cites a permanent one, by section name,** so the citation survives a doc
   restructure. Never a doc in `design/work/`: it goes once the work is built.
7. **Focus moves after a render, never on a timer.** After a dialog, sheet, menu or form closes, move focus with
   `useFocusAfterRender` (or an effect keyed on the state that brings the target), not with `setTimeout` or a guessed
   number of animation frames: those race React Aria's own focus restore, and lose on a slow machine.

#### Tests

1. **End to end with Playwright for the golden paths,** at every width the app supports and in every build mode,
   against the real server. Each test makes its own account and data, so tests run in parallel and share nothing.
2. **Everything else is tested below the UI,** on the server, where the logic is.
3. **The type check and the lint are tests.** They run in CI on every pull request.

### API

#### The contract

1. **The API is the product.** The web app is one client and the CLI another; nothing in the API is for one client
   only, except cookie authentication for the browser.
2. **The spec is checked in and reviewed.** Code-first OpenAPI generated from the handlers, committed as a file,
   with CI failing on drift, so a change to the generated file is a reviewed API change. Client types are generated
   from it and never hand-written.
3. **A versioned base path,** `/api/v1`, JSON over HTTPS.
4. **Breaking changes are additive or versioned.** Never rename or remove a field in place. A new enum value is
   additive, so clients tolerate values they don't know in sets they only read; a set the server reads stays strict.
5. **Request bodies refuse unknown fields,** so a misspelt field fails instead of saving nothing.

#### Errors

1. **One shape everywhere:** `application/problem+json` with the HTTP status, a stable machine-readable `code` and a
   human message. Clients branch on `code`, never on the message.
2. **Mapped in one place,** exhaustively, from the error variants, so an unmapped variant fails the build.
3. **The categories:** the caller's input (`400`, `422`); not allowed or not visible (`404`, see below, and `403`
   only where existence is already known); a rule or a stale version (`409`); a dependency down (`503`); a bug
   (`500`, with only the request ID).
4. **Standards keep their own format.** An OAuth token endpoint answers in the RFC's shape so every client library
   understands it; the server's own failures there are still problem responses.

#### Paths and handles

1. **Paths name things by readable handle** (a slug, a username, a short key), responses carry both the `id` and
   the handle, and request bodies refer to things by ID, since clients have the objects in hand.
2. **Every handle follows a fixed word** (`/teams/{team}`), so no name is reserved.
3. **A handle that's malformed or names something the caller can't see is `404`.** Don't leak existence.
4. **Creating takes an optional handle,** derived from the name when left out, with the race on uniqueness handled
   in the write.

#### Writes and concurrency

1. **Updates are `PATCH` with a partial body and the expected `version` in `If-Match`;** a stale version is `409`.
2. **Declarative endpoints take no `If-Match`:** archive, unarchive and membership say what something should end
   up as, so repeating or racing them is safe, and an already-applied write succeeds.
3. **Every write and its audit entry commit in one transaction,** when the product has an audit log.
4. **A write that invalidates other rows bumps their versions,** so an edit made from the old value fails with a
   conflict instead of undoing the change.
5. **A fixed lock order** where a write touches several aggregates, written down in architecture.md, and locking
   reads where the database's isolation level needs them.

#### Responses

1. **Ready to show.** Labels, flags and derived values are computed on the server in the request's context:
   `due: { at, firm, label, overdue }`, "4 days ago", a group code the client has headings for. Clients compute
   nothing but text search over what they hold.
2. **`allowed_actions` on every resource,** so clients never re-implement role or state rules to decide what to
   show.
3. **The whole aggregate,** so a client re-renders from one response after any change: a team with its members and
   their actions, a record with its children. `null` for unset values, never a missing key.
4. **Who did what** as a reference to the user plus a label, not a bare ID.

#### Lists

1. **Cursor pagination** for lists that grow without bound.
2. **The whole collection at once** where the size is bounded and ranking or grouping needs every row. Write the
   size assumption down, and the plan for when it no longer holds.
3. **Filters and sorts on the server,** because they decide what's pinned and grouped; text search may stay on the
   client, over what it holds.
4. **Hidden counts in the response,** so a client can say how many rows a filter left out.

#### Time

1. **Store and send UTC.** Every client sends its IANA timezone in a header, and the server computes every label and
   day boundary in it. A date without a time means the end of that day in the request's timezone.
2. **A stable order comes from a frozen `as_of`,** returned with the first page and echoed by the client, not from
   server state; the server ignores one older than about a day.

#### Authentication

1. **A session table with hashed random tokens** for the browser, in a secure cookie, with a custom header as the
   CSRF defence, since a cross-site form can't set one.
2. **Bearer tokens for every other client,** from the server's own OAuth 2.0 authorization-code flow with PKCE:
   public clients registered in code with loopback redirect URIs, short-lived access tokens, refresh tokens
   rotating in families where reuse revokes the family.
3. **A bearer token resolves to the same principal as a cookie,** and skips the CSRF header, which is a cookie
   problem.
4. **Sign-in attempts are limited** per account, and a locked-out user is told how long.
5. **External identity (OIDC)** maps onto the same session, with a username step on first sign-in when the product
   has usernames.

#### Health

1. **`/healthz` says the process is up; `/readyz` says its dependencies answer.**
2. **Every response carries a request ID** that the logs carry too.

### CLI

#### Goals

1. **The common thing is one short line:** `tool item add "Rotate the certs" --effort s --due next-week`.
2. **It answers the question the app answers,** with the same order and the same reasons, because it's a thin
   client of the same API ([API](#api)): it never computes what the server computes.
3. **Every command works in scripts:** stable exit codes, `--json` with the server's own response, and IDs on
   stdout with `--quiet`.
4. **No prompts, no TUI.** Every command takes all its input from arguments and flags, so it works the same in a
   script and in a terminal. An interactive mode, if ever, is a separate command.

#### Command shape

1. **`tool <category> <verb> [<required input>...] [--<option>...]`.** Every command is a category and a verb, and
   the same verb means the same thing in every category: `item list` and `tag list` both list. More typing than
   top-level verbs, but the tree stays the same everywhere; aliases can come once usage shows which are worth it.
2. **Required inputs are positional, options are flags.** Flags-only is for tools called by build systems.
3. **One verb per state change** (`start`, `done`, `reopen`), not `edit --state done`, so there is one way to do
   it and each reads as a sentence.
4. **Nothing given means nothing to do:** the command says so and exits 0. Repeating a command is safe, since the
   server treats an already-applied change as success.
5. **Clearing a field is a `--no-<field>` flag;** adding to and removing from a set are `--tag` and `--untag`.
6. **`--json` prints the server's body unchanged,** so scripts get the documented types and the tool adds no second
   schema. **`--quiet`** prints what a script needs next and nothing else.
7. **The thing a command works in** (a workspace, a project) comes from, in order: a flag, an environment variable,
   a default set by a `use` command. With none set, the command fails and says how to set one; it doesn't guess.

#### Naming things

1. **Stable handles, never cached positions.** A record is named by the key the server gives it, which names the
   same record for good, in any shell and on any machine; the tool keeps no cache of the last list. Lists print the
   key beside each row, and `--json` and `--quiet` print keys.
2. **A pasted link works.** A full path names a record elsewhere, and a link without its prefix is the same path.
3. **Names are resolved on the client only where it's cheap and unambiguous:** a tag by name, a choice by label,
   ignoring case. An unknown name fails with the closest existing names and the command that would create it; it
   doesn't create silently, since a typo in a terminal would make something nobody can delete.

#### Output

1. **Human output by default:** aligned columns, colour when stdout is a terminal and `NO_COLOR` isn't set.
2. **Every control character in text from the server is shown as U+FFFD** before it reaches a terminal, stdout or
   stderr. Names and titles come from other people, and an escape sequence would run in the reader's terminal or a
   newline forge a row. `--json` prints bytes unchanged when piped; on a terminal it escapes the controls JSON
   doesn't (`\u007f` to `\u009f`), the same value with nothing to interpret.
3. **Results go to stdout; warnings and errors go to stderr.** A reader that stops early (`| head -1`) isn't an
   error: exit 0, no message.
4. **One `Output` value, built in `main` from the flags and passed down.** Only the command's flow function writes;
   lookups and API calls return values and errors and never print.
5. **A footer says what's hidden** and which flag shows it, when a list leaves things out.

#### Errors and exit codes

1. **Print the message and the stable code** the server sent, add a hint when there is an obvious next step, and
   exit with the code for the category, so scripts branch on the exit code or, with `--json`, on the problem body
   printed to stderr.
2. **Exit codes are categories,** not `1` for everything:

   | Exit | Meaning | From |
   | :---: | --- | --- |
   | 0 | Done, or nothing to do | |
   | 1 | Unexpected: a bug in the tool, or a `500` with its request ID | |
   | 2 | The command line was wrong | argument parsing, and checks that run before any request |
   | 3 | The request was refused: bad input or a rule | `400`, `409`, `422`, and the tool's own lookups |
   | 4 | Not signed in, not allowed, or not found | `401`, `403`, `404` |
   | 5 | The server couldn't be reached or is unavailable | connection errors, timeouts, `503`, a proxy's `502` or `504` |
   | 6 | The local setup is broken: unreadable config, no keychain | |

   Permission and not found share a code because the server deliberately doesn't tell them apart.
3. **Don't retry a version conflict.** Someone else changed the record between the read and the write; the user
   should look before trying again, and the message says so.
4. **Validate what can be validated before any request,** and make that exit 2.

#### Configuration

1. **One file** in the platform's config directory, with few settings: the server, the default thing to work in,
   and its format version ([Data](#data)). Flags beat environment variables, which beat
   the file.
2. **Writes take a lock and are atomic:** read again under the lock, write a temporary file in the same directory,
   rename it over, so two instances changing different settings both keep theirs and nobody reads half a file.
3. **Unknown settings are refused, not ignored,** naming the setting and suggesting an upgrade if a newer version
   wrote it.
4. **Settings that belong together are written together:** a default record is one server's, so changing the
   server clears it and says so.
5. **Logging goes to a file in the same directory** when the log level is set, never to the terminal.

#### Signing in

1. **OAuth authorization code with PKCE through the browser,** on a loopback redirect, receiving an access token and
   a rotating refresh token. The refresh token goes in the OS keychain; without a keychain, sign-in fails (exit 6)
   rather than writing a token to a plain file.
2. **The access token is reused** while it has more than a minute left, so most commands make no token request.
3. **Refreshing takes a lock.** Tokens rotate and a spent one revokes the sign-in, so two instances refreshing at
   once would sign the user out: hold an exclusive lock, read the tokens again under it, and whoever waited finds
   the new pair. A refused refresh forgets the sign-in.
4. **A new sign-in replaces the old one by doing what can fail first and the destructive step last:** verify the
   new token, write the config, save the tokens under the lock, then revoke the old sign-in, best effort. Any
   failure before the save revokes the new tokens, which never reached the keychain, and leaves the old sign-in
   working.
5. **Sign-out revokes on the server first,** and keeps the local tokens if the server can't be reached, so a later
   try still ends it; a `--local` flag forgets them here only and says the sign-in may stay open on the server.
6. **Tokens go only where they were sent.** A plain `http://` server is refused unless it's on this machine;
   requests to this machine bypass the proxy variables; the HTTP client follows no redirects, so a form with a
   token or verifier is never re-sent to another host.
7. **The loopback listener** reads each connection in its own task with a short limit on the request line, since
   browsers open spare connections, and checks `state` itself: only the sign-in's own redirect ends the wait.
8. **An entry the tool can't read doesn't lock the user out:** sign-in overwrites it, `--local` sign-out forgets
   it, and other commands fail with a hint naming those two.

#### Code layout

```
src/
  main.rs          gather the environment (Deps::system), start the log, call run
  lib.rs           parse args, build Output, dispatch, report the error and its exit code
  error.rs         every failure, its message, hint and exit code
  config.rs        config file and environment, read once at startup
  client/          the HTTP client: base URL, bearer token, headers, problem+json errors
  auth/            the sign-in flow, keychain, token refresh
  lookup/          resolving names and labels to handles, IDs and values
  commands/
    <category>/    one file per verb or group of related verbs, each with its args struct and run
  output/          human and JSON rendering
```

1. **A library with a thin binary:** `main` gathers the environment and calls `run(args, deps)`, which returns the
   exit code. `Deps` holds the environment, the token store, the browser and the output streams, so tests pass an
   in-memory store, a browser that answers the consent page itself, and buffers.
2. **Shared flags are one struct flattened into every command;** flags that only some commands take are a second
   struct flattened only there, so a flag never shows where it would mean nothing.
3. **The API types are shared with the server,** from a crate with no IO, never hand-written ([API](#api)). A
   value a newer server adds must not break an older tool: enums the tool only reads tolerate unknown values.

#### Tests

1. **Lookups are plain functions over plain values,** unit tested.
2. **Integration tests run each command against the real API** on an in-memory database, served on a local port,
   and check stdout, stderr and the exit code. They sign in through the real endpoints, most by seeding the token
   store with tokens they obtained, and the sign-in tests by running the flow with a fake browser.
3. **The sign-in flow's client half** (the listener, the verifier) gets a unit test with a fake authorization
   server.

### User interface

#### Principles

1. **Consistent look.** One set of tokens for surfaces, ink, accents and state colours, each colour meaning one
   thing everywhere: one for "in progress" and the next action, one for done, one only for what's overdue. A
   control looks the same wherever it appears; a size picker in a dialog is the picker in the detail pane.
2. **Predictable layout.** Every screen keeps the same areas in the same places. Overlays come in two kinds only.
3. **At your fingertips.** Whatever is used several times a session is visible without opening anything: the main
   list, add, the common state changes, search. On a phone it sits within thumb reach at the bottom.
4. **One step ahead.** Whatever the user is likely to do next is one click away, and everything they can do with
   what's on screen is at most two obvious clicks away: open, then act. Keyboard shortcuts come on top of this, not
   instead of it.
5. **Only where it pays.** A suggestion, an extra button or an explanatory line costs attention on every visit.
   Before adding one, check three things: is the next step clear in most cases, rather than one of several guesses;
   does it save more than one click, or prevent a mistake or a loss; would the user miss it if it were gone? If
   not, leave it out. When it earns its place, show it only in the state where it applies, and prefer one quiet
   option to several.
6. **The server decides, the client shows.** The interface never ranks, checks permissions or works out labels; it
   renders what the server returns, shows an action only when the server says it's allowed, and adds layout,
   gestures and animation.

#### One app at every width

1. **One app, whose layout changes with the width:** phone under 640 px, tablet 640 to 1023 px, wide from 1024 px.
   Not a separate mobile app, and not a stretched phone or a squeezed desktop at the tablet width: each width gets
   its own layout from the same components.
2. **Navigation:** bottom tabs on a phone, with the primary action in the middle under the thumb and the overflow in
   a More sheet; a narrow rail on a tablet, icons with short labels; a sidebar on a wide screen. The same entries in
   the same order at every width, the account last.
3. **The main list** is full width on a phone with compact rows, beside the rail on a tablet with the wide screen's
   toolbar, and the middle column on a wide screen.
4. **The detail** opens as a bottom sheet over the list on a phone, a panel over the right of the list on a tablet
   with the list still live, and a pane beside the list on a wide screen. On a wide screen the content is one
   centred block that widens when the pane opens, so the menu stays next to what it opens.
5. **Overlays are two kinds:** a centred dialog or popover on a tablet or wide screen, a bottom sheet on a phone.
   Pickers that are lists are rows with a check on the current one.

#### Rules that settle questions

1. **Confirmations.** A reversible change acts at once and offers Undo in a toast: a state change, archiving
   something that can be unarchived. A change that can't be undone, or that affects other people, asks first, in a
   dialog or a sheet, and names the object: removing a member, giving up your own admin role, replacing all local
   data, discarding typed text.
2. **Editing lives in the detail.** Rows are for reading, ordering and changing state; everything else is edited in
   the record's own view, where there is room, and each change is saved as it's made.
3. **Every page that edits a list of things** uses one settings layout: a title, a one-line explanation, a way to
   add one near the top, rows that turn into a form when edited, and the rarer actions out of the way, in an
   overflow menu or at the foot of a card.
4. **Shared components differ only in presentation.** The phone sheet's buttons are the wide pane's buttons in a
   bar.
5. **After a session expires, signing in returns the user to the page they were on.**
6. **A new thing says where it landed.** Adding to a ranked or sorted list tells the user the position, from the
   server's own count.
7. **A forbidden control is absent, not disabled.** Read-only states, such as an archived record or a viewer's
   role, show the values and one line saying why nothing can be changed.

#### Touch and phones

1. **Targets are 44 px** where the pointer is coarse, and list rows in pickers 52 px. Inputs are at least 16 px so
   iOS doesn't zoom on focus.
2. **Respect the safe-area insets** under the tabs and sheets (`viewport-fit=cover`), and keep sheets above the
   on-screen keyboard: let the page resize for it where the browser does, and lift the sheet by the covered height
   from the visual viewport where it doesn't.
3. **Gestures where the pointer is coarse,** on phones and tablets alike: a row dragged one way completes, dragged
   the other way reveals the other state buttons; a sheet drags down to close, a panel drags sideways, and letting
   go short of the threshold springs back. A tap just after a drag doesn't open anything. Every gesture has a
   visible equivalent.
4. **A scroll is never a press.** Components that handle touch properly (React Aria) over hand-rolled handlers.

#### Theme

1. **Tokens in one place,** light and dark from the same names.
2. **System, Light or Dark,** chosen on the account page and kept per browser rather than on the account, since
   it's about the screen; applied by an inline script before the first paint, so a reload doesn't flash.

#### Keyboard

1. **Screens work without shortcuts first.** Shortcuts come after, shown as hints on the controls they trigger, a
   hint bar for what's on screen, and `?` for all of them.
2. **Arrow keys move, Enter opens, Escape closes,** in every list and overlay, from the component library rather
   than by hand.
3. **A command palette** (⌘K) once there is something to search across. It needs a server search endpoint, which is
   a design decision of its own.

#### Errors and conflicts

1. **A field's error sits under the field.** A page's error sits where the content would be, with the way back.
2. **A version conflict says someone else changed this,** shows the current value, and lets the user reapply; it
   never overwrites silently and never discards what was typed without asking.
3. **A failed refresh keeps the data shown** and says it may be stale. An offline state is one banner, not a
   different error on every page.
4. **An unexpected error shows the request ID** so it can be reported.

#### Before a screen ships

Walk the change along every axis and write down what happens on each. An axis that doesn't apply gets one line
saying so.

- Every width, and every build or deployment mode the app has.
- Every role and permission state, including read-only and archived.
- Data states: loading, empty, error, a failed refresh with data already shown, stale data, a version conflict.
- Input states: an unsaved draft when something else saves, refetches or remounts; where focus goes after save,
  cancel, remove and close; keyboard and touch targets.
- Persistence and compatibility: data stored before the change, old links and URLs, other tabs and sessions.
- Cost: requests or work added per user action.

## This project

### Layering

1. **`domain` is the core** of [Boundaries](#boundaries) §1–2: no IO, no clock, no randomness, and time comes in as
   `as_of`, so every ranking is reproducible. `service` holds the use cases, `store` the storage, `api` the HTTP
   layer, `api-types` the shared API types and `cli` a client; who may depend on whom is in
   [CLAUDE.md](../CLAUDE.md#components). `domain` depends on nothing in the workspace, and `store` and `api` never
   depend on each other.

### Thin clients

1. **Clients render what the server returns** ([API, Responses](#responses) §1–2; [User interface,
   Principles](#principles) §6): ranking, permissions, contributors, fuzzy due-date labels and validation live on
   the server. A client's own logic is text search, local UI state and animation, and it shows or hides actions
   only from `allowed_actions`. Anything else is a bug. One exception: the CLI's help names the rough due choices
   from `domain`, since help is read offline; the server still lists their dates and the CLI matches against that.
2. **Overrides [API, Responses](#responses) §1 for the local build only.** It counts and words facts about its own
   browser-held database and last download date in the client ([architecture.md](architecture.md#local-build),
   Keeping the data), since no server is involved.

### Ranking

1. **Deterministic.** The same inputs always give the same order, and every tie is settled by a fixed rule
   ([architecture.md](architecture.md#computation), Computation §4–5), never by iteration order or the clock.
2. **Every score has an explanation.** Any factor that changes position shows up in contributors. If we can't
   explain it, we don't ship it.
3. **The formula is versioned and isolated behind one function,** so it can change without touching storage or the
   API.
4. **Property tests cover the invariants** ([Testing](#testing) §5): tie stability, monotonic age, blockers
   inheriting the priority of what they block, and no cycles.

### Nothing is deleted

1. **Tightens [Data](#data) §4 from prefer to must.** Nothing a person made is deleted: archive columns and state
   transitions, never `DELETE`, and no `ON DELETE CASCADE`. The one exception in domain data is a team membership,
   whose removal deletes the row while the audit log keeps the history ([architecture.md](architecture.md#data-model)).
   Authentication plumbing that isn't domain data, ended sessions, sign-in attempt counts, and ended OAuth codes
   and token families with their tokens, may be deleted.

### Local build

1. **The same backend without accounts,** compiled to WebAssembly ([architecture.md](architecture.md#local-build)).
   It runs on `wasm-bindgen-futures` in one worker thread, so code shared with it uses only tokio's task-locals,
   which need no runtime, and the rest of tokio stays behind features the local build leaves out, such as
   `accounts` ([Rust, Async](#async) §1).
2. **No configuration** ([Operations](#operations) §1): the local build has nothing to configure.
