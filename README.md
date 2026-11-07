# Upheap

A todo list for people with more work than they can keep track of. Add items as they come up with a rough
size, impact and due date; the app ranks everything and keeps the ranking current as time passes.

Design docs: [requirements](design/requirements.md), [offering](design/offering.md),
[technical design](design/technical-design.md), [engineering principles](design/engineering-principles.md).

## What works today

- Local accounts: register, sign in, sign out. Registering creates your personal team and heap.
- The ranked list: items ordered by age, deadline proximity and value per effort (placeholder formula `V1`),
  with the top reasons for each position. The order stays stable within a session (`as_of`) and edited items
  move with an animation.
- List view controls: each state pinned, shown or hidden (in progress pinned and blocked hidden by default),
  blocked items grouped or mixed in, sort by priority, due, effort or impact, filters by tag, effort and impact,
  and text search (press `/`). The view lives in the URL; the search text doesn't.
- Adding items (press `N`), inline edits of title, effort, impact, due date and state, closing, and
  reopening within a week from the Closed view.
- Edit item: every field, tags, state and blocker in one form, opened from the list or from Closed. A closed
  item can be reopened and edited in the same save.
- Tags: per-heap tags on items, a tag picker, and a Manage tags view for renaming and describing them. Tags
  can be removed from an item but never deleted.
- Blockers: a blocked item can wait for another item in its heap. Cycles are refused, and a blocker inherits
  the priority of what it blocks (with a small boost), shown as "Blocks …" in its reasons.
- Custom parameters: admins define per-heap parameters such as "Customer" with ordered options and a weight, link
  them to tags, and archive options (open items switch to a replacement) or unused parameters. Items get a value per
  parameter; it counts in the ranking and its reasons, and the list filters and sorts by it.
- Teams: create teams, add members by name or email, make admins, remove members or leave; a team always keeps an
  admin. Admins create, rename, archive and unarchive heaps; an archived heap is read-only.
- Saved views: name the list view you're looking at (states, sort, filters) and the whole team gets it as a link
  above the list. Change one and save the change into it or as a new view; archive views nobody needs.
- Optimistic concurrency (`If-Match`), audit entries for every write, problem+json errors with stable codes.
- Audit log: the history of each item, tag, parameter, saved view, heap and member, and Activity pages per heap and
  team.
- Sign-in rate limiting: each email address gets 5 attempts per 15 minutes.
- One binary serving the API, the web app, `/healthz` and `/readyz`, on SQLite or MySQL.

Not yet: editing the heap's own weights, OIDC sign-in, the CLI OAuth flow and invitations.

## Run it

With Docker (SQLite on a volume):

```sh
docker compose up --build
# open http://localhost:8080
```

From source (Rust 1.95+, Node 24+):

```sh
(cd web && npm ci && npm run build)
DATABASE_URL="sqlite://data/heap.db?mode=rwc" BASE_URL=http://localhost:8080 LOG_FORMAT=pretty \
  cargo run --release -p server
```

In production, put a TLS-terminating reverse proxy in front, set `BASE_URL` to its `https://` address, and point
`DATABASE_URL` at MySQL (`mysql://user:pass@host/db`) if you want more than SQLite.

### Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | required | `sqlite://path?mode=rwc` or `mysql://…` |
| `BASE_URL` | required | Public URL. Cookies are `Secure` unless it starts with `http://` |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address |
| `ALLOW_REGISTRATION` | `true` | Set to `false` to close sign-ups |
| `SESSION_TTL_DAYS` | `30` | How long a sign-in lasts |
| `LOG_FORMAT` | `json` | `json` or `pretty`; levels via `RUST_LOG` |

## Develop

```sh
# API server on :8137 with a throwaway database
DATABASE_URL="sqlite://data/dev.db?mode=rwc" BASE_URL=http://localhost:5173 BIND_ADDR=127.0.0.1:8137 \
  LOG_FORMAT=pretty cargo run -p server
scripts/seed-dev.sh             # test account dev@example.test and sample items (BASE=http://localhost:8137)
(cd web && npm run dev)         # http://localhost:5173, proxies /api to :8137
```

Checks:

```sh
cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
(cd web && npm run typecheck && npm run lint && npm run format:check)
```

End-to-end tests of the golden paths, in Chromium. Playwright builds the web app and starts the server on port 8139
with an in-memory database:

```sh
(cd web && npx playwright install chromium && npm run e2e)
```

The tests use in-memory SQLite. To run the store and API suites on MySQL as CI does, point `TEST_DATABASE_URL` at a
MySQL server (no database name; each test creates its own):

```sh
docker run -d --rm --name upheap-test-mysql -e MYSQL_ROOT_PASSWORD=root -p 127.0.0.1:3307:3306 mysql:8.4
TEST_DATABASE_URL=mysql://root:root@127.0.0.1:3307 cargo test -p store -p api
docker stop upheap-test-mysql
```

The local build ([design/local-build.md](design/local-build.md)) is checked as CI does: without accounts, for
wasm, and run in Node on SQLite-wasm. It needs `rustup target add wasm32-unknown-unknown` and wasm-pack 0.14:

```sh
cargo clippy -p service -p api --no-default-features -- -D warnings
cargo clippy -p local --target wasm32-unknown-unknown -- -D warnings
wasm-pack build crates/local --target web --release --no-typescript --no-pack
(cd crates/local/spike && npm install && node node.mjs)
```

After changing the API, regenerate the spec and the TypeScript types, and commit both:

```sh
cargo run -q -p server -- openapi > openapi.json && (cd web && npm run gen:api)
```

## Layout

```
crates/domain   pure logic: types, state machine, ranking, due labels, permissions
crates/store    SeaORM entities, migrations, repositories
crates/service  use cases: authorization, transactions, audit
crates/api      axum routes, DTOs, OpenAPI, sessions, problem+json
crates/server   the binary: config, startup, embedded web app
crates/local    the local build: the backend in wasm, on SQLite-wasm in the browser
web/            React app (TypeScript), generated API types only
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
