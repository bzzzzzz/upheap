# Engineering principles

Companion to [architecture.md](architecture.md). These are the rules we follow when writing the code. Where a principle and the architecture disagree, fix one of them; don't leave them disagreeing.

## General

1. **Simple first.** Write the most direct code that meets the requirement. Add abstraction only when there's a second real use for it, not a guessed one.
2. **One source of truth.** Each fact lives in one place: permissions in `domain`, API types in the OpenAPI spec, schema in migrations. Everything else is derived or generated from it.
3. **Make invalid states unrepresentable.** Use types to rule out bad data rather than checking for it later. For example, a closed item has a `closed_at`, and an open item can't have one.
4. **Fail loudly and early.** Validate at the boundary and reject bad input with a clear, stable error code. Never silently coerce or guess.
5. **Small, reviewable changes.** One concern per change. Refactors go in separate changes from behaviour changes.
6. **Boring technology.** Prefer well-known, maintained libraries. Every dependency has to justify its weight, and it is pinned through lockfiles.
7. **Document the why.** Code says what it does. Comments and design docs say why, especially when the obvious approach was rejected.
8. **Delete dead code.** Version control remembers it. Don't comment it out or leave flags on that nobody turns off.

## App-specific

### Thin clients

1. Clients render what the server returns. Ranking, permissions, contributors, fuzzy due-date labels and validation all live on the server.
2. Client-side logic is limited to text search, local UI state and animation. Anything else is a bug.
3. Clients never infer permissions. They show or hide actions only from `allowed_actions`.
4. The one exception: the local build counts and words facts about its own browser-held database and last download date in the client ([local-build.md](local-build.md), Keeping the data), since no server is involved.

### Domain and ranking

1. `domain` is pure: no IO, no clock, no randomness. Time is passed in as `as_of`, so every ranking is reproducible.
2. Ranking is deterministic. The same inputs always give the same order, with ties broken by `created_at` and then by ID.
3. Every score has an explanation. Any factor that changes position must show up in contributors. If we can't explain it, we don't ship it.
4. The formula is versioned and isolated behind one function, so it can change without touching storage or the API.

### Data

1. Nothing is deleted. Use archive columns and state transitions, never `DELETE`, and no `ON DELETE CASCADE`. The exceptions are a team membership, whose removal deletes the `team_members` row while the audit log keeps the history (`member_removed`, `member_left`), and authentication plumbing that isn't domain data: ended sessions and sign-in attempt counts.
2. Every change and its audit entry are written in one transaction. If the audit write fails, the change fails. Audit entries belong to a team, so a change to a user, such as their display name, is recorded in each of their teams; a user in no team has nowhere to record it, and the change goes unrecorded.
3. Use portable SQL only. Anything that works on one of SQLite, MySQL and Postgres but not the others doesn't go in. CI runs on all three.
4. Migrations only go forward and are never edited after release. Each one must be safe on a populated database.
5. Store UTC everywhere. Timezones only apply at the edges, through the `Time-Zone` header.
6. Use optimistic concurrency on every mutable row. A stale `version` gets `409`, never a silent overwrite.

### API

1. The API is the product. The web app is one client, so nothing in the API should be web-only, except cookie auth.
2. Write the spec first and check it in. Changing the generated OpenAPI file is a reviewed API change.
3. Breaking changes are additive or versioned. Never rename or remove a field in place.
4. Errors use one shape everywhere: HTTP status plus a stable machine-readable `code` plus a human message. Clients branch on `code`, never on the message.
5. Endpoints are idempotent where possible: `PUT`/`PATCH` with `If-Match`, and safe retries.

### Security

1. Deny by default. Every service operation checks a permission before doing anything. An operation without a check fails the access-control test.
2. Don't leak existence. Non-members get `404`, not `403`.
3. Every query is scoped to the caller's team or heap. IDs from the request are never trusted on their own.
4. Never log secrets: no passwords, tokens or session cookies in logs, errors or audit diffs.
5. Use vetted crypto only: argon2id, PKCE S256, and random tokens stored as hashes. Never roll our own.
6. Self-hosting means insecure defaults are bugs. Secure cookies, CSRF protection and strict CORS are on unless explicitly disabled.

## Rust

Two terms used below: **flows** are the axum handlers and the `service` use cases: they decide what happens and own what the caller sees. **Operations** are functions in `domain` and `store` that do one job and return a result.

### Tooling

