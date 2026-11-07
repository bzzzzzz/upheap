# Local build

Draft. A change spec on top of [technical-design.md](technical-design.md). Status lives in
[progress.md](progress.md).

## Goal

A second way to ship Upheap: a static site, published on GitHub Pages, that runs the whole app in one browser for
one person and keeps the data in that browser. Nothing to install, no server, no account.

The self-hosted server stays the product for teams. The local build is for trying Upheap out and for people who only
need a personal heap.

### Non-goals

1. Sync between devices or browsers, and sharing with other people.
2. Signing in. There is one user, and opening the page is enough.
3. A second implementation of anything. Ranking, validation, permissions and the API stay in Rust and are the same
   code in both builds.

## Approach

The backend is compiled to WebAssembly and runs in a Web Worker. The web app talks to it through the same typed
client it uses today, so it can't tell the two builds apart, apart from the actions the API offers.

```
React app ── openapi-fetch ── worker fetch ─▶ Web Worker
                                               ├─ wasm: api Router → service → store (SeaORM)
                                               └─ SQLite-wasm, database file in OPFS
```

1. **New crate `crates/local`.** The counterpart of `server`. Through `wasm-bindgen` it exposes `Backend`:
   `Backend.open()` builds the clock and the database connection, runs the migrations, opens the local user and
   builds the `api` Router, and `backend.handle(method, uri, headers, body)` answers one request. Dependencies
   still point one way: `local → api → service → store, domain`.
2. **Requests go through the Router.** `handle` turns its arguments into an `http::Request`, calls the axum Router
   as a tower `Service`, and returns the status, headers and body. Routes, DTOs, error mapping, `If-Match` and
   problem+json all stay as they are.
