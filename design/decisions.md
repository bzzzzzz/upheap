# Decisions

Choices that could have gone another way, with the reasons, so they aren't reopened by accident or without the
context. [architecture.md](architecture.md) describes the mechanisms these choices led to.

One entry per decision, appended at the end, each with the **context**, the **decision**, what it was chosen
**instead of**, and the **consequences**. An entry isn't rewritten once made; when a later decision replaces it,
both say so.

## 1. A stable order comes from a frozen `as_of`, not from server state

**Context.** The list must not reorder on its own during a session (Req Priority §6), while an edited item should
move at once and teammates' edits should still show up.

**Decision.** The score is a deterministic function of item data and time, so the server returns an `as_of` with
the first list and the client echoes it on later requests. Edits move because their data changed, not because
time did; teammates' edits arrive on the next refetch, animated, like one's own.

**Instead of.** A server-side snapshot per session, or a list frozen until the user refreshes.

**Consequences.** No server state and nothing to expire; reloading drops `as_of` and re-ranks; the server
ignores an `as_of` more than about a day old; a single item's labels, computed at the current time, can read a day
ahead of the list's.

## 2. The timezone comes from the client on every request

**Context.** Due labels, "overdue", the closed list's groups and the time labels in histories depend on the
reader's timezone, and nothing is stored with one.

**Decision.** Every instant is stored and sent as UTC. Clients send the device's IANA timezone in a `Time-Zone`
header, and dates without a time are stored as the end of that day in the request's timezone.

**Instead of.** A timezone saved per user on the server.

**Consequences.** Every client, the CLI and the local build included, sends the header. The local build ships the
time zone database in its wasm. The reopen window is measured in hours from `closed_at`, so it's timezone-free.

## 3. The whole heap in one response, and text search on the client

**Context.** A heap is expected to hold at most a few thousand open items.

**Decision.** The ranked list endpoint returns every open item at once, and the client searches titles and
descriptions over what it holds. Filters and sorts stay on the server, because they decide what's pinned and
grouped, and ranking runs over every non-closed item so blockers inherit from hidden items too.

**Instead of.** Pagination and a server search endpoint from the start.