1. **Clippy and rustfmt are law.** CI runs `cargo clippy --all-targets -- -D warnings` and `cargo fmt --check`. rustfmt uses edition 2024 and a maximum width of 120. Allow a lint only locally, with a comment explaining why.
2. **Workspace dependencies.** Versions are declared once, in the root `Cargo.toml`. Member crates use `{ workspace = true }`. `Cargo.lock` is committed.
3. **Features for optional parts.** Optional functionality, such as a MySQL driver or profiling, goes behind Cargo features.
4. **No `unsafe`.** Use `#![forbid(unsafe_code)]` in every crate.

### Naming and docs

1. **Name types after what they hold.** Don't use catch-all names like `Info`, `Data`, `Details` or `Context`. A type that is hard to name is usually doing too much. Names are explicit but not verbose: `ItemPatch`, not `Patch` or `UpdateItemRequestBodyPatch`.
2. **Functions start with a verb** that shows whether a call is cheap or does real work: `load_heap_policy`, not `heap_policy`. Exceptions:
   - Simple getters keep the noun (`item.state()`).
   - Predicates read as questions (`is_closed`, `can_reopen`).
   - Constructors say what they build (`new`, `with_parts`).
   - Conversions follow `to_` / `as_` / `into_`.
3. **Doc comments say what the name can't:** units, when a field is `None`, what a return value means, when a function fails, and what it touches outside its arguments. Skip them when they would only repeat the name. Public items in `domain` and `service` are documented.

### Structure and types

1. **One concept, one type.** Don't keep two structs for the same thing.
2. **Inputs and outputs stay separate.** Request DTOs, domain types, database entities and response DTOs are different types even when their fields match today, because they change for different reasons. Convert at the layer boundaries.
3. **Behaviour lives with its data,** at that data's layer. A domain type gets methods for domain rules, never for IO or HTTP concerns.
4. **Functions do what they say and nothing else.** `rank` doesn't write, a getter doesn't mutate, a validator doesn't log an audit entry. If a function has a side effect, its name or signature shows it.
5. **Newtypes for IDs and units.** Use `ItemId`, `HeapId` and `Timestamp`, not bare `Uuid` or `i64`, so arguments can't be swapped.
6. **Enums for closed sets** (states, permissions, audit actions, error codes) with exhaustive `match`, so a new variant breaks the build where it matters.
7. **Dependencies point inward.** `domain` depends on nothing in the workspace, and `store` and `api` never depend on each other. Crate boundaries enforce this.

### Inputs

1. **Functions don't reach for what they need.** Only functions whose job is IO read config, environment variables, files, the clock or the database. Everything else receives values, or a repository or client, from its caller.
2. **Gather the environment early.** `server` reads config once, builds the database pool, OIDC clients and clock, and passes them down through axum state. Operations never decide where they connect.
3. **Be explicit about needs.** If a required input is missing, return an error. Don't guess a default or carry on with partial input.
4. **Nothing to do is success.** Repeating a write that is already applied succeeds and says so. Fail only when the work is needed and can't be done.

### Function bodies

1. **Read top to bottom.** Branches return early so the main path stays unindented.
2. **Don't repeat work across arms.** Work that doesn't depend on the branch goes outside it: when arms differ only in a value, compute the value in the `match` and do the work once.
3. **At most one level of shallow nesting.** Anything deeper or longer is a private function waiting to be named.
4. **Concurrent work in three phases:** dispatch every job, collect the results, then interpret them together.
5. These are defaults; bend them when the linear shape genuinely doesn't fit.

### Errors

1. **Typed errors.** Each crate has `thiserror` enums whose variants say what happened and carry the safe context (`ItemError::ReopenWindowExpired { item_id, closed_at }`, not a string). `anyhow` is only for `server` startup and tests.
2. **No `unwrap` or `expect` in non-test code,** except for proven invariants, each with a comment. Propagate errors with `?`.
3. **Keep the source.** Wrap an error only to add context (which heap, which step), and keep the original as its `source`. Never wrap just to change the type. A `Display` message describes only its own layer.
4. **Classify at the edge.** Operations return errors as they are. Only `api` maps them to an HTTP status and stable `code`, in one exhaustive `match`, so an unmapped variant doesn't compile. Categories:
   - **caller:** bad input → `400`/`422`.
   - **permission:** → `403`/`404`.
   - **conflict:** a state rule or stale version → `409`.
   - **dependency:** database or identity provider unavailable → `503`.
   - **unexpected:** a bug or broken invariant → `500`. The response gives only the request ID; the details go to the log.
5. **A warning is not an error.** If an operation has something non-fatal to report, it returns it in its result. Only errors fail a request.

### Async

1. `tokio` is the runtime. Keep async at the edges: `domain` is synchronous. The local build runs on `wasm-bindgen-futures` in one worker thread instead, so code shared with it uses only tokio's task-locals, which need no runtime. The rest of tokio stays behind features the local build leaves out, such as `accounts` (see [local-build.md](local-build.md)).
2. Never block inside async code; move CPU-heavy or blocking work to `spawn_blocking`.
3. Borrow, don't clone, in hot paths such as ranking. Elsewhere, clarity beats micro-optimisation.

