# Technical design

Draft. Companion to [requirements.md](requirements.md) and [offering.md](offering.md). Coding rules are in [engineering-principles.md](engineering-principles.md).

## Principles

1. **Thin clients.** The API server does all computation: ranking, score contributors, filtering, sorting, fuzzy due labels, permission checks, and state rules such as whether an item can still be reopened. Clients render what the API returns and send user intent back. Clients do two things themselves: text search over the list they already have (see [Search](#search)), and animations. Ranking, permission and state-machine logic exist only on the server, so a new client (CLI, native app) needs nothing beyond an HTTP client and the generated API types.
2. **The server tells clients what is allowed.** Responses include the actions the current user can take (for example `can_reopen` or `can_edit_parameters`), so clients don't re-implement role or state rules to decide which controls to show.
3. **Database-agnostic.** Only portable SQL features. No JSON column types, database-specific full-text search, triggers or stored procedures. Everything is tested against SQLite, MySQL and Postgres.
4. **Nothing is deleted, and everything is audited.** There are no `DELETE` statements for domain data. Every write and its audit entry are committed in the same transaction.
5. **Single binary.** One statically linked executable serves the API and the embedded web app. The [local build](local-build.md) is the one other deliverable: the same backend crates compiled to WebAssembly and shipped with the web app as a static site.

## Stack

| Area | Choice |
| --- | --- |
| Language (backend) | Rust (stable), Cargo workspace |
| HTTP | axum on tokio, tower middleware |
| API contract | Code-first OpenAPI 3.1 via utoipa; TypeScript types for the web app generated with openapi-typescript |
| Data access | SeaORM (on sqlx), runtime-selected backend: SQLite, MySQL or Postgres. The local build uses SeaORM's proxy connection over SQLite-wasm |
| Migrations | sea-orm-migration, embedded in the binary, run on startup |
| Auth | `openidconnect` + `oauth2` crates (external IdPs), `argon2` (local passwords), own session table with hashed random tokens (web sessions), own minimal OAuth authorization-code + PKCE endpoints (non-web clients) |
| Web app | React + Vite + TanStack Query + TanStack Router, Tailwind, Motion (layout animations); embedded with `rust-embed` |
| Observability | `tracing` with JSON logs, `/healthz` and `/readyz` |
| Testing | `cargo test`, `proptest` for ranking invariants, MySQL 8.4 and Postgres 17 service containers in CI, Playwright for end to end |
| Packaging | Static musl binary; minimal Docker image; example docker-compose with MySQL. The local build is a static site on GitHub Pages |

## Workspace layout

```
crates/
  domain/    Pure logic, no IO: types, state machine, ranking, fuzzy due labels, validation
  store/     SeaORM entities, migrations, repositories
  service/   Use cases: authorization, transactions, audit logging, orchestration
  api/       axum routes, request/response DTOs, OpenAPI, auth flows, sessions
  server/    Binary: config, startup, migrations, embedded web assets
  local/     WebAssembly entry point for the local build: clock, proxy database, migrations, Router
web/         React app (TypeScript), consumes generated API types only
```

Dependencies point one way: `server → api → service → store, domain`, and `local` sits beside `server`. `service` and `api` put what only a shared server needs behind their default `accounts` feature, which the local build leaves out ([local-build.md](local-build.md#single-user-mode)). `domain` depends on nothing in the workspace, so the ranking logic can be unit- and property-tested in isolation.

### Errors

1. Library crates (`domain`, `store`, `service`) define typed error enums with `thiserror`. Callers can match on them, and each variant maps to one stable error `code`.
2. `api` converts service errors to `application/problem+json` responses in exactly one place, an `IntoResponse` implementation. No handler builds an error response by hand.
3. `anyhow` is used only in `server`, for startup and configuration failures that end the process, and in tests.

## Data model

Conventions:

- **IDs** are UUIDv7, stored as `CHAR(36)`. They are sortable by creation time and safe to expose.
- **Handles** name things in URLs, API paths and the CLI ([readable-ids.md](readable-ids.md)): usernames, team slugs, heap slugs (unique per team) and item keys (five random characters, unique per heap). They are extra columns with unique indexes; every foreign key still points at the UUID.
- **Timestamps** are UTC Unix milliseconds in `BIGINT`. This avoids differences between the SQLite and MySQL date types. No timezone is stored anywhere (see [Time and timezones](#time-and-timezones)).
- **Structured data** (audit diffs, saved list views) is serialized JSON in `TEXT` and never queried inside the database.
- **Archiving** uses a nullable `archived_at`. There are no hard deletes.
- **Foreign keys** never cascade. They use `ON DELETE RESTRICT`, so even an accidental `DELETE` fails instead of removing related rows.
- **Optimistic concurrency:** mutable rows carry a `version` integer. Updates require the expected version and fail with `409 Conflict` otherwise.
- **State and `closed_at`:** in the database, `closed_at` is set exactly when `state` is done or won't do, and `closed_by` only then. In `domain`, the state is an enum where only the closed variants carry `closed_at` and `closed_by`, so an open item with a close time or a closer can't be built. The store converts between the two and rejects rows that break the rule.

Tables:

| Table | Key columns |
| --- | --- |
| `users` | id, display_name, email, created_at, version |
| `password_credentials` | user_id, argon2 hash, updated_at |
| `identities` | id, user_id, provider, subject (unique per provider) |
| `sessions` | token_hash (SHA-256 of a random 256-bit token), user_id, created_at, expires_at |
| `oauth_auth_codes` | code_hash (SHA-256 of a random 256-bit code), user_id, client_id, redirect_uri, code_challenge (S256), created_at, expires_at, used_at and family_id (set together when the code is exchanged) |
| `oauth_token_families` | id, user_id, client_id, created_at, revoked_at |
| `oauth_tokens` | token_hash (SHA-256 of a random 256-bit token), family_id, kind (access/refresh), created_at, expires_at, used_at (a refresh token's exchange) |
| `teams` | id, name, slug (unique; for a personal team, its owner's username), personal_user_id (unique, nullable: set on a user's personal team), created_at, version |
| `team_members` | team_id, user_id, role (admin/member), joined_at |
| `heaps` | id, team_id, name, slug (unique per team), age_weight, deadline_weight, value_weight, archived_at, version |
| `items` | id, heap_id, key (unique per heap), title, description, state, need, effort, impact, due_at, due_firm, blocked_by_item_id, created_at, created_by (nullable), closed_at, closed_by (nullable), version |
| `tags` | id, heap_id, name, name_key (lowercased name, unique per heap), description, created_at, version |
| `item_tags` | item_id, tag_id, added_at, removed_at (set when the tag is taken off; adding it again clears it) |
| `parameters` | id, heap_id, name, weight, created_at, archived_at, version |
| `parameter_options` | id, parameter_id, label, position, archived_at, replaced_by_option_id |
| `parameter_tags` | parameter_id, tag_id, added_at, removed_at |
| `item_parameter_values` | item_id, parameter_id, option_id (`NULL` once cleared) |
| `saved_views` | id, heap_id, name, list_view (JSON text), created_by, created_at, archived_at, version |
| `audit_log` | id, team_id, heap_id (nullable), actor_user_id, action, entity_type, entity_id, diff (JSON text), created_at |

Notes:

- Effort and impact are columns on `items` rather than custom parameters. They are fixed, apply to every heap, and share one weight (`heaps.value_weight`).
- Need is a column on `items` too, for the same reasons, and is never null. Its weight is `heaps.need_weight`.
- `items.created_by` is the user who added the item. The migration that added it filled it in from each item's `item_created` audit entry; it stays nullable, so an item whose entry is missing keeps no creator instead of failing the migration, and the API returns `created_by: null` for it.
- `items.closed_by` is the user who closed the item, set on closing and cleared on reopening. The migration that added it filled it in for closed items from the audit log, matching each item's `closed_at` to the `created_at` of its `item_updated` entry, since closing writes both from the same instant; no JSON diff is read. Items with no matching entry, or with entries by different users at that instant, keep `NULL`, and the API returns `closed_by: null` for them, and for open items.
- The need, age and deadline weights live on `heaps`, next to `value_weight`. Together with the parameter weights, they make up the heap's ranking policy.
- A user's username is the slug of their personal team, so usernames and team slugs share one namespace under one unique index. A personal team has its owner as its only member, can't take others, and its slug never changes.
- A team always has at least one admin. The service layer enforces this inside the transaction that changes membership.
- The application can only insert into `audit_log`. There is no update or delete code path for it.
- Tag names are unique per heap ignoring case. The check runs on `name_key` rather than the column collation, because MySQL compares case-insensitively by default and SQLite doesn't.
- A parameter is one aggregate: its options and tag links have no version of their own, so every change to them takes the parameter's version. Options in use are ordered by `position`, highest priority first. An archived option always names its replacement (`archived_at` and `replaced_by_option_id` are set together), so a value can always be followed to an option in use. Unlinking a tag sets `parameter_tags.removed_at`, and clearing an item's value sets `option_id` to `NULL`, so neither deletes a row. Parameter names are unique among a heap's parameters in use, ignoring case; archived parameters keep their names, so a unique index can't express it, and the service checks it under the heap lock.
- `oauth_token_families` holds one row per sign-in by a client; its tokens are the pair the code was exchanged for and every pair refreshed from it, and take their user and client from it. Revoking is setting the family's `revoked_at`, which ends every token in it: a token is accepted only while its family isn't revoked. One row per family also gives a refresh and a revocation one row to serialise on (see [Non-web client tokens](#non-web-client-tokens)), and `/me/tokens` its list. `client_id` names a client of the registry in code (`domain::oauth`), so it has no foreign key. Used codes and spent tokens stay for now; like sessions they are plumbing that may be deleted, and cleaning them up comes with `/me/tokens`.
- Taking a tag off an item sets `item_tags.removed_at` instead of deleting the row, so relationship rows follow the no-`DELETE` rule too. The change is audited as an `item_updated` entry whose diff lists the tag IDs before and after. Team membership is the one exception (see [Resources](#resources-initial-set)).

## Ranking

### Computation

Ranking is a pure function in `domain`:

```rust
fn rank(formula: FormulaVersion, policy: &HeapPolicy, items: &[ItemForRanking], as_of: Timestamp) -> Vec<RankedItem>
// RankedItem { item_id, position, contributors: Vec<Contributor> }
```

1. The service loads the heap's policy, all non-closed items, their parameter values, and the blocking edges, then calls `rank`. Heaps are expected to hold at most a few thousand open items, so ranking them in memory on each request is cheap. A per-heap cache invalidated on write can be added if measurements call for it.
2. Each factor (need, age, deadline proximity, value per effort, each custom parameter) produces a weighted component. The final score is a combination of these. The formula lives behind one function, so changing it doesn't affect the API.
   - **Formula versioning:** each formula is a named version (`FormulaVersion::V2`, …). The server uses one version at a time, set in code, not stored per heap. Tests pin expected orderings per version, so a formula change shows up as an explicit test update. Because scores are never stored, switching versions needs no migration.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are rejected on write. Each item's effective score is the maximum of its own score and the effective scores of the items it blocks, times a boost (1.1 in `V2`). This is computed with a memoized walk over the graph, so it is linear in the number of items. Equal scores go to the item that blocks the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
   - **Need** in `V2` counts twice. On its own it adds its weight times 0 for an idea, 0.25 for nice to have, 0.5 for expected and 2 for critical: the deadline factor's pull nine days out, three days out, and just past an exact deadline. It also scales age: an idea ages at a quarter of the rate of an expected item, nice to have at half, critical at one and a half. The contributor reads "Need: Critical". An idea's need adds nothing and has no contributor, so when the rate isn't an expected item's the age contributor carries it: "Waiting 12 months (idea, ages slowly)", "Waiting 3 weeks (critical, ages fast)". The need component comes last, so a tie in the contributors goes to the more telling factor. `V2` replaced `V1`, which had no need: items from before it count as expected, which leaves their age as it was, so they mostly keep their order among themselves. Blockers are the exception: expected adds the same amount to every item, and a blocker takes on 1.1 times the score of what it blocks, so a blocker can move above an item it was just below.
   - **Custom parameters** in `V2`: each parameter in use adds its weight times where the item's option sits in the parameter's order, from 1 for the first option to 0 for the last. An unset value counts as the last option, so a parameter with a single option adds nothing, and an archived option counts as the option that replaced it. Archived parameters don't count. The contributor reads "Customer: Enterprise".
4. **Contributors** are the largest weighted components, returned with a label and a relative share (for example "Deadline in a few days" or "Blocks *Migrate auth*"). Scores themselves are never returned.
5. **Ties** that the chain length (point 3) doesn't settle are broken by `created_at`, oldest first. The ID is the final tie-breaker, which makes the order fully deterministic.

### Session-stable order

The score is a deterministic function of item data and time. A stable session therefore needs no server-side state, only a fixed `as_of` time.

1. The first list request returns an `as_of` timestamp along with the ranked page.
2. The client sends the same `as_of` on later requests: pagination, refetches after an edit, and filter changes.
3. Items whose data hasn't changed keep their positions, because the time input is frozen. An edited item is ranked with its new data, so it moves right away (Priority §6).
4. Edits by teammates move items the same way, on the client's next refetch. The client refetches when the window regains focus and on a short interval. Server push (SSE) can replace polling later.
5. Whenever positions change between two responses, the web app animates the items to their new positions. It uses layout animations (FLIP) keyed by item ID. New items fade in, and items that leave the list fade out.
6. A "refresh" action, or reloading the page, drops `as_of`. The server then ranks at the current time.
7. A client must not use an `as_of` more than about 24 hours old. Past that, the server ignores it and uses the current time.
8. Every label in the list (due, created, closed) is worked out at `as_of` too, so the list reads as one moment. Responses for a single item use the current time, so an item's labels there can be a day ahead of the list's.

### Search

1. The ranked list endpoint returns every open item in the heap in one response, with no pagination. This is fine at the expected size of a few thousand items.
2. The client does text search itself, over the titles and descriptions it already has. It keeps the matches in their ranked order and filters as the user types, without a request per keystroke.
3. Filters by state, tag and parameter value stay on the server, because they change which items are pinned and how they are grouped. Ranking always runs over every non-closed item, since a blocker inherits from what it blocks even when that item is filtered out or hidden; the view then picks, groups and sorts the result.
   - Each non-closed state is `pinned`, `shown` or `hidden` (`open=`, `in_progress=`, `blocked=`; by default in progress is pinned and blocked is hidden). Shown blocked items get their own group or are mixed in (`blocked_layout=`). The response counts what hidden states leave out (`hidden_count`), what the view would list without its filters (`unfiltered_count`, for "6 of 14"), and the heap's in-progress items, whatever the view and filters leave out (`in_progress_count`, so a client offers to start the next item only when nothing else is under way).
   - These defaults are the product's defaults, so every client starts from the same list. Before this, the response always carried blocked items and the web app hid them; the change was made before any release, when the web app was the only client.
   - `tag=`, `need=`, `effort=` and `impact=` take comma-separated values and keep items with any of them. A malformed value is `400` `invalid_query`; a tag of another heap is `422` `tag_not_in_heap` with `field` `tag`.
   - `param=` takes option IDs of the heap's parameters, comma-separated. For each parameter they belong to, it keeps items with any of its listed options, and the parameters combine like the other filters. An archived option matches the items that switched to its replacement, so a saved view keeps working after an option is archived. An unknown option is `422` `option_not_in_heap` with `field` `param`.
   - `sort=` is `priority`, `due` (soonest first), `effort` (smallest first), `impact` (largest first) or `parameter`, with the parameter in `sort_parameter=` (its first option first). `sort_parameter` without `sort=parameter`, or the reverse, is `422` `sort_parameter_mismatch`; a parameter of another heap is `parameter_not_in_heap`. Archived parameters and options are accepted in filters and sorts, for the same reason as above; options of an archived parameter don't filter, and a sort by one keeps priority order, since archived parameters count nowhere else either. Items without the value come last, and ties keep priority order. Grouped blocked items always stay in priority order (Req Views: List items §3). `position` is the place in the returned group.
   - The web app keeps the view in the URL, so it survives reloads and can be shared, but not the search text: it changes on every keystroke and is a momentary narrowing rather than part of the view. Saved views store the URL's view: the same settings, under the query parameters' names, so a client turns a saved view into a list query field by field. The web app hides saved views for now, so it neither adds `view=<id>` to the URL nor offers to save a change into a view; the API keeps them for clients that do.
4. If heaps grow beyond this size, search moves to the server with per-database full-text indexes. Clients would then call `GET /teams/{team}/heaps/{heap}/items?q=`.

### Time and timezones

1. All instants are stored and sent as UTC timestamps. The server stores no timezone, for users or anywhere else.
2. Clients send the device's IANA timezone on every request, in a `Time-Zone` header such as `Europe/Berlin`. The browser reads it from `Intl.DateTimeFormat().resolvedOptions().timeZone`. If the header is missing, the server uses UTC.
3. **Dates without a time,** such as a due date, are stored as the end of that day in the user's timezone at the time of entry, then converted to a UTC timestamp. For example, 12 March entered in Berlin is stored as 12 March 22:59:59.999 UTC.
4. The server uses the request's timezone to compute presentation values, such as fuzzy due labels, formatted dates and "overdue".
5. The one-week reopen window is measured as exactly 7 × 24 hours from `closed_at`, so it doesn't depend on any timezone.

### Cycle detection

Setting `blocked_by_item_id` walks the blocker chain inside the write transaction. If the walk reaches the item being blocked, the API returns `422` with a `blocking_cycle` error.

1. Every item and tag write first locks the heap row (`SELECT … FOR UPDATE` on MySQL), before touching any other row, so writes in one heap run one after the other and always take locks in the same order. The write checks the heap isn't archived on the row read under that lock, and archiving takes the same lock, so a write never lands in a heap archived while it waited. The walk then reads each link with a locking read, so of two writes that would close a cycle between them, the second sees the first. SQLite allows one writer at a time, so the second of two such writes fails instead.
2. Only a blocked item has a blocker: the blocker is part of the blocked state, and leaving that state clears it. Sending a blocker for an item that isn't blocked, and isn't moved to blocked in the same request, returns `422` `item_not_blocked`.
3. The blocker must be an item of the same heap (`blocker_not_in_heap`, which also covers unknown IDs) that isn't closed (`blocker_closed`). Closing a blocker later doesn't unblock what waits for it; the blocked item shows the closed blocker until someone moves it on.
4. Ranking still tolerates a cycle in stored data, by ignoring the edge that closes it, so one bad row can't break the list.

## API

### Conventions

1. All endpoints are under `/api/v1`, JSON over HTTPS.
2. Errors use `application/problem+json` with a stable `code` field. The one exception is `POST /oauth/token` and `POST /oauth/revoke`, which refuse a request in the RFC 6749 §5.2 format (`{"error": "invalid_grant", "error_description": …}`, `400`, or `401` for `invalid_client`), so any OAuth client library understands them. A failure of the server itself there (`500`, `503`) is still a problem response, since the RFC defines no error for it.
3. Updates are `PATCH` requests with partial bodies. The expected `version` goes in an `If-Match` header.
4. List endpoints paginate with a cursor, except the ranked list, which returns every open item in one response (see [Search](#search)).
5. Responses carry presentation-ready derived data, so clients don't compute anything:
   - `due`: `{ at, firm, label, overdue }`, where `label` is the fuzzy text ("in a few weeks") for loose dates or a formatted date for exact ones, in the timezone from the request's `Time-Zone` header.
   - `contributors`: a list of labelled factors.
   - `created_by` and `created_label` on items: who added it (`{ id, display_name }`, the `UserRef` audit entries use for their actor) and how long ago ("4 days ago"), in the request's timezone. `UserRef` was called `AuditActor` until items used it too. The JSON didn't change, and the schema was renamed in place because the API has no outside clients yet.
   - `closed_by` and `closed_group` on items: who closed it (a `UserRef`) and which part of the closed list it belongs in, `today`, `this_week` (1 to 6 calendar days before) or `earlier`, in the request's timezone. `closed_group` is a code, like `state`; the client has the headings. Both are `null` for open items. Clients group consecutive closed items by `closed_group` rather than comparing dates.
   - `allowed_actions`: for example `["edit", "reopen", "set_blocked"]`.
   - `unset` markers for parameters with no value.

### Resources (initial set)

| Area | Endpoints |
| --- | --- |
| Auth | `POST /auth/register`, `POST /auth/login`, `POST /auth/logout`, `GET /auth/oidc/{provider}/start`, `GET /auth/oidc/{provider}/callback` |
| OAuth | `GET /oauth/authorize` (checks a request for the consent page), `POST /oauth/authorize` (approves it), `POST /oauth/token`, `POST /oauth/revoke` |
| Me | `GET /me` (the caller, their teams with `member_count`, and each team's heaps with `open_count`), `PATCH /me` (display name), `GET /me/tokens`, `POST /me/tokens/{id}/revoke` |
| Teams | `POST /teams`, `GET/PATCH /teams/{team}` (the team with its members and heaps; name and slug), `POST /teams/{team}/members`, `PATCH/DELETE /teams/{team}/members/{username}` (role change, removal), `POST /teams/{team}/leave`. The caller's teams are listed by `GET /me` |
| Users | `GET /users?query=` (lookup for adding members, for admins of a shared team) |
| Heaps | `POST /teams/{team}/heaps`, `GET/PATCH /teams/{team}/heaps/{heap}` (name and slug), `POST /teams/{team}/heaps/{heap}/archive`, `POST /teams/{team}/heaps/{heap}/unarchive` |
| Ranked list | `GET /teams/{team}/heaps/{heap}/items?as_of=&open=&in_progress=&blocked=&blocked_layout=&tag=&need=&effort=&impact=&param=&sort=&sort_parameter=` returns the pinned, ranked and blocked groups, `hidden_count`, `unfiltered_count`, `in_progress_count`, and the `as_of` value |
| Closed items | `GET /teams/{team}/heaps/{heap}/items/closed?cursor=` returns closed items, most recently closed first, each with its `closed_by` and `closed_group` |
| Items | `POST /teams/{team}/heaps/{heap}/items`, `GET/PATCH /teams/{team}/heaps/{heap}/items/{item}`, `POST /teams/{team}/heaps/{heap}/items/{item}/reopen` |
| Due choices | `GET /due-choices`: the rough due times a due picker offers ("next week"), each with its date in the request's timezone and the label an item due then reads back with |
| Tags | `GET/POST /teams/{team}/heaps/{heap}/tags`, `PATCH /teams/{team}/heaps/{heap}/tags/{id}` |
| Parameters | `GET/POST /teams/{team}/heaps/{heap}/parameters` (in use and archived; add one), and under it `PATCH /parameters/{id}` (name, weight), `POST /parameters/{id}/archive`, `POST /parameters/{id}/options` (add one last), `PATCH /parameters/{id}/options` (every option in use, in the new order, with its label), `POST /options/{id}/archive` (with `replacement_option_id`), `PUT /parameters/{id}/tags` |
| Saved views | `GET/POST /teams/{team}/heaps/{heap}/views` (the views in use, by name; save one), `PATCH /teams/{team}/heaps/{heap}/views/{id}` (rename, or replace the whole view), `POST /teams/{team}/heaps/{heap}/views/{id}/archive` |
| Audit | `GET /teams/{team}/heaps/{heap}/audit`, `GET /teams/{team}/audit`, newest first, cursor-paged, `entity_id=` for one entity's history; filtered by what the caller may do |

Paths name things by handle: `{team}` is a team slug, `{heap}` a heap slug, `{item}` an item key and `{username}` a username. Tags, parameters, options and views have no handle and keep their IDs, checked against the heap in the path. A handle that's malformed or names nothing the caller can see is `404`. Responses carry both the `id` and the handle; request bodies refer to other things by ID (`tag_ids`, `blocked_by_item_id`). Creating a team or heap takes an optional `slug`, derived from the name when left out; a taken one is `409 handle_taken` and a malformed one `422 invalid_handle` with a `hint`. A personal team refuses members, leaving and slug changes with `409 personal_team`.

The `DELETE` on a team member removes the membership. That is the one place where a relationship row goes away, and it is audited. The membership history stays in the audit log.

`GET /me` carries `allowed_actions` for the account (`sign_out`, `create_team`); the local build's one user gets none of them, and only `create_heap` on their team ([local-build.md](local-build.md)). Team endpoints return the whole team, members and heaps included, with `allowed_actions` on the team, each member and each heap, so a client re-renders the team from one response after any change (promoting someone, for example, changes what can be done with every other admin). Adding a member who is already one answers `200` with the team unchanged; removing someone who isn't a member succeeds. Archiving and unarchiving take no `If-Match`: each says what the heap should end up as, so repeating or racing them is safe. Every heap the API returns as a `Heap`, and each heap `GET /me` lists, carries `open_count`: the items its list shows by default, before filters (open and in progress, the `unfiltered_count` of the default view, from the one `ListView::listed_states` definition). It is counted in one grouped query per request, for the Teams and Team pages; `HeapSummary` in the list responses doesn't carry it, as the list already counts its own items.

Saved views belong to the heap, not to whoever saved them: every member sees them, and can change and archive them, as with tags. They are archived rather than deleted, and an archived view is left out of the list and refuses edits (`saved_view_archived`); there is no unarchive, since saving the view again is as quick. Archiving takes no `If-Match`, like archiving a heap. A view may filter only by tags of its heap (`tag_not_in_heap` with `field` `view.tag`), and by at most 200 of them (`too_many_tags`), which keeps the stored view well inside a MySQL `TEXT` column. Request bodies refuse unknown fields (`invalid_body`), so a misspelt field fails instead of saving nothing. While the web app hides saved views it doesn't read `view`, and drops it from the URL like any unknown parameter. Names need not be unique. Writes lock the heap and then the view, and read the view under its lock, for the MySQL snapshot reason given under [Rules that depend on state](#rules-that-depend-on-state).

Parameter endpoints return the whole parameter, options and tag links included. Changes take the parameter's `version` in `If-Match`; archiving a parameter or an option says what it should end up as, so it takes none. Any member lists parameters, since members set the values on items (`parameter_values` on create and edit, by parameter ID, `null` to clear); only admins change them (`ConfigureHeap`). A value set must be an option in use of a parameter in use (`parameter_not_in_heap`, `option_not_in_parameter`); clearing one works for archived parameters too. Archiving an option switches the open items that have it to the replacement and bumps their versions, so an edit made from the old value fails with `version_conflict` instead of undoing the switch; the `option_archived` entry lists the switched items, and each switched item gets an `item_updated` entry naming the value that changed. A closed item keeps the archived option, and when it is reopened its values are brought up to date the same way: each one follows its replacements, and values of archived parameters are dropped, all in the reopening's `item_updated` entry. Items list a value or `null` for every parameter in use, and the values closed items keep of archived parameters. Writes lock the heap, then the parameter, and read the parameter, its options and the item values they check with locking reads, for the MySQL snapshot reason given under [Rules that depend on state](#rules-that-depend-on-state). Item writes read the parameters they check values against the same way.

A display name change is audited once in each of the user's teams, since the audit log is kept per team and that is where teammates see the name. A user in no team has nowhere to record it.

## Authentication and authorization

1. **Web app:** cookie session (`HttpOnly`, `Secure`, `SameSite=Lax`). The cookie holds a random 256-bit token; the database stores only its SHA-256, so a leaked database can't be used to sign in. Every state-changing request, sign-in included, must carry an `X-CSRF-Protection` header. Browsers only let same-origin scripts set custom headers (cross-origin ones need a CORS preflight, which the server never approves), so the header together with `SameSite=Lax` blocks cross-site request forgery without a per-session token.
2. **Non-web clients:** the OAuth 2.0 authorization code flow with PKCE (S256), against the server's own endpoints (RFC 6749, RFC 7636, RFC 7009 for revoking).
   - The CLI opens the browser on the web app's `/oauth/authorize` page and listens on a loopback redirect URI.
   - The user signs in using the normal web flow, if they aren't already, and returns to the page, which asks whether the client may use their account. The page checks the request with `GET /api/v1/oauth/authorize` before it shows anything, so it never offers, or redirects to, a request the server would refuse. Allow approves it with `POST /api/v1/oauth/authorize` (session cookie and CSRF header; a request with a bearer token is refused, so a client can't approve more access for itself) and sends the browser to the `redirect_to` it gets back, `redirect_uri?code=…&state=…`. Cancel sends it to `redirect_uri?error=access_denied&state=…`, which the page builds itself, as the server has checked the redirect URI by then.
   - The CLI exchanges the code at `POST /api/v1/oauth/token` (`grant_type=authorization_code`, with `code_verifier`, `redirect_uri` and `client_id`) for an access token and a refresh token. Token responses carry `Cache-Control: no-store`.
   - Only registered first-party public clients are allowed. The registry is a constant in `domain::oauth`, not a table: one client today, `upheap-cli` ("Upheap CLI"). Its redirect URIs are loopback only, per RFC 8252: `http://127.0.0.1:<port><path>` and `http://[::1]:<port><path>` with any port. `localhost`, other schemes and hosts, user info, a query and a fragment are all refused, so adding `?code=…` always gives the URI that was approved.
   - Codes and tokens are random 256-bit values, stored only as SHA-256 hashes, as session tokens are. Codes last 10 minutes and work once, for the client, redirect URI and challenge they were issued for. Access tokens last 15 minutes and refresh tokens 30 days; see [Non-web client tokens](#non-web-client-tokens) for rotation.
   - `POST /oauth/token` and `POST /oauth/revoke` take form-encoded bodies and answer refusals in the RFC format (see [Conventions](#conventions)). They read no cookie, so they need no CSRF header.
   - **Bearer tokens:** `Authorization: Bearer <access token>` resolves to the same `Principal` a session cookie does, on every endpoint. A request whose `Authorization` header uses the `Bearer` scheme (compared ignoring case) is authenticated by it alone and any cookie is ignored, so it doesn't need `X-CSRF-Protection`: a cross-site page can't set that header either, and CSRF is a cookie problem. A malformed, empty, invalid, expired or revoked token is `401` `unauthenticated` with `WWW-Authenticate: Bearer error="invalid_token"`, with no fallback to the cookie. Other schemes, such as the `Basic` credentials a browser sends to a proxy with `auth_basic` (on cross-site requests too), are ignored: the cookie and the CSRF check apply as usual. `POST /auth/logout` ends only a session cookie; a client signs out with `POST /oauth/revoke`.
   - No page of the web app may be framed by another site: the server sends `X-Frame-Options: DENY` and `Content-Security-Policy: frame-ancestors 'none'` with every response of the web app, so a page can't hide the consent page under something that gets the user to click Allow (RFC 6749 §10.13).
   - All of it is part of the `accounts` feature; the local build has none of it.
3. **External identity providers:** generic OIDC, configured per deployment, for example Okta, Google or Keycloak. A first sign-in creates the user. An identity is matched by `(provider, subject)`, never by email alone.
4. **Local accounts:** passwords are hashed with argon2id, and sign-in takes an email address or a username, and attempts are rate-limited per account, keyed by its email address whichever was typed, or per unknown address or username (`domain::sign_in`).
   - An address gets five attempts per 15-minute window, opened by its first attempt. Later attempts get `429` `too_many_attempts`, with `Retry-After` in seconds, until the window ends. They are refused before any password is checked, so they cost no argon2 work.
   - Addresses without an account are counted the same way, so a refusal doesn't reveal whether an account exists. Malformed addresses can't have an account and aren't counted.
   - A successful sign-in clears the count. Refused attempts don't extend the window. Someone who knows an address can still keep its owner out, by using up its attempts at the start of every window; that is the price of a limit per address.
   - Counts live in the database, in `sign_in_attempts`, so they hold across instances. Each attempt is counted by one upsert inside a transaction, so concurrent attempts are each counted and each reads back its own count.
   - The table is keyed by the SHA-256 of the normalised address. That keeps addresses out of plain sight but not secret: anyone who can read the table can test a guessed address. Each attempt deletes a few rows whose window has ended, by key, so the table only holds addresses tried in the last few minutes.
   - Addresses are compared exactly on both databases (`utf8mb4_bin` on MySQL). Otherwise MySQL would find one account under many spellings, each with its own attempts.
   - There is no per-IP limit, because the server sits behind a proxy and doesn't know client addresses. Spraying one password across many accounts is left to the reverse proxy.
5. **Authorization:** the API resolves a cookie or a bearer token into one `Principal`. The service layer enforces access as described in [Access control](#access-control).

## Access control

### Permissions as code

1. The roles table in [offering.md](offering.md#roles-and-permissions) is implemented as one typed enum in `domain`, for example `Permission::EditItem`, `Permission::ConfigureHeap` and `Permission::ManageMembers`. Each permission also maps to a function that says whether a given role grants it. One permission can back more than one row of the table when the rows always grant the same roles: `EditItems` covers "Add and edit items", "Add and edit tags" and "Save, change and archive a heap's views". If those rows ever diverge, they get separate permissions.
2. Every service use case declares the permission it requires and checks it first, before it reads or writes anything else.
3. `allowed_actions` in responses comes from the same functions. The checks and what clients display therefore can't drift apart.
4. Handlers never check permissions. They pass the `Principal` to the service.

### Resource scoping

1. Every resource resolves to exactly one team: heap → team; item, tag, parameter, option and view → heap → team. The service resolves the owning team first, then loads the caller's membership in that team.
2. If the caller isn't a member of the owning team, the API returns `404 Not Found`, the same as for a resource that doesn't exist. An outsider can't probe for IDs.
3. If the caller is a member but lacks the permission (for example, a member trying to configure a heap), the API returns `403 Forbidden`.
4. Every repository query that loads heap-level data filters by the resolved `heap_id`, so an ID from a different heap can never leak in. For example, a `blocked_by_item_id` or a `tag_id` from another heap is rejected with `422` (`tag_not_in_heap` for tags).
5. `GET /users?query=` (for adding members) is for admins of a shared team; everyone is admin of their personal team, which doesn't count. It is still built so it can't list addresses: a name matches when one of its words starts with the query, an email matches only when the query has its whole local part, the `@` and at least one character of the domain, and a match shows the email only when the email matched. It needs at least 3 characters (`field_too_short`), or a whole username shorter than that, which then matches only that user, and returns at most 10, with IDs, usernames and display names.

### Archived heaps

1. An archived heap is read-only for everyone, admins included.
2. Every write use case on a heap and on its items, tags, parameters and views first checks that the heap is active. If it isn't, the API returns `409` with code `heap_archived`.
3. Unarchiving is the only write allowed on an archived heap, and only an admin can do it.
4. `allowed_actions` is empty for anything in an archived heap, apart from `unarchive` for admins.

### Rules that depend on state

These checks run in the same transaction as the write, after the permission check, with rows locked where needed. Each failure returns `409` or `422` with a stable code.

| Rule | Check | Error code |
| --- | --- | --- |
| A team keeps at least one admin | Every team write locks the team row before any other read, then reads the memberships, checks the caller's role and counts the remaining admins. On MySQL, a plain read before the lock would fix the transaction's snapshot and hide changes that committed while it waited. | `last_admin` |
| Reopen within one week | `now − closed_at ≤ 7 × 24h` | `reopen_window_expired` |
| Closed items are read-only until reopened | Editing anything other than state on a done or won't-do item is refused. | `item_closed` |
| No blocking cycles | Walk the blocker chain (see [Cycle detection](#cycle-detection)). | `blocking_cycle` |
| A blocker is an open item of the same heap, on a blocked item | Checked with the cycle walk. | `blocker_not_in_heap`, `blocker_closed`, `item_not_blocked` |
| Archiving an option requires a replacement | The replacement must be an active option of the same parameter. | `replacement_required` |
| A parameter is archived only when unused | No non-terminal item has a value for it, read with a locking read. | `parameter_in_use` |
| Archived parameters can't change | Checked on the parameter as read under its row lock. | `parameter_archived` |
| Parameter names are unique per heap | Among parameters in use, ignoring case, under the heap lock. | `parameter_name_taken` |
| Option labels are unique per parameter | Among options in use, ignoring case. | `option_label_taken` |
| Rearranged options are exactly those in use | Every option in use, once each. | `options_mismatch` |
| Tag names are unique per heap | Compared ignoring case, backed by a unique index on `(heap_id, name_key)`. | `tag_name_taken` |
| Archived saved views can't change | Checked on the view as read under its row lock. | `saved_view_archived` |
| Stale writes | `version` in `If-Match` must match. | `version_conflict` |

### Non-web client tokens

1. For now, access tokens issued to the CLI or other first-party clients carry the user's full permissions. There are no narrower scopes.
2. Access tokens are short-lived (15 minutes). Refresh tokens rotate on each use, and if an old one is reused, the whole token family is revoked.
   - A family is one sign-in by a client: the pair the code was exchanged for and every pair refreshed from it. Refreshing uses the refresh token up and issues a new pair in the same family; the new refresh token lasts 30 days from then, so a client in use stays signed in.
   - A refresh token presented a second time means someone holds a copy, so the family is revoked, the legitimate client included, which then signs in again. A code presented a second time within its 10 minutes revokes the family its first exchange issued, as RFC 6749 §4.1.2 asks. Past them it's only refused, so a code left in a browser's history can't end the sign-in later. When two exchanges of one code race on MySQL or Postgres, the loser reads the code again outside its transaction (on MySQL a read inside it would still see its old snapshot) and revokes the winner's family.
   - Single use is enforced by a conditional update (`used_at IS NULL`), so of two requests racing with one refresh token or code, one wins and the other counts as reuse. A refresh first locks its family's row (`FOR UPDATE` on MySQL and Postgres), and a revocation updates that same row, so the two run one after the other: a revocation that comes during a refresh waits for it and ends the new pair too, and a refresh after a revocation sees it. Tokens are never issued twice for one code or refresh token on any backend.
   - On SQLite, which has no row locks and lets one transaction write at a time, the request that loses such a race may fail with a server error (`SQLITE_BUSY`) rather than be refused as reuse; with one connection, as in-memory databases have, the two simply take turns. Starting these transactions with `BEGIN IMMEDIATE` would turn the error into a wait, but SeaORM has no way to ask for it per transaction, and the outcome that matters, no second pair, holds either way.
   - Revoking either token of a family (`POST /oauth/revoke`, as `upheap auth logout` will) revokes the family. It answers `200` for unknown tokens too (RFC 7009).
3. A user can list and revoke their own client tokens (`GET /me/tokens`, `POST /me/tokens/{id}/revoke`).
4. Scoped tokens (for example read-only, or a single heap) can be added later without changing the permission model, because a scope would only ever narrow the role's permissions.

## Audit log

1. Each service use case runs in one database transaction. It performs the change and inserts the audit entry before committing.
2. `action` is a typed Rust enum, for example `ItemCreated`, `ItemStateChanged`, `OptionArchived` or `MemberPromoted`. `diff` stores the changed fields as before/after pairs.
3. Each action is shown to those who hold the permission it takes (`audit_permission` in `domain::permission`), and the read filters on it in SQL so paging stays exact:
   - Members see item, tag and saved view changes, and display name changes.
   - Admins additionally see heap, parameter, team and membership changes. Leaving a team is a membership change, so only admins see it, although any member may leave.
4. `GET /teams/{team}/heaps/{heap}/audit` reads what is recorded against the heap. `GET /teams/{team}/audit` reads the team's own entries (`heap_id` is `NULL`) and the heaps' own entries (created, renamed, archived), but not what happens inside the heaps. With `entity_id`, either reads one entity's history; `idx_audit_log_entity` serves it.
5. Entries come ready to show: the actor's and the entity's current names, a time label in the request's timezone, a `phrase` to read after the actor's name ("edited item", or "joined the team" when the entry is about the actor, with `names_entity` false), and `changes`, one per changed field with its label and the values before and after as text. IDs in diffs (tags, options, parameters, blockers) are replaced by current names, due times become fixed dates, and an item's parameter values become one change per parameter. The raw `diff` is returned too. The service reads diffs by action: created entries hold the values set, ended memberships the role held, edits before/after pairs.
6. The web app shows the history where things are edited: under the Edit item view, in a collapsed History on each tag, parameter, heap and member (saved views, hidden for now, have none in the web app). Parameters and heaps offer `view_history` in `allowed_actions` only to those who may see their entries, so members aren't shown histories that are always empty. Archived saved views have no place of their own; their history is on the heap's Activity page. The heap's Activity page and the team's Activity page show the whole logs. Any mutation invalidates every loaded history.

## Configuration and deployment

1. Configuration comes from environment variables, optionally with a TOML file:
   - `DATABASE_URL` (`sqlite://…`, `mysql://…` or `postgres://…`)
   - `DATABASE_MAX_CONNECTIONS` (default `5`, small because several Cloud Run instances may share one database)
   - `BASE_URL` (session cookies are `Secure` unless it starts with `http://`)
   - `BIND_ADDR` (default `0.0.0.0:8080`)
   - `ALLOW_REGISTRATION` (default `true`)
   - `SESSION_TTL_DAYS` (default `30`)
   - `LOG_FORMAT` (`json` by default, or `pretty`)
   - a list of OIDC providers (not yet implemented)

   No session secret is needed, because session tokens are random and stored hashed rather than signed.
2. On startup the server runs pending migrations, then serves the API and the web app on one port. On Postgres the run holds an advisory lock, so instances starting together apply each migration once.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at Postgres or MySQL. TLS terminates at a reverse proxy, or at Cloud Run in front of a Neon database ([cloud-run-postgres.md](cloud-run-postgres.md)).
5. **Health checks:** `/healthz` returns `200` while the process is up. `/readyz` returns `200` only once migrations have finished and the database answers a ping, so orchestrators and proxies route traffic only to ready instances. Neither needs authentication, and neither reveals versions or configuration.
6. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server with a clear message before it listens. On SIGTERM it stops accepting connections and drains in-flight requests.
7. **Local build:** a static site with no configuration, published to GitHub Pages. It has one user and one team, which holds every heap, and its data lives in the browser. See [local-build.md](local-build.md).

## Testing

1. **Domain:**
   - Unit tests for each ranking factor, the state machine, and fuzzy due labels.
   - `proptest` invariants, for example:
     - ranking is deterministic;
     - an item blocking another never ranks below it;
     - editing one item under a fixed `as_of` doesn't change the relative order of the others.
2. **Access control:** a table-driven test runs every endpoint as a non-member, a member and an admin. It also runs every write endpoint against an archived heap. Expected status codes come from the permission table, so adding an endpoint without adding it to the table fails the test.
3. **Store and service:** the store and API suites (the API suite drives the services) run against in-memory SQLite, and again against MySQL and Postgres in CI. `TEST_DATABASE_URL` points them at a MySQL or Postgres server, and each test creates its own database there (`store::testing`). A service container rather than `testcontainers` keeps `cargo test` free of Docker and the tests free of container start-up.
4. **API:** the generated OpenAPI spec is checked into the repo, and CI fails if it changes without being regenerated.
5. **End to end:** Playwright covers the golden paths (sign up and sign in, add item, ranking, block and unblock, close and reopen, admin parameter management) in `web/e2e`. They drive Chromium against the real server binary with the web app embedded, on in-memory SQLite. Each test registers its own account, so tests share no data and run in parallel; anything beyond these paths is tested lower down. The same tests, without sign-up, run against the local build, with its own tests for export and import and the tab lock ([local-build.md](local-build.md)).

## Decisions

1. **Teammates' edits** move items within a session too, with animation (Ranking → Session-stable order).
2. **Timezone:** comes from the client device on each request. All instants are stored as UTC timestamps, and dates are stored as end of day in the user's timezone (Ranking → Time and timezones).
3. **Search:** text matching runs on the client for now (Ranking → Search).
4. **Size:** at most a few thousand open items per heap. Ranking runs in memory, and the list endpoint returns the whole heap.
5. **Formula:** designed separately. It plugs in behind `rank`.
