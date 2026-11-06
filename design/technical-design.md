# Technical design

Draft. Companion to [requirements.md](requirements.md) and [offering.md](offering.md). Coding rules are in [engineering-principles.md](engineering-principles.md).

## Principles

1. **Thin clients.** The API server does all computation: ranking, score contributors, filtering, sorting, fuzzy due labels, permission checks, and state rules such as whether an item can still be reopened. Clients render what the API returns and send user intent back. Clients do two things themselves: text search over the list they already have (see [Search](#search)), and animations. Ranking, permission and state-machine logic exist only on the server, so a new client (CLI, native app) needs nothing beyond an HTTP client and the generated API types.
2. **The server tells clients what is allowed.** Responses include the actions the current user can take (for example `can_reopen` or `can_edit_parameters`), so clients don't re-implement role or state rules to decide which controls to show.
3. **Database-agnostic.** Only portable SQL features. No JSON column types, database-specific full-text search, triggers or stored procedures. Everything is tested against both SQLite and MySQL.
4. **Nothing is deleted, and everything is audited.** There are no `DELETE` statements for domain data. Every write and its audit entry are committed in the same transaction.
5. **Single binary.** One statically linked executable serves the API and the embedded web app.

## Stack

| Area | Choice |
| --- | --- |
| Language (backend) | Rust (stable), Cargo workspace |
| HTTP | axum on tokio, tower middleware |
| API contract | Code-first OpenAPI 3.1 via utoipa; TypeScript types for the web app generated with openapi-typescript |
| Data access | SeaORM (on sqlx), runtime-selected backend: SQLite or MySQL (Postgres later) |
| Migrations | sea-orm-migration, embedded in the binary, run on startup |
| Auth | `openidconnect` + `oauth2` crates (external IdPs), `argon2` (local passwords), own session table with hashed random tokens (web sessions), own minimal OAuth authorization-code + PKCE endpoints (non-web clients) |
| Web app | React + Vite + TanStack Query + TanStack Router, Tailwind, Motion (layout animations); embedded with `rust-embed` |
| Observability | `tracing` with JSON logs, `/healthz` and `/readyz` |
| Testing | `cargo test`, `proptest` for ranking invariants, `testcontainers` for MySQL, Playwright for end to end |
| Packaging | Static musl binary; minimal Docker image; example docker-compose with MySQL |

## Workspace layout

```
crates/
  domain/    Pure logic, no IO: types, state machine, ranking, fuzzy due labels, validation
  store/     SeaORM entities, migrations, repositories
  service/   Use cases: authorization, transactions, audit logging, orchestration
  api/       axum routes, request/response DTOs, OpenAPI, auth flows, sessions
  server/    Binary: config, startup, migrations, embedded web assets
web/         React app (TypeScript), consumes generated API types only
```

Dependencies point one way: `server → api → service → store, domain`. `domain` depends on nothing in the workspace, so the ranking logic can be unit- and property-tested in isolation.

### Errors

1. Library crates (`domain`, `store`, `service`) define typed error enums with `thiserror`. Callers can match on them, and each variant maps to one stable error `code`.
2. `api` converts service errors to `application/problem+json` responses in exactly one place, an `IntoResponse` implementation. No handler builds an error response by hand.
3. `anyhow` is used only in `server`, for startup and configuration failures that end the process, and in tests.

## Data model

Conventions:

- **IDs** are UUIDv7, stored as `CHAR(36)`. They are sortable by creation time and safe to expose.
- **Timestamps** are UTC Unix milliseconds in `BIGINT`. This avoids differences between the SQLite and MySQL date types. No timezone is stored anywhere (see [Time and timezones](#time-and-timezones)).
- **Structured data** (audit diffs, saved view filters) is serialized JSON in `TEXT` and never queried inside the database.
- **Archiving** uses a nullable `archived_at`. There are no hard deletes.
- **Foreign keys** never cascade. They use `ON DELETE RESTRICT`, so even an accidental `DELETE` fails instead of removing related rows.
- **Optimistic concurrency:** mutable rows carry a `version` integer. Updates require the expected version and fail with `409 Conflict` otherwise.
- **State and `closed_at`:** in the database, `closed_at` is set exactly when `state` is done or won't do. In `domain`, the state is an enum where only the closed variants carry `closed_at`, so an open item with a close time can't be built. The store converts between the two and rejects rows that break the rule.

Tables:

| Table | Key columns |
| --- | --- |
| `users` | id, display_name, email, created_at |
| `password_credentials` | user_id, argon2 hash, updated_at |
| `identities` | id, user_id, provider, subject (unique per provider) |
| `sessions` | token_hash (SHA-256 of a random 256-bit token), user_id, created_at, expires_at |
| `oauth_auth_codes` | code hash, user_id, client_id, PKCE challenge, redirect_uri, expires_at |
| `oauth_tokens` | token hash, user_id, client_id, kind (access/refresh), expires_at, revoked_at |
| `teams` | id, name, created_at, version |
| `team_members` | team_id, user_id, role (admin/member), joined_at |
| `heaps` | id, team_id, name, age_weight, deadline_weight, value_weight, archived_at, version |
| `items` | id, heap_id, title, description, state, effort, impact, due_at, due_firm, blocked_by_item_id, created_at, closed_at, version |
| `tags` | id, heap_id, name, name_key (lowercased name, unique per heap), description, created_at, version |
| `item_tags` | item_id, tag_id, added_at, removed_at (set when the tag is taken off; adding it again clears it) |
| `parameters` | id, heap_id, name, weight, archived_at, version |
| `parameter_options` | id, parameter_id, label, position, archived_at, replaced_by_option_id |
| `parameter_tags` | parameter_id, tag_id |
| `item_parameter_values` | item_id, parameter_id, option_id |
| `saved_views` | id, heap_id, name, filter (JSON text), created_by, version |
| `audit_log` | id, team_id, heap_id (nullable), actor_user_id, action, entity_type, entity_id, diff (JSON text), created_at |

Notes:

- Effort and impact are columns on `items` rather than custom parameters. They are fixed, apply to every heap, and share one weight (`heaps.value_weight`).
- The age and deadline weights live on `heaps`, next to `value_weight`. Together with the parameter weights, they make up the heap's ranking policy.
- A team always has at least one admin. The service layer enforces this inside the transaction that changes membership.
- The application can only insert into `audit_log`. There is no update or delete code path for it.
- Tag names are unique per heap ignoring case. The check runs on `name_key` rather than the column collation, because MySQL compares case-insensitively by default and SQLite doesn't.
- Taking a tag off an item sets `item_tags.removed_at` instead of deleting the row, so relationship rows follow the no-`DELETE` rule too. The change is audited as an `item_updated` entry whose diff lists the tag IDs before and after. Team membership is the one exception (see [Resources](#resources-initial-set)).

## Ranking

### Computation

Ranking is a pure function in `domain`:

```rust
fn rank(formula: FormulaVersion, policy: &HeapPolicy, items: &[ItemForRanking], as_of: Timestamp) -> Vec<RankedItem>
// RankedItem { item_id, position, contributors: Vec<Contributor> }
```

1. The service loads the heap's policy, all non-closed items, their parameter values, and the blocking edges, then calls `rank`. Heaps are expected to hold at most a few thousand open items, so ranking them in memory on each request is cheap. A per-heap cache invalidated on write can be added if measurements call for it.
2. Each factor (age, deadline proximity, value per effort, each custom parameter) produces a weighted component. The final score is a combination of these. The exact formula is still open and lives behind one function, so changing it doesn't affect the API.
   - **Formula versioning:** each formula is a named version (`FormulaVersion::V1`, …). The server uses one version at a time, set in code, not stored per heap. Tests pin expected orderings per version, so a formula change shows up as an explicit test update. Because scores are never stored, switching versions needs no migration.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are rejected on write. Each item's effective score is the maximum of its own score and the effective scores of the items it blocks, times a boost (1.1 in `V1`). This is computed with a memoized walk over the graph, so it is linear in the number of items. Equal scores go to the item that blocks the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
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

### Search

1. The ranked list endpoint returns every open item in the heap in one response, with no pagination. This is fine at the expected size of a few thousand items.
2. The client does text search itself, over the titles and descriptions it already has. It keeps the matches in their ranked order and filters as the user types, without a request per keystroke.
3. Filters by state, tag and parameter value stay on the server, because they change which items are pinned and how they are grouped.
4. If heaps grow beyond this size, search moves to the server with per-database full-text indexes. Clients would then call `GET /heaps/{id}/items?q=`.

### Time and timezones

1. All instants are stored and sent as UTC timestamps. The server stores no timezone, for users or anywhere else.
2. Clients send the device's IANA timezone on every request, in a `Time-Zone` header such as `Europe/Berlin`. The browser reads it from `Intl.DateTimeFormat().resolvedOptions().timeZone`. If the header is missing, the server uses UTC.
3. **Dates without a time,** such as a due date, are stored as the end of that day in the user's timezone at the time of entry, then converted to a UTC timestamp. For example, 12 March entered in Berlin is stored as 12 March 22:59:59.999 UTC.
4. The server uses the request's timezone to compute presentation values, such as fuzzy due labels, formatted dates and "overdue".
5. The one-week reopen window is measured as exactly 7 × 24 hours from `closed_at`, so it doesn't depend on any timezone.

### Cycle detection

Setting `blocked_by_item_id` walks the blocker chain inside the write transaction. If the walk reaches the item being blocked, the API returns `422` with a `blocking_cycle` error.

1. Every item and tag write first locks the heap row (`SELECT … FOR UPDATE` on MySQL), before touching any other row, so writes in one heap run one after the other and always take locks in the same order. The walk then reads each link with a locking read, so of two writes that would close a cycle between them, the second sees the first. SQLite allows one writer at a time, so the second of two such writes fails instead.
2. Only a blocked item has a blocker: the blocker is part of the blocked state, and leaving that state clears it. Sending a blocker for an item that isn't blocked, and isn't moved to blocked in the same request, returns `422` `item_not_blocked`.
3. The blocker must be an item of the same heap (`blocker_not_in_heap`, which also covers unknown IDs) that isn't closed (`blocker_closed`). Closing a blocker later doesn't unblock what waits for it; the blocked item shows the closed blocker until someone moves it on.
4. Ranking still tolerates a cycle in stored data, by ignoring the edge that closes it, so one bad row can't break the list.

## API

### Conventions

1. All endpoints are under `/api/v1`, JSON over HTTPS.
2. Errors use `application/problem+json` with a stable `code` field.
3. Updates are `PATCH` requests with partial bodies. The expected `version` goes in an `If-Match` header.
4. List endpoints paginate with a cursor, except the ranked list, which returns every open item in one response (see [Search](#search)).
5. Responses carry presentation-ready derived data, so clients don't compute anything:
   - `due`: `{ at, firm, label, overdue }`, where `label` is the fuzzy text ("in a few weeks") for loose dates or a formatted date for exact ones, in the timezone from the request's `Time-Zone` header.
   - `contributors`: a list of labelled factors.
   - `allowed_actions`: for example `["edit", "reopen", "set_blocked"]`.
   - `unset` markers for parameters with no value.

### Resources (initial set)

| Area | Endpoints |
| --- | --- |
| Auth | `POST /auth/register`, `POST /auth/login`, `POST /auth/logout`, `GET /auth/oidc/{provider}/start`, `GET /auth/oidc/{provider}/callback`, `GET /oauth/authorize`, `POST /oauth/token`, `POST /oauth/revoke` |
| Me | `GET /me`, `PATCH /me` (display name), `GET /me/tokens`, `POST /me/tokens/{id}/revoke` |
| Teams | `GET /teams`, `POST /teams`, `GET /teams/{id}`, `PATCH /teams/{id}`, `GET/POST /teams/{id}/members`, `PATCH/DELETE /teams/{id}/members/{user_id}` (role change, removal), `POST /teams/{id}/leave` |
| Users | `GET /users?query=` (lookup for adding members, admins only) |
| Heaps | `GET /teams/{id}/heaps`, `POST /teams/{id}/heaps`, `GET/PATCH /heaps/{id}`, `POST /heaps/{id}/archive`, `POST /heaps/{id}/unarchive` |
| Ranked list | `GET /heaps/{id}/items?as_of=&state=&tag=&param=&sort=` returns pinned items, all ranked items, and the `as_of` value |
| Closed items | `GET /heaps/{id}/items/closed?cursor=` |
| Items | `POST /heaps/{id}/items`, `GET/PATCH /items/{id}`, `POST /items/{id}/reopen` |
| Tags | `GET/POST /heaps/{id}/tags`, `PATCH /tags/{id}` |
| Parameters | `GET/POST /heaps/{id}/parameters`, `PATCH /parameters/{id}`, `POST /parameters/{id}/archive`, `POST/PATCH /parameters/{id}/options`, `POST /options/{id}/archive` (with replacement), `PUT /parameters/{id}/tags` |
| Saved views | `GET/POST /heaps/{id}/views`, `PATCH /views/{id}` |
| Audit | `GET /heaps/{id}/audit`, `GET /teams/{id}/audit` (filtered by what the caller may do) |

The `DELETE` on a team member removes the membership. That is the one place where a relationship row goes away, and it is audited. The membership history stays in the audit log.

## Authentication and authorization

1. **Web app:** cookie session (`HttpOnly`, `Secure`, `SameSite=Lax`). The cookie holds a random 256-bit token; the database stores only its SHA-256, so a leaked database can't be used to sign in. Every state-changing request, sign-in included, must carry an `X-CSRF-Protection` header. Browsers only let same-origin scripts set custom headers (cross-origin ones need a CORS preflight, which the server never approves), so the header together with `SameSite=Lax` blocks cross-site request forgery without a per-session token.
2. **Non-web clients:** the OAuth 2.0 authorization code flow with PKCE (S256), against the server's own `/oauth/authorize` and `/oauth/token`.
   - The CLI opens the browser and listens on a loopback redirect URI.
   - The user signs in using the normal web flow.
   - The CLI receives a short-lived access token and a rotating refresh token.
   - Only registered first-party public clients are allowed.
3. **External identity providers:** generic OIDC, configured per deployment, for example Okta, Google or Keycloak. A first sign-in creates the user. An identity is matched by `(provider, subject)`, never by email alone.
4. **Local accounts:** passwords are hashed with argon2id, and login attempts are rate-limited.
5. **Authorization:** the API resolves a cookie or a bearer token into one `Principal`. The service layer enforces access as described in [Access control](#access-control).

## Access control

### Permissions as code

1. The roles table in [offering.md](offering.md#roles-and-permissions) is implemented as one typed enum in `domain`, for example `Permission::EditItem`, `Permission::ConfigureHeap` and `Permission::ManageMembers`. Each permission also maps to a function that says whether a given role grants it. One permission can back more than one row of the table when the rows always grant the same roles: `EditItems` covers both "Add and edit items" and "Add and edit tags". If those rows ever diverge, they get separate permissions.
2. Every service use case declares the permission it requires and checks it first, before it reads or writes anything else.
3. `allowed_actions` in responses comes from the same functions. The checks and what clients display therefore can't drift apart.
4. Handlers never check permissions. They pass the `Principal` to the service.

### Resource scoping

1. Every resource resolves to exactly one team: heap → team; item, tag, parameter, option and view → heap → team. The service resolves the owning team first, then loads the caller's membership in that team.
2. If the caller isn't a member of the owning team, the API returns `404 Not Found`, the same as for a resource that doesn't exist. An outsider can't probe for IDs.
3. If the caller is a member but lacks the permission (for example, a member trying to configure a heap), the API returns `403 Forbidden`.
4. Every repository query that loads heap-level data filters by the resolved `heap_id`, so an ID from a different heap can never leak in. For example, a `blocked_by_item_id` or a `tag_id` from another heap is rejected with `422` (`tag_not_in_heap` for tags).
5. `GET /users?query=` (for adding members) is admin-only, returns only display name and email, and needs at least 3 characters to match.

### Archived heaps

1. An archived heap is read-only for everyone, admins included.
2. Every write use case on a heap and on its items, tags, parameters and views first checks that the heap is active. If it isn't, the API returns `409` with code `heap_archived`.
3. Unarchiving is the only write allowed on an archived heap, and only an admin can do it.
4. `allowed_actions` is empty for anything in an archived heap, apart from `unarchive` for admins.

### Rules that depend on state

These checks run in the same transaction as the write, after the permission check, with rows locked where needed. Each failure returns `409` or `422` with a stable code.

| Rule | Check | Error code |
| --- | --- | --- |
| A team keeps at least one admin | Leaving, demotion and removal count the remaining admins inside the transaction. | `last_admin` |
| Reopen within one week | `now − closed_at ≤ 7 × 24h` | `reopen_window_expired` |
| Closed items are read-only until reopened | Editing anything other than state on a done or won't-do item is refused. | `item_closed` |
| No blocking cycles | Walk the blocker chain (see [Cycle detection](#cycle-detection)). | `blocking_cycle` |
| A blocker is an open item of the same heap, on a blocked item | Checked with the cycle walk. | `blocker_not_in_heap`, `blocker_closed`, `item_not_blocked` |
| Archiving an option requires a replacement | The replacement must be an active option of the same parameter. | `replacement_required` |
| A parameter is archived only when unused | No non-terminal item has a value for it. | `parameter_in_use` |
| Tag names are unique per heap | Compared ignoring case, backed by a unique index on `(heap_id, name_key)`. | `tag_name_taken` |
| Stale writes | `version` in `If-Match` must match. | `version_conflict` |

### Non-web client tokens

1. For now, access tokens issued to the CLI or other first-party clients carry the user's full permissions. There are no narrower scopes.
2. Access tokens are short-lived (15 minutes). Refresh tokens rotate on each use, and if an old one is reused, the whole token family is revoked.
3. A user can list and revoke their own client tokens (`GET /me/tokens`, `POST /me/tokens/{id}/revoke`).
4. Scoped tokens (for example read-only, or a single heap) can be added later without changing the permission model, because a scope would only ever narrow the role's permissions.

## Audit log

1. Each service use case runs in one database transaction. It performs the change and inserts the audit entry before committing.
2. `action` is a typed Rust enum, for example `ItemCreated`, `ItemStateChanged`, `OptionArchived` or `MemberPromoted`. `diff` stores the changed fields as before/after pairs.
3. Each read endpoint maps to the set of actions the caller can perform:
   - Members see item and tag events on heap pages.
   - Admins additionally see membership and configuration events.

## Configuration and deployment

1. Configuration comes from environment variables, optionally with a TOML file:
   - `DATABASE_URL` (`sqlite://…` or `mysql://…`)
   - `BASE_URL` (session cookies are `Secure` unless it starts with `http://`)
   - `BIND_ADDR` (default `0.0.0.0:8080`)
   - `ALLOW_REGISTRATION` (default `true`)
   - `SESSION_TTL_DAYS` (default `30`)
   - `LOG_FORMAT` (`json` by default, or `pretty`)
   - a list of OIDC providers (not yet implemented)

   No session secret is needed, because session tokens are random and stored hashed rather than signed.
2. On startup the server runs pending migrations, then serves the API and the web app on one port.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at MySQL. TLS terminates at a reverse proxy.
5. **Health checks:** `/healthz` returns `200` while the process is up. `/readyz` returns `200` only once migrations have finished and the database answers a ping, so orchestrators and proxies route traffic only to ready instances. Neither needs authentication, and neither reveals versions or configuration.
6. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server with a clear message before it listens. On SIGTERM it stops accepting connections and drains in-flight requests.

## Testing

1. **Domain:**
   - Unit tests for each ranking factor, the state machine, and fuzzy due labels.
   - `proptest` invariants, for example:
     - ranking is deterministic;
     - an item blocking another never ranks below it;
     - editing one item under a fixed `as_of` doesn't change the relative order of the others.
2. **Access control:** a table-driven test runs every endpoint as a non-member, a member and an admin. It also runs every write endpoint against an archived heap. Expected status codes come from the permission table, so adding an endpoint without adding it to the table fails the test.
3. **Store and service:** the same test suite runs against SQLite (in-memory) and MySQL (testcontainers) in CI.
4. **API:** the generated OpenAPI spec is checked into the repo, and CI fails if it changes without being regenerated.
5. **End to end:** Playwright covers the golden paths (add item, ranking, block and unblock, close and reopen, admin parameter management).

## Decisions

1. **Teammates' edits** move items within a session too, with animation (Ranking → Session-stable order).
2. **Timezone:** comes from the client device on each request. All instants are stored as UTC timestamps, and dates are stored as end of day in the user's timezone (Ranking → Time and timezones).
3. **Search:** text matching runs on the client for now (Ranking → Search).
4. **Size:** at most a few thousand open items per heap. Ranking runs in memory, and the list endpoint returns the whole heap.
5. **Formula:** designed separately. It plugs in behind `rank`.