3. **The web app swaps `fetch`, nothing else.** openapi-fetch takes a custom `fetch`. The API client imports it as
   `#api-fetch`, which package.json `imports` resolves to none (the browser's) or, under the `local-build`
   condition that `vite --mode local-build` sets, to `src/local/fetch.ts`. That one posts each request to the
   worker (`src/local/worker.ts`) through `src/local/backendWorker.ts`, which starts it on first use. TanStack Query, the routes and the
   components don't change, and the server build holds neither the worker nor the wasm.
4. **One request at a time.** The worker queues requests and finishes one before starting the next. This gives the
   one-writer behaviour that SQLite already has on the server, and keeps transactions from interleaving when a
   handler awaits. A request the worker can't answer at all, such as one made when the backend failed to start,
   fails like a network error.

### Database

1. **SQLite in the browser:** the official SQLite WebAssembly build, `@sqlite.org/sqlite-wasm`, with its
   `opfs-sahpool` VFS. The database is a real SQLite file in the Origin Private File System. This VFS works without
   the COOP and COEP headers, which GitHub Pages can't set, but only inside a worker, which is where the backend runs
   anyway.
2. **SeaORM over a proxy connection.** sqlx doesn't build for wasm. SeaORM's `proxy` feature takes our own backend
   (`ProxyDatabaseTrait`), which gets the SQL and values and returns rows. `store`'s `proxy` feature turns it on, and `local`
   opens the connection with `DbBackend::Sqlite`, so SeaORM generates the same SQLite SQL as on the server. The proxy
   calls SQLite-wasm synchronously through a small JS shim in the same worker (`src/local/sqlite.ts`).
   - **Typing rows.** Proxy rows must hold exactly the value type each entity field reads, and SQLite declares
     `i32` and `i64` columns alike as `integer`. So `store::proxy` lists every column's type from the entities
     (SeaORM's migration table included), and the proxy types each result column by the table and column SQLite
     says it came from (`sqlite3_column_table_name`, `sqlite3_column_origin_name`, which also work for `RETURNING`).
     Expressions are typed from their value.
   - **Unique violations.** SeaORM recognises them only in sqlx errors. The proxy marks SQLite's unique and
     primary-key constraint codes, and `StoreError` maps the mark to `Duplicate`, so `email_taken`,
     `tag_name_taken` and the like still reach the client.
   - **Failed commits.** The proxy's `begin`, `commit` and `rollback` can't return errors. A failed `COMMIT` is
     rolled back and recorded, and the worker fails the request it belongs to with `500`.
3. **Same schema, same migrations.** The migrations run at worker start-up, like the server's run at boot. The
   portable-SQL rule doesn't change: this is SQLite, just another way of running it. Row locks (`FOR UPDATE`) are
   already left out of SQLite queries, and one request at a time gives the same guarantees.
4. **Migrations reach users' browsers directly.** A new version on Pages migrates each user's database the next time
   they open the page. The rule that released migrations are never edited matters even more here: we can't repair a
   database in someone's browser.

### Single-user mode

The local build has one user and one team. `Services::open_local_user` makes both on first start, with the first
heap, by the same service code that makes a personal team and heap on registration: the user "Me", the team "My
heaps" and the heap "My heap". Later starts find the same user. The API resolves every request to that user
(`api::Authentication::LocalUser`), and every heap belongs to that team.

| Area | Local build |
| --- | --- |
| Register, sign in, sign out, sessions, CSRF | Not built. Every request is the local user. |
| Sign-in rate limiting, argon2 | Not built. |
| OIDC, OAuth for the CLI | Not built. |
| Adding and removing members, `GET /users` | Not built. There is no one else to add. |
| Teams | One team, made on first start, holds every heap. No creating, leaving or renaming teams, and no Teams list. |
| Team page | Kept as the place to create, rename, archive and unarchive heaps. Its members section shows nothing to do. |
| Heaps, items, tags, blockers, parameters, saved views | Unchanged. |
| Audit log | Unchanged. It is the item history in the UI. |
| Health checks | Not built. There is no process to check. |

1. **The server says what's offered.** The web app doesn't check which build it's in. For the local user
   (`Authentication::LocalUser`) the API leaves out of `allowed_actions` what one user can't do: the team offers
   only `create_heap`, with no member actions, and `GET /me`'s `allowed_actions` is empty, where a signed-in user
   gets `sign_out` and `create_team`. The web app hides sign-out from that, and when the caller can't create teams
   and has one, its header links straight to that team's page instead of the Teams list, and `/teams` redirects
   there.
2. **Cargo features, not runtime flags.** `service` and `api` have a default feature, `accounts`, for
   registration, sign-in, sessions, rate limiting, user lookup and team management (creating, renaming and leaving
   teams, and changing members). The local build turns it off, so that code, argon2 and tokio's blocking pool aren't
   in the wasm at all. The features are additive: with `accounts` on, `Authentication::LocalUser` still works, which
   lets the native API tests cover the local user, and the sign-in routes then answer `404`.
   The permission functions in `domain` don't change. What changes is which use cases exist.

### Platform differences

1. **Clock:** `service::Clock` gets an implementation over `Date.now()`, built by `local`. `SystemTime::now()` panics
   on `wasm32-unknown-unknown`.
2. **Randomness:** UUIDv7 and anything else that needs randomness use `getrandom` with its `wasm_js` backend.
3. **Async without a tokio runtime:** `wasm-bindgen-futures` drives the Router, and the worker is single-threaded.
   - The workspace asks only for tokio's `rt`. `server` adds the multi-threaded runtime, signals and networking,
     and axum's `tokio` and `http1` features, which only `axum::serve` needs.
   - The server's trace and timeout layers can't run in wasm: `tower-http`'s tracing reads `Instant::now()`, and
     its timeout needs tokio's timer. They are `api`'s `trace-and-timeout` feature, and the local build keeps only
     the request-ID layers.
   - Migration timestamps use `sea-orm-migration`'s `with-time` with `time`'s `wasm-bindgen` feature, since the
     default reads `SystemTime::now()`, which panics.
4. **`Send` bounds:** SeaORM's proxy and axum's handlers want `Send` futures, and JS values aren't `Send`. No
   wrapper is needed: the proxy calls free JS functions and turns their results into Rust values before any `await`,
   so no JS value lives in a future.
5. **SeaORM features:** in SeaORM 2.0, `proxy` doesn't compile alongside the sqlx drivers. `local`'s dependencies
   are declared for the wasm target only, so a native workspace build never turns `proxy` on.
6. **Build flags:** `getrandom` picks its browser backend by a `cfg` flag (`.cargo/config.toml`), and wasm-pack's
   bundled `wasm-opt` is given the WebAssembly features Rust emits by default.
7. **Time zone:** the web app keeps sending the `Time-Zone` header, so due labels work the same.

### Keeping the data

Browser storage can be cleared by the user, or by the browser when the disk fills up.

1. **Persistent storage.** Each start the app asks the browser to keep its data (`navigator.storage.persist()`)
   unless it already does, and its footer says plainly that the data lives only in this browser, and when the
   browser hasn't agreed, that it may clear it when space runs low.
2. **Export and import.** The Account page offers the database file as a download, and opening a copy replaces the
   local database after a confirmation. The export is a normal SQLite file with the server's schema, which leaves a
   path open to moving it onto a self-hosted server later.
   - **They aren't API endpoints.** They are about where this build keeps its data, which a server has no part
     in. The worker takes `export` and `import` messages beside the HTTP requests, in the same queue, so neither
     runs in the middle of a request.
   - **An import can't lose the data.** The worker refuses a file that isn't a sound SQLite database with
     SeaORM's migration table. Otherwise it keeps a copy of the current database, swaps in the file, and opens it
     with the migrations, so an older copy is brought up to date. If that fails, as for a copy from a newer
     version, it puts the old database back and says nothing changed. After an import the page starts again from
     the top, since everything on screen came from the database it replaced.
3. **One tab at a time.** `opfs-sahpool` allows one connection per database. Before it opens the database, the
   worker takes a Web Lock that it holds until its tab closes. A second tab's worker waits for the lock and says
   so, and the page shows "Upheap is open in another tab" instead of the app until the other tab closes, then
   opens there.
4. **The web app finds these through `#local-database`,** which package.json `imports` resolves like
   `#api-fetch`: to `src/local/database.ts` in the local build, and to `null` in the server build, which then
   shows none of it and holds none of its code. This is about the build, not about what the user may do, so it
   doesn't go through `allowed_actions`.

## Build and publish

1. **Rust:** `npm run build:backend` in `web` runs wasm-pack on `crates/local` (release, `wasm-opt -Oz`) and writes
   the wasm and its JS glue into `web/src/local/backend`, which git ignores.
2. **Web:** `vite build --mode local-build` (`npm run build:local`) picks the worker `fetch`, and `UPHEAP_BASE`
   sets the base path for Pages (`/<repo>/`), which the router uses too. A copy of `index.html` as `404.html` will
   make deep links work, since Pages can't rewrite paths. `npm run dev:local` runs it with Vite, with no server.
3. **CI:** the "Local build" job lints `service` and `api` without their features and `local` for wasm, builds the
   backend and checks its size, typechecks the worker, runs the backend in Node on SQLite-wasm
   (`web/scripts/local-smoke.ts`), and builds the web app. A change that breaks the local build fails the pull
   request. A separate workflow will publish to GitHub Pages from `main`.
4. **Tests:** the Rust suites stay native and cover the shared code. Playwright runs the golden paths that apply
   (add and rank, close and reopen, block and unblock, parameter management) against the static build too, plus one
   test for export and import. The proxy connection is tested only there, because it needs a browser.
5. **Size:** without accounts the backend is 4.1 MB of wasm (1.31 MB gzipped; 4.5 MB and 1.45 MB with them), and
   SQLite-wasm adds 0.4 MB gzipped. CI fails a build over 1.4 MB gzipped. No one part dominates what's left. Of
   about 2.7 MB of code before `wasm-opt`, async state machines take 0.5 MB, then axum, serde, sea-query and
   SeaORM about 0.25 MB each, and the OpenAPI document about 0.2 MB. The bundled time zone database is another
   0.2 MB of data, and the Time-Zone header needs it. Leaving the OpenAPI document out would mean declaring the
   routes twice, which isn't worth 0.2 MB, so the budget stands as is.

## Plan

1. **Spike: done.** See [Spike results](#spike-results).
2. **Features: done.** `accounts` in `service` and `api`, the local user, `trace-and-timeout` in `api`, and the CI
   job. The server build doesn't change in behaviour, and `openapi.json` is unchanged.
3. **`crates/local` and the worker: done.** `Backend`, the worker and its `fetch`, the `local-build` mode, and the
   smoke test in CI. In headless Chromium the app opens on the local user's heap, migrations included, in about
   220 ms, and items stay across a reload and a browser restart.
4. **Single-user mode: done.** `GET /me` offers `sign_out` and `create_team` on the server only, the local user's
   team offers only `create_heap`, and the web app hides the rest: no sign-out, and the header links straight to
   the one team, where `/teams` also leads.
5. **Keeping the data: done.** The footer note and the request for persistent storage, export and import on the
   Account page, and one tab at a time. In headless Chromium a second tab waits and opens once the first closes,
   a downloaded copy opened again replaces what was added since, and a file that isn't an Upheap database or comes
   from a newer version is refused with the data left as it was.
6. **Publish:** the Pages workflow and Playwright against the static build.

## Spike results

The spike lived in `crates/local` and `crates/local/spike` until step 3 replaced it with `Backend` and the
worker. It ran in Node on in-memory SQLite-wasm, and in a module worker on `opfs-sahpool` driven with Playwright in
headless Chromium.

1. **The backend runs unchanged.** The real migrations, `register`, `create_item` and `list_ranked` ran on
   SQLite-wasm through the proxy, with audit entries, and the items came back ranked with contributors.
2. **The Router runs unchanged.** Register (`201` with a session cookie), `GET /me`, the ranked list (`200`) and an
   unknown item (`404` problem+json) went through `api::router` as HTTP requests.
3. **The data stays.** After a reload and after a browser restart, the database was found in OPFS and no migration
   ran again.
4. **Transactions behave.** A dropped transaction left nothing behind, a committed one kept its row, and registering
   an email twice gave `EmailTaken`.
5. **It's fast.** The worker starts in about 25 ms when cached (85 ms the first time). The first run, with all 8
   migrations, took about 180 ms; HTTP requests take a few milliseconds each.
6. **The server build is unchanged.** All 196 native tests pass, and clippy is clean natively and for wasm.

Its shortcuts are gone: inline password hashing on wasm and leaving out the trace and timeout layers by target went
with the features in step 2, and the spike entry points with `Backend` in step 3.

## Open questions

1. Is moving an exported database onto a server worth building? The local user has no password, so an import there
   would need a way to claim the account.
