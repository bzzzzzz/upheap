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

1. **New crate `crates/local`.** The counterpart of `server`. It builds the clock and the database connection, runs
   the migrations, builds the `api` Router and exposes one `handle(request) → response` function through
   `wasm-bindgen`. Dependencies still point one way: `local → api → service → store, domain`.
2. **Requests go through the Router.** The worker turns a message into an `http::Request`, calls the axum Router as a
   tower `Service`, and posts the response back. Routes, DTOs, error mapping, `If-Match` and problem+json all stay
   as they are.
3. **The web app swaps `fetch`, nothing else.** openapi-fetch takes a custom `fetch`. In the local build it posts the
   request to the worker instead of the network. TanStack Query, the routes and the components don't change.
4. **One request at a time.** The worker handles requests in order and finishes one before starting the next. This
   gives the one-writer behaviour that SQLite already has on the server, and keeps transactions from interleaving
   when a handler awaits.

### Database

1. **SQLite in the browser:** the official SQLite WebAssembly build, `@sqlite.org/sqlite-wasm`, with its
   `opfs-sahpool` VFS. The database is a real SQLite file in the Origin Private File System. This VFS works without
   the COOP and COEP headers, which GitHub Pages can't set, but only inside a worker, which is where the backend runs
   anyway.
2. **SeaORM over a proxy connection.** sqlx doesn't build for wasm. SeaORM's `proxy` feature takes our own backend
   (`ProxyDatabaseTrait`), which gets the SQL and values and returns rows. `store` adds a `local` feature that builds
   this connection with `DbBackend::Sqlite`, so SeaORM generates the same SQLite SQL as on the server. The proxy
   calls SQLite-wasm synchronously through a small JS shim in the same worker.
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

The local build has one user and one team. Both are created on first start, with the first heap, by the same
service code that creates a personal team and heap on registration. The API resolves every request to that user, and
every heap belongs to that team.

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

1. **The server says what's offered.** The web app doesn't check which build it's in. Actions the local build lacks
   are left out of `allowed_actions` (for example `add_member` and `rename` on the team). `GET /me` gains
   `allowed_actions` too, with `sign_out` and `create_team` in the server build only, so the web app hides sign-out,
   the Teams list and team creation the same way. With one team, the web app links straight to its page.
2. **Cargo features, not runtime flags.** `api` and `service` get a default feature, `accounts`, for sessions,
   passwords, rate limiting, member search and team management (creating, renaming and leaving teams, and changing
   members). The local build turns it off, so that code isn't in the wasm at all.
   The permission functions in `domain` don't change. What changes is which use cases exist.

### Platform differences

1. **Clock:** `service::Clock` gets an implementation over `Date.now()`, built by `local`. `SystemTime::now()` panics
   on `wasm32-unknown-unknown`.
2. **Randomness:** UUIDv7 and anything else that needs randomness use `getrandom` with its `wasm_js` backend.
3. **Async without a tokio runtime:** `wasm-bindgen-futures` drives the Router, and the worker is single-threaded.
   - The workspace asks only for tokio's `rt`. `server` adds the multi-threaded runtime, signals and networking,
     and axum's `tokio` and `http1` features, which only `axum::serve` needs.
   - The server's trace and timeout layers can't run in wasm: `tower-http`'s tracing reads `Instant::now()`, and
     its timeout needs tokio's timer. The local build keeps only the request-ID layers.
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

1. On first start the app asks for persistent storage (`navigator.storage.persist()`), and says plainly in the
   footer that the data lives only in this browser.
2. **Export and import.** The Account view offers the database file as a download, and an import that replaces the
   local database after a confirmation. The export is a normal SQLite file with the server's schema, which leaves a
   path open to moving it onto a self-hosted server later.
3. **One tab at a time.** `opfs-sahpool` allows one connection per database. The worker takes a Web Lock on start-up,
   and a second tab shows "Upheap is open in another tab" instead of the app.

## Build and publish

1. **Rust:** `cargo build -p local --target wasm32-unknown-unknown --release`, then `wasm-bindgen` (version pinned to
   the crate's) for the JS glue. Release builds use `opt-level = "z"` for this crate.
2. **Web:** `vite build --mode local` picks the worker `fetch` and the base path for Pages (`/<repo>/`). A copy of
   `index.html` as `404.html` makes deep links work, since Pages can't rewrite paths.
3. **CI:** the existing workflow also builds the local target, so a change that breaks the wasm build fails the pull
   request. A separate workflow publishes to GitHub Pages from `main`.
4. **Tests:** the Rust suites stay native and cover the shared code. Playwright runs the golden paths that apply
   (add and rank, close and reopen, block and unblock, parameter management) against the static build too, plus one
   test for export and import. The proxy connection is tested only there, because it needs a browser.
5. **Size:** with the Router, the backend is 4.5 MB of wasm (1.45 MB gzipped), and SQLite-wasm adds 0.4 MB
   gzipped. Without `api` the backend is 0.5 MB gzipped, so the API layer is most of it; the OpenAPI document built
   into the Router is the first thing to check, then the `accounts` code. CI gets a budget once that's trimmed.

## Plan

1. **Spike: done.** See [Spike results](#spike-results).
2. **Features.** Put `accounts` behind a default feature in `service` and `api`, and keep tokio-only code out of the
   shared path. The server build doesn't change in behaviour.
3. **`crates/local` and the worker,** plus the custom `fetch` in the web app.
4. **Single-user mode:** the local user and their one team, `allowed_actions` on `GET /me`, and the UI that hides
   missing actions.
5. **Keeping the data:** persistent storage, export and import, the tab lock.
6. **Publish:** the Pages workflow and Playwright against the static build.

## Spike results

`crates/local` and `crates/local/spike` hold the spike. `spike/node.mjs` runs it in Node on in-memory SQLite-wasm;
`spike/index.html` runs it in a module worker on `opfs-sahpool`, and was driven with Playwright in headless Chromium.
To run it: `wasm-pack build --target web --release --no-typescript --no-pack` in `crates/local`, `npm install` in
`spike`, then `node node.mjs`, or serve `crates/local` over HTTP and open `/spike/index.html`.

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

Spike-only shortcuts to replace in the next steps: `service` hashes passwords inline on wasm (the `accounts`
feature removes this), `api` leaves out its trace and timeout layers by target rather than by feature, and the
spike entry points (`spike`, `spike_http`) stand in for the worker's `handle`.

## Open questions

1. Is moving an exported database onto a server worth building? The local user has no password, so an import there
   would need a way to claim the account.