### Logging and tracing

1. **Structured `tracing` only.** Log levels are set with `RUST_LOG`. Each request gets a span carrying its request ID, user ID and route. Logs are diagnostics, never a substitute for API responses.
2. **Log inside the operation.** The function that does the work logs it; callers log only what the callee can't know, such as why it made the call. Don't wrap calls in generic "started/finished" helpers.
3. **Log outcomes, not just events.** Substantial operations (migrations, sign-in, archiving a parameter with replacements) log their start and their result at `INFO`, with what they produced, such as `items_switched=12`.
4. **Log a failure once,** where it is detected, at a level matching its severity. Callers that propagate it don't log it again.
5. **IDs are fields, not text:** `heap_id`, `item_id` and `user_id` are always `tracing` fields, so one query finds every event for a heap or item.
6. **Keep values lazy.** Pass them as fields or format arguments. Never build a string with `format!` before the macro.
7. **Never log secrets:** no passwords, session or OAuth tokens, cookies, or OIDC client secrets. Don't use default `#[instrument]` argument recording; skip all arguments and record only chosen fields.
8. **Short, stable span names** (`rank_heap`, `load_items`). Spans that are only for debugging use `DEBUG`.

### Layout

1. **Source is a tree, not a flat list.** Group modules by feature or layer, and split a directory when it gets crowded. In `api`, each resource has its own module with its handlers and DTOs (`api/src/items/`).
2. **Code goes in the lowest crate that can own it.** A rule that doesn't need IO belongs in `domain`, not in a handler.
3. **Read before you move.** Read the module you're changing end to end, and understand its layer, before moving or renaming types.

## TypeScript and React

1. **Strict mode, no `any`.** Use `strict: true` and `noUncheckedIndexedAccess`. Use `unknown` plus narrowing at boundaries.
2. **Never hand-write API types.** Generate them from the OpenAPI spec and regenerate them in CI. A diff fails the build.
3. **Server state lives in TanStack Query.** Don't copy it into component state or a global store. Use local state for UI only.
4. **Keep components thin.** No business logic. If a component computes something about an item, that belongs on the server.
5. **Make the URL the state.** The heap, view, filters and selected item live in the router, so links and reloads work.
6. **Accessibility is part of done.** Use semantic HTML, full keyboard support (the app is shortcut-driven), visible focus and labelled controls. Honour `prefers-reduced-motion` for FLIP animations.
7. **Optimistic updates only where the server agrees.** Roll back when the server returns an error, and always re-render from the server response.
8. **ESLint and Prettier in CI,** with no warnings.

## Testing

1. Test behaviour, not implementation. Tests go through public functions and endpoints.
2. Use property tests for ranking invariants: tie stability, monotonic age, blockers inheriting the priority of what they block, and no cycles.
3. Every bug gets a regression test before the fix.
4. Integration tests run against real SQLite, MySQL and Postgres, with no database mocks.
5. Tests are fast and hermetic. They take an injected clock, and an injected random source when they check random values, use no network, and don't depend on order.
6. Keep end-to-end tests to the golden paths. Everything else is tested lower down.
7. **Rust test layout:**
   - Unit tests go in `#[cfg(test)] mod tests` in the same file.
   - Integration tests go in each crate's `tests/`.
   - Shared helpers and fakes go in a test-only module.
8. **Rust test shape:**
   - Async tests use `#[tokio::test]` and return `Result<()>`.
   - Tests are written in Arrange / Act / Assert sections.
   - Assertions use `googletest` matchers.
   - Tracing for tests is initialised once per test binary, through a `std::sync::Once` in the shared harness, with the level set by `RUST_LOG`. (Not `ctor`: it expands to `unsafe` code, which `forbid(unsafe_code)` rejects.)
9. **Testable by design.** Operations take plain values, repositories, a clock and a random source from their caller, so tests can use plain values and fakes instead of patching globals.

## Operations

1. **One binary, one config.** Configure through environment variables and an optional file, with every option documented and validated at startup. The local build has no configuration.
2. **Fail at startup, not at runtime.** Bad config, an unreachable database or a failed migration stops the server with a clear message.
3. **Observable by default.** Provide `/healthz`, `/readyz`, structured JSON logs and request IDs returned to clients.
4. **Shut down gracefully.** On SIGTERM, drain in-flight requests.
5. **Reproducible releases.** Builds come from a tagged commit with a locked dependency tree. Release notes call out migrations.
