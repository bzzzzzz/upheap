# Upheap

A todo list for people with more work than they can keep track of. Add items as they come up with how much
they're needed and a rough size, impact and due date; the app ranks everything and keeps the ranking current as
time passes.

The design docs are indexed in [design/README.md](design/README.md).

## What works today

- Local accounts: register, sign in, sign out. Registering creates your personal team and heap.
- The ranked list: items ordered by need, age (faster the more an item is needed), deadline proximity and value
  per effort (formula `V2`),
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
- Saved views (API only for now; the web app hides them): name a list view (states, sort, filters) and the whole
  team gets it. Change one, save it as a new view, or archive views nobody needs.
- Optimistic concurrency (`If-Match`), audit entries for every write, problem+json errors with stable codes.
- Audit log: the history of each item, tag, parameter, saved view, heap and member, and Activity pages per heap and
  team.
- Sign-in rate limiting: each email address gets 5 attempts per 15 minutes.
- Signing in non-web clients: OAuth 2.0 with PKCE (`/api/v1/oauth/authorize`, `/token`, `/revoke`) for the
  `upheap` CLI, with rotating refresh tokens and `Authorization: Bearer` on every endpoint. Account lists the CLI's
  sign-ins by machine and ends them.
- One binary serving the API, the web app, `/healthz` and `/readyz`, on SQLite, MySQL or Postgres.
- The `upheap` CLI: sign in through the browser, list your heaps and pick a default, list a heap's ranked or closed
  items with the web app's filters and saved views, add, show, edit and move items between states, and list, add and edit
  tags.

Not yet: OIDC sign-in and invitations.

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
`DATABASE_URL` at Postgres (`postgres://user:pass@host/db`) or MySQL (`mysql://user:pass@host/db`) if you want more
than SQLite. For Cloud Run with Neon, see [design/operations.md](design/operations.md).

### Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | required | `sqlite://path?mode=rwc`, `mysql://…` or `postgres://…` |
| `DATABASE_MAX_CONNECTIONS` | `5` | Most database connections the server keeps open. Size it to the database's limit divided by the instances that may run; earlier versions used 10. In-memory SQLite always keeps one |
| `BASE_URL` | required | Public URL. Cookies are `Secure` unless it starts with `http://` |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address |
| `ALLOW_REGISTRATION` | `true` | Set to `false` to close sign-ups; a team admin's invite link still lets a new person create an account |
| `SESSION_TTL_DAYS` | `30` | How long a sign-in lasts |
| `LOG_FORMAT` | `json` | `json` or `pretty`; levels via `RUST_LOG` |

## The CLI

`upheap` talks to a running server over its API.

### Install

On macOS with Apple silicon:

```sh
brew install bzzzzzz/tap/upheap
```

On Linux (x86_64 or aarch64), a static binary with no system dependencies:

```sh
arch=$(uname -m)   # x86_64 or aarch64
base=https://github.com/bzzzzzz/upheap/releases/latest/download
curl -fsSLO "$base/upheap-$arch-unknown-linux-musl.tar.gz"
curl -fsSLO "$base/SHA256SUMS"
grep " upheap-$arch-unknown-linux-musl.tar.gz\$" SHA256SUMS | sha256sum -c
mkdir -p ~/.local/bin
tar -xzf "upheap-$arch-unknown-linux-musl.tar.gz" -C ~/.local/bin upheap
```

`~/.local/bin` must be on your `PATH`; some distributions add it only if it existed when you logged in.

On Linux the sign-in is kept in the Secret Service, so a desktop keyring (GNOME Keyring or KWallet) must be running.

Or build it with `cargo build --release -p cli` (the binary is `target/release/upheap`), or run it from source:

```sh
cargo run -p cli -- auth login --server http://localhost:8080   # opens the browser; the sign-in goes in the keychain
cargo run -p cli -- heap list
cargo run -p cli -- heap use personal                           # or team/heap
cargo run -p cli -- heap show                                   # the heap's name and context
cargo run -p cli -- heap edit --context-file context.md         # or - for stdin, or --no-context
cargo run -p cli -- item list --effort s,m --tag infra
cargo run -p cli -- item list --json | jq '.ranked[].item.key'
cargo run -p cli -- item add "Rotate the staging certs" --effort s --due next-week
cargo run -p cli -- item show k3x9a --history                    # name an item by key, team/heap/key or a link
cargo run -p cli -- item edit k3x9a --context-file context.md    # the item's context; or - for stdin, or --no-context
cargo run -p cli -- item done k3x9a
cargo run -p cli -- item to-task k3x9a                           # a note becomes a task, keeping its key and history
cargo run -p cli -- step add k3x9a "Run in staging"             # a task's steps; add --after k3x9a.1 to place one
cargo run -p cli -- step tick k3x9a.1                            # a step is its item's key and its number
cargo run -p cli -- tag add infra --description "Servers and the network"
cargo run -p cli -- auth logout
```

The server address and default heap are kept in `config.toml` under the platform's config directory, and
`UPHEAP_SERVER`, `UPHEAP_HEAP`, `--server` and `--heap` override them. Exit codes: 0 done, 2 bad command line, 3
refused, 4 not signed in or not found, 5 server unreachable, 6 local setup broken (no keychain), 1 anything else.

## Develop

```sh
# API server on :8137 with a throwaway database
DATABASE_URL="sqlite://data/dev.db?mode=rwc" BASE_URL=http://localhost:5173 BIND_ADDR=127.0.0.1:8137 \
  LOG_FORMAT=pretty cargo run -p server
(cd web && npm run dev)         # http://localhost:5173, proxies /api to :8137
```

Or start the API server on sample data instead: `scripts/seed-dev.py` deletes `data/dev.db`, starts the server on a
fresh one, and adds three test accounts (alex, blair, casey; password `dev-password-123`) in two shared teams with
items in every state. It keeps the server running until Ctrl-C; `--no-serve` stops it once the data is in.

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

The tests use in-memory SQLite. To run the store and API suites on MySQL or Postgres as CI does, point
`TEST_DATABASE_URL` at a server (no database name; each test creates its own):

```sh
docker run -d --rm --name upheap-test-mysql -e MYSQL_ROOT_PASSWORD=root -p 127.0.0.1:3307:3306 mysql:8.4
TEST_DATABASE_URL=mysql://root:root@127.0.0.1:3307 cargo test -p store -p api
docker stop upheap-test-mysql

docker run -d --rm --name upheap-test-postgres -e POSTGRES_PASSWORD=postgres -p 127.0.0.1:5433:5432 postgres:17
TEST_DATABASE_URL=postgres://postgres:postgres@127.0.0.1:5433 cargo test -p store -p api
docker stop upheap-test-postgres
```

The local build ([design/architecture.md](design/architecture.md#local-build)) runs the backend in a worker in the page, on
SQLite-wasm with its data in the browser. It needs `rustup target add wasm32-unknown-unknown` and wasm-pack 0.14.
Build the backend into the web app, then run it with Vite or check it as CI does:

```sh
(cd web && npm run build:backend)   # after every change to the Rust code
(cd web && npm run dev:local)       # http://localhost:5173, no server needed
cargo clippy -p service -p api --no-default-features -- -D warnings
cargo clippy -p local --target wasm32-unknown-unknown -- -D warnings
(cd web && npm run typecheck:local && npm run smoke:local)
(cd web && npm run e2e:local)       # Playwright against the static build, served as GitHub Pages does
```

The Pages workflow (`.github/workflows/pages.yml`) publishes it from `main` to
`https://<owner>.github.io/<repo>/`, once the repository's Pages source is set to GitHub Actions. Pull requests
that auto-merge start it only when the `AUTO_MERGE_TOKEN` secret holds a personal access token (see
`.github/workflows/auto-merge.yml`); a merge made with the workflow's own token starts no workflows.

After changing the API, regenerate the spec and the TypeScript types, and commit both:

```sh
cargo run -q -p server -- openapi > openapi.json && (cd web && npm run gen:api)
```

## Layout

```
crates/domain     pure logic: types, state machine, ranking, due labels, permissions
crates/store      SeaORM entities, migrations, repositories
crates/service    use cases: authorization, transactions, audit
crates/api-types  request and response types, shared by the API and the CLI
crates/api        axum routes, OpenAPI, sessions, problem+json
crates/cli        the upheap command-line client
crates/server     the binary: config, startup, embedded web app
crates/local      the local build: the backend in wasm, on SQLite-wasm in the browser
web/              React app (TypeScript), generated API types only
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
