# Architecture

How Upheap is built, and why. Companion to [requirements.md](requirements.md) and [offering.md](offering.md). The
coding rules are in [engineering-principles.md](engineering-principles.md), and the choices that could have gone
another way, with their reasons, are in [decisions.md](decisions.md).

This doc explains mechanisms and reasons. What the code states, it doesn't repeat: the endpoints are in
`openapi.json` (`upheap-server openapi` prints it), the error codes in `crates/api/src/error.rs`, the schema in
`crates/store/src/migration`, the configuration variables and the crate layout in [README.md](../README.md).

## Principles

1. **Thin clients.** The server does all computation: ranking, contributors, filtering, sorting, fuzzy due labels,
   permission checks, and state rules such as whether an item can still be reopened. Clients render what the API
   returns and send intent back. They do two things themselves: text search over the list they already hold
   ([Search](#search)) and animation. A new client, such as the CLI, needs nothing beyond HTTP and the generated
   API types.
2. **The server says what's allowed.** Responses carry `allowed_actions`, so clients never re-implement role or
   state rules to decide which controls to show.
3. **Database-agnostic.** Portable SQL only: no JSON column types, no database-specific full-text search, no
   triggers or stored procedures. Everything runs on SQLite, MySQL and Postgres, and CI tests all three.
4. **Nothing is deleted, and everything is audited.** No `DELETE` of domain data. Every write and its audit entry
   commit in one transaction.
5. **Single binary.** One executable serves the API and the embedded web app. The [local build](local-build.md) is
   the one other deliverable: the same backend crates compiled to WebAssembly and shipped with the web app as a
   static site.

## Stack

| Area | Choice |
| --- | --- |
| Backend | Rust (stable), a Cargo workspace |
| HTTP | axum on tokio, tower middleware |
| API contract | Code-first OpenAPI 3.1 with utoipa. The web app's TypeScript types are generated from it with openapi-typescript, and CI fails on drift |
| Data access | SeaORM 2 over sqlx, with the backend chosen at startup from `DATABASE_URL`: SQLite, MySQL or Postgres. The local build uses SeaORM's proxy connection over SQLite-wasm |
| Migrations | sea-orm-migration, embedded in the binary and run at startup |
| Auth | argon2id for local passwords; an own session table with hashed random tokens for the web app; own OAuth 2.0 authorization-code endpoints with PKCE for non-web clients. External identity providers (OIDC) are planned, not built |
| Web app | React, Vite, TanStack Query and Router, Tailwind, React Aria Components, Motion; embedded in the binary with rust-embed |
| Observability | `tracing` with JSON logs, request IDs, `/healthz` and `/readyz` |
| Testing | `cargo test` with googletest matchers, proptest for ranking invariants, MySQL and Postgres service containers in CI, Playwright end to end |
| Packaging | A static binary in a minimal Alpine image; docker-compose with SQLite on a volume; Cloud Run with Neon Postgres in production ([cloud-run-postgres.md](cloud-run-postgres.md)); the local build on GitHub Pages |

## Workspace

Dependencies point one way: `server → api → service → store, domain`, and `local` sits beside `server` as the
other entry point. `domain` depends on nothing in the workspace, so ranking and the state rules are unit- and
property-tested in isolation, and `store` and `api` never depend on each other. The crates and their roles are
listed in [README.md](../README.md#layout); each crate's root module says what it owns.

`service` and `api` put what only a shared server needs behind their default `accounts` feature: registration,
sign-in and sessions, team management, user lookup, OAuth. The local build leaves it out, so that code and its
dependencies aren't in the wasm at all ([local-build.md](local-build.md#single-user-mode)). The features are
additive: with `accounts` on, the local user still works, which lets the native API tests cover it.

## Data model

The tables and columns are in the migrations. These are the conventions they follow.

- **IDs** are UUIDv7 in `CHAR(36)`: sortable by creation time and safe to expose.
- **Handles** name things in URLs, API paths and the CLI ([readable-ids.md](readable-ids.md)): usernames, team
  slugs, heap slugs (unique per team) and item keys (unique per heap). They are extra columns with unique indexes,
  and every foreign key still points at the UUID, so a handle can change without touching references.
- **Timestamps** are UTC Unix milliseconds in `BIGINT`, which sidesteps the differences between the databases'
  date types. No timezone is stored anywhere ([Time and timezones](#time-and-timezones)).
- **Structured data** (audit diffs, saved list views) is JSON in `TEXT`, never queried inside the database.
- **Archiving** is a nullable `archived_at`. There are no hard deletes, and foreign keys use `ON DELETE RESTRICT`,
  so even an accidental `DELETE` fails instead of removing related rows. Relationship rows follow the same rule:
  taking a tag off an item sets `item_tags.removed_at`, and clearing a parameter value sets its `option_id` to
  `NULL`. Team membership is the one exception: removing a member deletes the `team_members` row, and the audit log
  keeps the history.
- **Optimistic concurrency:** every mutable row carries a `version`. Updates require the expected version and fail
  with `409` otherwise. A parameter is one aggregate: its options and tag links have no version of their own, and
  every change to them takes the parameter's version.
- **State and `closed_at`:** in the database, `closed_at` and `closed_by` are set exactly when the state is done or
  won't do. In `domain`, the state is an enum where only the closed variants carry them, so an open item with a
  close time can't be built. The store converts between the two and rejects rows that break the rule.
- **Case-insensitive uniqueness** (tag names per heap) is a lowercased key column with a unique index rather than a
  collation, because MySQL compares case-insensitively by default and SQLite doesn't. Where an index can't express
  the rule, as for parameter names, which must be unique only among parameters in use, the service checks under the
  heap lock.
- **Insert-only tables:** the application can only insert into `audit_log`. There is no update or delete code path
  for it.
- **The heap's ranking policy** is its row: the need, age, deadline and value weights on `heaps`, plus each
  parameter's weight. Effort, impact and need are columns on `items` rather than custom parameters, because they are
  fixed and apply to every heap ([decisions.md](decisions.md#6-effort-impact-and-need-are-item-columns-not-custom-parameters)).
- **A user's username is the slug of their personal team**, so usernames and team slugs share one namespace under
  one unique index. The personal team can't take members, and its slug never changes.
- **Archived options always name their replacement** (`archived_at` and `replaced_by_option_id` are set together),
  so a stored value can always be followed to an option in use.

## Ranking

### Computation

Ranking is a pure function in `domain`: the heap's policy, its non-closed items with their parameter values and
blocking edges, and an `as_of` time go in; positions and contributors come out. Nothing about scores is stored.

1. The service loads everything the heap needs and ranks in memory on each request. Heaps are expected to hold at
   most a few thousand open items, so this is cheap. A per-heap cache invalidated on write can be added if
   measurements call for it.
2. Each factor (need, age, deadline proximity, value per effort, each custom parameter) produces a weighted
   component, and the score combines them. The formula is a named version (`FormulaVersion::V2`) behind one
   function, set in code rather than stored per heap: tests pin orderings per version, so a formula change is an
   explicit test update, and since scores are never stored, switching versions needs no migration. The module doc of
   `domain::ranking` explains the current formula.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are refused on write. An item's
   effective score is the greater of its own and the boosted effective scores of the items it blocks, computed by
   one memoized walk, so it's linear in the number of items and carries through chains. Ties go to the item blocking
   the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
4. **Contributors** are the largest weighted components, each with a label and a relative share ("Deadline in a few
   days", "Blocks *Migrate auth*"). Scores themselves are never returned.
5. **Ties** that the chain length doesn't settle go to the older item, then to the ID, so the order is fully
   deterministic.

### Session-stable order

The score is a deterministic function of item data and time, so a stable session needs no server-side state, only
a fixed time.

1. The first list request returns an `as_of` timestamp with the ranked list. The client sends the same `as_of` on
   later requests: refetches after an edit, filter changes.
2. Items whose data hasn't changed keep their positions, because the time input is frozen. An edited item is ranked
   with its new data, so it moves right away, and so do teammates' edits on the client's next refetch, which happens
   on focus and on a short interval (server push can replace polling later). The web app animates every move.
3. Reloading drops `as_of`, so the list re-ranks at the current time. The server ignores an `as_of` more than about
   a day old.
4. Every label in the list (due, created, closed) is worked out at `as_of` too, so the list reads as one moment.
   Responses for a single item use the current time.

### Search

1. The ranked list endpoint returns every open item of the heap in one response, with no pagination. At the
   expected size that's fine, and it's what keeps client-side search and stable positions simple.
2. The client searches text itself, over the titles and descriptions it already has, keeping the ranked order and
   making no request per keystroke.
3. Filters by state, tag, need, size and parameter option, and the sorts, stay on the server, because they change
   which items are pinned and how they are grouped. Ranking always runs over every non-closed item, since a blocker
   inherits from what it blocks even when that item is filtered out or hidden; the view then picks, groups and sorts
   the result, and reports what it left out (`hidden_count`, `unfiltered_count`, `in_progress_count`) so a client
   can say "6 of 14" or offer the next item without a second request. The view's defaults (in progress pinned,
   blocked hidden) are the server's, so every client starts from the same list. Archived parameters and options are
   accepted in filters and sorts, so a saved view keeps working after an option is archived.
4. The web app keeps the view in the URL, so it survives reloads and can be shared, but not the search text, which
   is a momentary narrowing. Saved views store the same settings under the query parameters' names, so a client
   turns one into a list query field by field.
5. If heaps grow beyond this size, search moves to the server with per-database full-text indexes behind a `q=`
   parameter.

### Time and timezones

1. All instants are stored and sent as UTC timestamps. The server stores no timezone, for users or anywhere else.
2. Clients send the device's IANA timezone on every request in a `Time-Zone` header. The server uses it for
   presentation values: fuzzy due labels, formatted dates, "overdue", the closed list's groups. A missing header
   means UTC.
3. Dates without a time, such as a due date, are stored as the end of that day in the request's timezone, converted
   to UTC.
4. The one-week reopen window is exactly 7 × 24 hours from `closed_at`, so it doesn't depend on any timezone.
5. The server lists the rough due choices a picker offers (`GET /due-choices`), each with its date in the request's
   timezone and the label it reads back with, so "Next week" can never come back as "in a few days".

### Cycle detection and locking

1. Every item, tag, parameter and saved-view write first locks the heap row (`SELECT … FOR UPDATE` on MySQL and
   Postgres), before touching any other row, so writes in one heap run one after the other and always take locks in
   the same order. The write checks that the heap isn't archived on the row read under that lock, and archiving
   takes the same lock, so no write lands in a heap archived while it waited. Reads that a rule depends on (the
   blocker chain, a parameter and its options, a saved view) are locking reads too, because on MySQL a plain read
   before the lock would fix the transaction's snapshot and hide changes that committed while it waited. SQLite has
   one writer at a time, so there the second of two conflicting writes fails instead of waiting; the outcome that
   matters, no inconsistent state, holds either way.
2. Setting a blocker walks the chain inside that transaction and refuses a request that would reach the item
   itself. A blocker is part of the blocked state: leaving the state clears it, and it must be an open item of the
   same heap. Closing a blocker later doesn't unblock what waits for it; the blocked item shows the closed blocker
   until someone moves it on.
3. Ranking still tolerates a cycle in stored data, by ignoring the edge that closes it, so one bad row can't break
   the list.

## API

1. Everything is under `/api/v1`, JSON over HTTPS. The spec is `openapi.json`.
2. Errors are `application/problem+json` with a stable `code`, mapped in one place (`crates/api/src/error.rs`),
   where every error variant must be classified or the build fails. The exception is `POST /oauth/token` and
   `POST /oauth/revoke`, which refuse requests in the RFC 6749 format so any OAuth client library understands them;
   a failure of the server itself there is still a problem response.
3. Paths name things by handle: `{team}` and `{heap}` are slugs, `{item}` a key, `{username}` a username. Tags,
   parameters, options and views keep their IDs, checked against the heap in the path. A handle that's malformed or
   names nothing the caller can see is `404`. Responses carry both the `id` and the handle; request bodies refer to
   other things by ID, since clients have the objects in hand. Creating a team or heap takes an optional slug,
   derived from the name when left out.
4. Updates are `PATCH` with partial bodies and the expected `version` in `If-Match`. Endpoints that say what
   something should end up as (archive, unarchive) take no `If-Match`, so repeating or racing them is safe, and a
   write that is already applied succeeds (adding a member who is one, removing someone who isn't). Request bodies
   refuse unknown fields, so a misspelt field fails instead of saving nothing.
5. List endpoints paginate with a cursor, except the ranked list ([Search](#search)).
6. Responses are ready to show, so clients compute nothing: `due` as `{ at, firm, label, overdue }` in the
   request's timezone, `contributors`, who did what and when as a `UserRef` and a label ("4 days ago"),
   `closed_group` as a code the client has headings for, `allowed_actions`, and `null` for unset values.
7. Responses return the whole aggregate: a team with its members and heaps, each with their `allowed_actions`, so a
   client re-renders from one response after any change (promoting someone changes what can be done with every
   other admin); a parameter with its options and tag links; `GET /me` with the caller's teams and heaps and their
   counts.
8. Saved views belong to the heap, not to whoever saved them: every member sees, changes and archives them, as with
   tags. An archived view is left out of the list and refuses edits; there is no unarchive, since saving the view
   again is as quick.
9. Archiving an option switches the open items that have it to the replacement and bumps their versions, so an edit
   made from the old value fails with a version conflict instead of undoing the switch. A closed item keeps the
   archived option; when it's reopened, its values are brought up to date the same way, in the reopening's audit
   entry.
10. A display name change is audited once in each of the user's teams, since the audit log is per team and that is
    where teammates see the name. A user in no team has nowhere to record it.

## Authentication and authorization

1. **Web app:** a cookie session (`HttpOnly`, `Secure`, `SameSite=Lax`). The cookie holds a random 256-bit token;
   the database stores only its SHA-256, so a leaked database can't be used to sign in, and no session secret is
   needed. Every state-changing request, sign-in included, must carry an `X-CSRF-Protection` header. Browsers only
   let same-origin scripts set custom headers (cross-origin ones need a CORS preflight, which the server never
   approves), so the header together with `SameSite=Lax` blocks cross-site request forgery without a per-session
   token.
2. **Non-web clients:** the OAuth 2.0 authorization code flow with PKCE (S256) against the server's own endpoints
   (RFC 6749, 7636 and 7009).
   - The client opens the browser on the web app's `/oauth/authorize` page and listens on a loopback redirect URI.
     The page checks the request with the API before it shows anything, so it never offers a request the server
     would refuse. The user signs in if needed and chooses Allow, which approves the request with the session cookie
     and CSRF header (a bearer token can't approve more access for itself), or Cancel. The client then exchanges the
     code at `/oauth/token` for an access token and a refresh token.
   - Only registered first-party public clients are allowed, and the registry is a constant in `domain::oauth`, not
     a table. Redirect URIs are loopback only (RFC 8252), on any port, with no `localhost`, other hosts, user info,
     query or fragment, so adding `?code=…` always gives the URI that was approved.
   - Codes and tokens are random 256-bit values stored as SHA-256 hashes, like session tokens. Codes work once and
     briefly; access tokens are short-lived and refresh tokens rotate ([Non-web client tokens](#non-web-client-tokens)).
   - `Authorization: Bearer` resolves to the same `Principal` a cookie does, on every endpoint. A request with a
     bearer token is authenticated by it alone, with no fallback to the cookie, and needs no CSRF header: a
     cross-site page can't set that header either, and CSRF is a cookie problem. Other `Authorization` schemes, such
     as the `Basic` credentials a browser sends to a proxy, are ignored and the cookie rules apply.
   - No page of the web app may be framed by another site (`X-Frame-Options: DENY`, `Content-Security-Policy:
     frame-ancestors 'none'`), so a page can't hide the consent page under something that gets the user to click
     Allow.
   - A request that's wrong in some other way than its client or redirect URI is shown as an error on the page, not
     sent back to the client as RFC 6749 suggests: the only client is first-party, so such a request is a bug the
     person at the browser should see, and the CLI gives up waiting after a few minutes.
   - A proxy that puts HTTP Basic auth in front of the server works for the web app but not for the CLI, since a
     request carries one `Authorization` header. Such a proxy has to let `/api` through.
   - All of it is part of the `accounts` feature; the local build serves none of it.
3. **External identity providers** (planned): generic OIDC, configured per deployment. A first sign-in creates the
   user, after asking for a username. An identity is matched by `(provider, subject)`, never by email alone.
4. **Local accounts:** passwords are hashed with argon2id, sign-in takes an email address or a username, and
   attempts are rate-limited per account, keyed by its email address whichever was typed, or per unknown address or
   username.
   - A fixed number of attempts per window (five per 15 minutes). Later attempts get `429` with `Retry-After`,
     refused before any password is checked, so they cost no argon2 work. A successful sign-in clears the count.
   - Unknown addresses are counted the same way, so a refusal doesn't reveal whether an account exists.
   - Counts live in the database (`sign_in_attempts`), keyed by the SHA-256 of the normalised address, so they hold
     across instances. Each attempt is one upsert in a transaction, so concurrent attempts are each counted.
     Addresses are compared exactly on every database (`utf8mb4_bin` on MySQL), or MySQL would find one account
     under many spellings, each with its own attempts.
   - There is no per-IP limit, because the server sits behind a proxy and doesn't know client addresses. Spraying
     one password across many accounts is left to the reverse proxy.
5. **Authorization:** the API resolves a cookie or a bearer token into one `Principal`, and the service layer
   enforces access ([Access control](#access-control)).

## Access control

### Permissions as code

1. The roles table in [offering.md](offering.md#roles-and-permissions) is one typed enum in `domain::permission`,
   with a function that says whether a role grants each permission. One permission can back several rows of the
   table when they always grant the same roles; if the rows ever diverge, they get separate permissions.
2. Every service use case declares the permission it requires and checks it first, before reading or writing
   anything else. Handlers never check permissions; they pass the `Principal` to the service.
3. `allowed_actions` in responses comes from the same functions, so the checks and what clients show can't drift
   apart.

### Resource scoping

1. Every resource resolves to exactly one team: heap → team; item, tag, parameter, option and view → heap → team.
   The service resolves the owning team first, then loads the caller's membership in it.
2. A caller who isn't a member gets `404`, the same as for a resource that doesn't exist, so an outsider can't probe
   for IDs. A member without the permission gets `403`.
3. Every repository query that loads heap-level data filters by the resolved heap, so an ID from another heap can
   never leak in; one sent in a request body is refused with `422`.
4. Finding users to add (`GET /users`) is for admins of a shared team (everyone is admin of their personal team,
   which doesn't count) and is built so it can't list addresses: a name matches by word prefix, an email only when
   typed in full, and a match shows the email only when the email matched.

### Archived heaps

1. An archived heap is read-only for everyone, admins included. Every write use case on a heap and its contents
   first checks that the heap is active, under the heap lock, and refuses with `409` otherwise.
2. Unarchiving is the only write allowed on an archived heap, and only an admin can do it. `allowed_actions` is
   empty for anything in an archived heap, apart from `unarchive` for admins.

### Rules that depend on state

Rules such as a team keeping at least one admin, reopening within a week, closed items being read-only, no blocking
cycles, an archived option needing a replacement, a parameter being archived only when unused, and names being
unique run in the same transaction as the write, after the permission check, with the rows they depend on locked as
described under [Cycle detection and locking](#cycle-detection-and-locking). Each failure is a `409` or `422` with
a stable code; the codes are in `crates/api/src/error.rs`, and the rules themselves in `domain`. The team rule is
the one that locks the team row rather than a heap: every team write locks it before any other read, then counts
the remaining admins.

### Non-web client tokens

1. Access tokens issued to the CLI carry the user's full permissions; there are no narrower scopes yet. Scoped
   tokens could be added without changing the permission model, since a scope would only ever narrow a role's
   permissions.
2. Access tokens are short-lived and refresh tokens rotate on each use. A family is one sign-in by a client: the
   pair the code was exchanged for and every pair refreshed from it, one row in `oauth_token_families`. Refreshing
   uses the refresh token up and issues a new pair in the same family, so a client in use stays signed in.
3. A refresh token presented a second time means someone holds a copy, so the whole family is revoked, the
   legitimate client included, which then signs in again. A code presented twice within its lifetime revokes the
   family its first exchange issued (RFC 6749 §4.1.2); past that it's only refused, so a code left in a browser's
   history can't end the sign-in later.
4. Single use is a conditional update (`used_at IS NULL`), so of two racing requests one wins and the other counts
   as reuse. A refresh locks the family row and a revocation updates it, so the two run one after the other. On
   SQLite, which has no row locks, the loser of such a race may fail with a server error rather than be refused as
   reuse; tokens are never issued twice either way.
5. Revoking either token of a family (`POST /oauth/revoke`) revokes the family, and answers `200` for unknown tokens
   too (RFC 7009). `POST /auth/logout` ends only a session cookie. Listing and revoking one's own client tokens
   (`/me/tokens`) is designed, not built.

## Audit log

1. Each use case runs in one database transaction, performs the change and inserts the audit entry before
   committing.
2. `action` is a typed enum (`ItemCreated`, `OptionArchived`, `MemberPromoted`, …) and `diff` stores the changed
   fields as before/after pairs.
3. Each action is shown to those who hold the permission it takes (`audit_permission` in `domain::permission`), and
   the read filters on it in SQL so paging stays exact. Members see item, tag and saved view changes, and display
   name changes; admins additionally see heap, parameter, team and membership changes. Leaving a team is a
   membership change, so only admins see it, although any member may leave.
4. The heap log reads what is recorded against the heap. The team log reads the team's own entries and the heaps'
   own entries (created, renamed, archived), but not what happens inside the heaps. With `entity_id`, either reads
   one entity's history.
5. Entries come ready to show: the actor's and the entity's current names, a time label in the request's timezone,
   a `phrase` to read after the actor's name, and `changes`, one per changed field with the values before and after
   as text and IDs replaced by current names. The raw `diff` is returned too.
6. The web app shows history where things are edited: under the item, in a collapsed History on each tag,
   parameter, heap and member, and the whole logs on the heap's and the team's Activity pages. Parameters and heaps
   offer `view_history` only to those who may see their entries, so members aren't shown histories that are always
   empty.

## Configuration and deployment

1. Configuration comes from environment variables, validated at startup; the variables and their defaults are in
   [README.md](../README.md#configuration). No session secret is needed, because session tokens are random and
   stored hashed rather than signed.
2. On startup the server runs pending migrations, then serves the API and the web app on one port. On Postgres the
   run holds a transaction-scoped advisory lock, so instances starting together apply each migration once; MySQL and
   SQLite aren't run with several instances.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at Postgres or MySQL. TLS terminates at a reverse proxy, or at Cloud Run in
   front of a Neon database ([cloud-run-postgres.md](cloud-run-postgres.md)).
5. **Health checks:** `/healthz` returns `200` while the process is up. `/readyz` returns `200` only once migrations
   have finished and the database answers a ping, so orchestrators route traffic only to ready instances. Neither
   needs authentication, and neither reveals versions or configuration.
6. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server
   with a clear message before it listens. On SIGTERM it stops accepting connections and drains in-flight requests.
7. **Local build:** a static site with no configuration, published to GitHub Pages. One user and one team, which
   holds every heap, with the data in the browser ([local-build.md](local-build.md)).

## Testing

1. **Domain:** unit tests for each ranking factor, the state machine and the due labels, and proptest invariants:
   ranking is deterministic, an item blocking another never ranks below it, and editing one item under a fixed
   `as_of` doesn't change the relative order of the others.
2. **Access control:** a table-driven test runs every endpoint as a non-member, a member and an admin, and every
   write endpoint against an archived heap. Expected statuses come from the permission table, so an endpoint added
   without a row fails the test.
3. **Store and service:** the store and API suites (the API suite drives the services) run against in-memory
   SQLite, and again against MySQL and Postgres in CI. `TEST_DATABASE_URL` points them at a server, and each test
   creates its own database there. Service containers rather than testcontainers keep `cargo test` free of Docker
   and the tests free of container start-up.
4. **API contract:** `openapi.json` and the generated TypeScript types are checked in, and CI fails if either
   changes without being regenerated.
5. **End to end:** Playwright covers the golden paths (sign up and sign in, add and rank, block and unblock, close
   and reopen, parameter management) against the real binary on in-memory SQLite, at phone, tablet and desktop
   widths. Each test registers its own account, so tests share no data and run in parallel; everything beyond these
   paths is tested lower down. The same paths, without sign-up, run against the local build, with its own tests for
   export, import and the tab lock.

The commands are in [README.md](../README.md#develop).