**Consequences.** No request per keystroke, and stable positions are trivial. If heaps outgrow the assumption,
search moves to the server behind a `q=` parameter with per-database full-text indexes. The size assumption is
written down in [architecture.md](architecture.md#computation).

## 4. The ranking formula is a version in code, not a setting per heap

**Context.** The formula was expected to change as the product found its shape (Req Priority §10).

**Decision.** `rank` takes a `FormulaVersion`; the server uses one version, set in code. Tests pin orderings per
version. `V2` replaced `V1` by adding need, both as a factor and as the rate items age at; items from before
count as expected, the step that leaves their age as it was.

**Instead of.** Storing a formula version per heap, or exposing the formula's shape to clients.

**Consequences.** A formula change is an explicit test update and needs no migration, because scores are never
stored. Switching from `V1` to `V2` kept most relative orders; blockers were the exception, since a blocker takes
on a boosted copy of a higher base score and can move above an item it was just below.

## 5. Need is required when an item is added, a deliberate API break

**Context.** Need was added after the API existed. API §3 of the principles says breaking changes are additive or
versioned.

**Decision.** `POST …/items` requires `need`. Existing items were migrated to `expected`.

**Instead of.** Defaulting a missing `need`.

**Consequences.** Any value the server picked would be a guess the user didn't make, and there were no outside
clients yet, so the break cost nothing. Release notes call it out.

## 6. Effort, impact and need are item columns, not custom parameters

**Context.** Custom parameters already model "an option from an ordered list, with a weight per heap".

**Decision.** Effort, impact and need are fixed columns on `items`, with their weights on `heaps` beside the
parameter weights. Effort and impact share one weight: they count together as value per effort.

**Instead of.** Three built-in parameters.

**Consequences.** They exist in every heap, can't be archived or renamed, and the ranking can combine effort and
impact in one factor. The heap row plus the parameter weights make up the whole ranking policy.

## 7. Usernames are the slugs of personal teams

**Context.** Usernames and team slugs both appear right after `/teams/` in URLs, so they must never collide.

**Decision.** Every user has a personal team whose slug is their username, linked by `teams.personal_user_id`.
One unique index on `teams.slug` covers both. The personal team takes no members, and its slug never changes.

**Instead of.** A separate `users.username` column with a cross-table uniqueness check.

**Consequences.** `/teams/eugene` is always Eugene's personal team. A personal team can't be shared, so anything
shared lives in another team. Handles in general sit beside the UUIDs rather than replacing them, so a slug can
change without touching foreign keys ([architecture.md](architecture.md#handles)).

## 8. Cross-site request forgery is blocked by a custom header, not per-session tokens

**Context.** The web app signs in with a cookie, so state-changing requests need protection against cross-site
forgery.

**Decision.** Every state-changing request carries an `X-CSRF-Protection` header, and the session cookie is
`SameSite=Lax`.

**Instead of.** A per-session CSRF token embedded in pages or responses.

**Consequences.** Browsers only let same-origin scripts set custom headers (cross-origin ones need a CORS
preflight the server never approves), so the header alone proves the request came from the app. There is nothing
to issue, store or rotate. Requests authenticated by a bearer token don't need the header, since CSRF is a cookie
problem.

## 9. The OAuth client registry is a constant, and redirect URIs are loopback only

**Context.** The CLI signs in through the browser with the authorization code flow and PKCE.

**Decision.** Only registered first-party public clients are allowed; the registry is a constant in `domain::oauth`
with one client, `upheap-cli`. Redirect URIs must be `http://127.0.0.1` or `http://[::1]` on any port (RFC 8252).
A request that's wrong in some other way than its client or redirect URI is shown as an error in the browser
rather than redirected back to the client.

**Instead of.** Client registration in the database, `localhost` and custom-scheme redirects, and RFC 6749's
`redirect_uri?error=invalid_request` for malformed requests.

**Consequences.** No client management UI or table. A second client is a code change. The only client is
first-party, so a malformed request is a bug the person at the browser should see; the CLI gives up waiting after
a few minutes.

## 10. Refresh tokens rotate in families, and reuse revokes the family

**Context.** CLI access tokens should be short-lived, and a stolen refresh token must not grant access for long.

**Decision.** Each sign-in by a client is a family. Every refresh uses the token up and issues a new pair in the
family; a refresh token or code presented a second time revokes the whole family, the legitimate client included.
Access tokens last 15 minutes, refresh tokens 30 days from their issue.

**Instead of.** Long-lived access tokens, or refresh tokens that stay valid after use.

**Consequences.** A client in use stays signed in indefinitely; a copied token ends the sign-in for everyone,
which the real user notices and repairs by signing in again. Revocation and refresh serialise on the family row.
Tokens carry the user's full permissions; scopes can be added later since they would only narrow a role.

## 11. Sign-in attempts are limited per address, not per IP

**Context.** Password guessing must be slowed, and the server sits behind a proxy, so it doesn't know client
addresses.

**Decision.** Five attempts per address per 15-minute window, counted in the database so the limit holds across
instances, with unknown addresses counted the same way so a refusal reveals nothing.

**Instead of.** A per-IP limit in the server, or a limit only in the proxy.

**Consequences.** Someone who knows an address can keep its owner out by using up its attempts each window; that
is the price of a limit per address. Spraying one password across many accounts is left to the reverse proxy.
Attempts are refused before any password is hashed, so a flood costs no argon2 work.

## 12. Portable SQL over per-database features

**Context.** The product runs on SQLite, MySQL and Postgres, and the local build runs SQLite in the browser.

**Decision.** No JSON column types, database-specific full-text search, triggers or stored procedures. IDs are
`CHAR(36)`, timestamps `BIGINT` milliseconds, structured data JSON in `TEXT` that is never queried, and
case-insensitive uniqueness a lowercased key column, not a collation.

**Instead of.** Native UUID, timestamp and JSON types, and each database's own search.

**Consequences.** Every query works everywhere, and CI runs the suites on all three. The Postgres port needed only
small fixes, listed under "Portable SQL in practice" in [architecture.md](architecture.md#data-model). Full-text
search, if ever needed, is the one place a per-database path is planned.

## 13. Migrations run at startup, with an advisory lock on Postgres

**Context.** The server is one binary, and on Cloud Run several instances may start at once.

**Decision.** The server runs pending migrations before it listens, and `/readyz` answers only after they finish.
On Postgres the run holds a transaction-scoped advisory lock, so instances starting together apply each migration
once.

**Instead of.** A separate migrate step in the deploy, or an init container.

**Consequences.** Deploying is one image and one command. The lock is transaction-scoped because Neon's pooler
runs in transaction mode, where a session lock could be taken and released on different connections. MySQL and
SQLite have no such lock and aren't run with several instances.

## 14. Database suites run on service containers, with a database per test

**Context.** The store and API suites must run on MySQL and Postgres as well as SQLite.

**Decision.** CI starts MySQL and Postgres as service containers; `TEST_DATABASE_URL` points the suites at a server,
and each test creates its own database there.

**Instead of.** testcontainers starting a database per run from inside `cargo test`.

**Consequences.** `cargo test` needs no Docker and no container start-up; the same suites run locally against any
server the developer points them at. Tests share no data and run in parallel.

## 15. Every handle follows a fixed word in the URL, and short links are expanded by the client

**Context.** Handles replaced UUIDs in URLs and API paths. A readable URL invites root-level names (`/eugene`),
which would collide with the app's own pages and the server's paths.

**Decision.** Every handle sits after a word saying what it is (`/teams/{team}/heaps/{heap}/items/{item}`), and
API paths have the same shape under `/api/v1`. Short links (`/h/team/heap/key`) carry handles alone, one per
position, and the web app's router expands them as they open.

**Instead of.** Root-level usernames and reserved-name lists, or a server redirect for short links.

**Consequences.** No name is reserved and new pages can be added anywhere. A web URL becomes its API call by adding
the prefix. Short links work in the local build, which has no server to redirect. The API has no short form.

## 16. Item keys are five random base32 characters

**Context.** Items need a handle people can read aloud, type into a terminal and paste into chat.

**Decision.** Five lowercase characters from Crockford's base32 alphabet (no `i`, `l`, `o`, `u`) with at least one
digit, drawn at random inside the insert transaction and retried on a clash with the heap's unique index.

**Instead of.** Sequential numbers per heap, as issue trackers use, or UUIDs in URLs.

**Consequences.** A key says nothing about an item's age or the heap's size. About 30 million keys per heap; a
clash is rare and a retry settles it. Keys are fixed for life, since items are never deleted. The service draws the
random bytes from an injected `RandomSource`, so tests can script keys.

## 17. The local build is the same backend compiled to WebAssembly, with Cargo features for what it lacks

**Context.** A static, single-user version of Upheap was wanted for trying it out and for personal use, without a
server.

**Decision.** The backend crates compile to wasm and run in a Web Worker; the web app swaps only its `fetch`. What
only a shared server needs (`accounts`) and what the browser can't run (`trace-and-timeout`) are Cargo features
the local build leaves out.

**Instead of.** A second implementation in JavaScript, or a runtime "local mode" flag in one build.

**Consequences.** Ranking, validation, permissions and the API are one codebase, and the native tests cover the
local user too. Nothing left out can reach the wasm, which keeps it inside its size budget. A feature added to the
server must be placed behind `accounts` when the local build can't offer it.

## 18. SQLite in the browser through SeaORM's proxy connection, on `opfs-sahpool`

**Context.** The local build needs a database in the browser that runs the same migrations and SQL as the server.
sqlx doesn't build for wasm, and GitHub Pages can't send the COOP and COEP headers that the main OPFS VFS needs.

**Decision.** The official SQLite-wasm build with the `opfs-sahpool` VFS, in a worker, reached through SeaORM's
`ProxyDatabaseTrait` with the connection opened as SQLite so SeaORM emits the same SQL as on the server.

**Instead of.** IndexedDB or a JavaScript store with its own schema, or sqlx on wasm.

**Consequences.** One connection per database, so the worker holds a Web Lock and the app runs in one tab at a
time. The proxy types result columns from the entity metadata, since SQLite declares every integer alike. Released
migrations reach users' browsers directly and can never be edited.

## 19. Export and import are worker messages, not API endpoints

**Context.** The local build's data lives in one browser, which can clear it, so users need a copy and a way to
restore it.

**Decision.** The worker takes `export`, `import` and `count` messages beside the HTTP requests, in the same queue.
The web app reaches them through a build-time import (`#local-database`) that resolves to nothing in the server
build. The date of the last download is kept in `localStorage` and worded by the web app itself.

**Instead of.** `/api/v1/export` and `/import` endpoints, or storing the date on the server.

**Consequences.** The server build holds none of this code and the API stays what a server can do. Whether the
feature exists is a fact about the build, not a permission, so it bypasses `allowed_actions`. The storage note is
the one exception to thin clients (Thin clients §4 of the principles).

## 20. Production runs on Cloud Run with Neon Postgres

**Context.** A small deployment should cost close to nothing when idle. Cloud Run has no persistent disk, so SQLite
can't hold production data there, and most serverless databases that scale to zero speak Postgres.

**Decision.** Cloud Run for the app and Neon Postgres, through its pooled endpoint, for the database. Postgres
joined the supported databases and the CI matrix.

**Instead of.** A machine with SQLite or MySQL on a disk, or a serverless MySQL such as TiDB or Aiven.

**Consequences.** An external database and a small pool per instance; TLS at Cloud Run with no reverse proxy; no
background work between requests, so any timer would have to be Cloud Scheduler calling an endpoint; cold starts of
about a second. Neon's pooler runs in transaction mode, which is why the migration lock is transaction-scoped
(decision 13). Running the suites on Postgres found four portability fixes, listed in
[architecture.md](architecture.md#data-model). The setup is in [operations.md](operations.md).

## 21. Deploys sign in through Workload Identity Federation

**Context.** The deploy workflow needs Google Cloud credentials.

**Decision.** GitHub's OIDC token is exchanged for a deploy service account that only `deploy.yml`, run from `main`
of this repository (matched by numeric id), can assume. The deploy job's actions are pinned to commits.

**Instead of.** A service-account key in a repository secret.

**Consequences.** No key to leak or rotate. A renamed or recreated repository can't deploy. The one-time setup is a
script in [operations.md](operations.md#one-time-setup).
