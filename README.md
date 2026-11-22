# Upheap

A ranked todo list for people with more work than they can keep track of. Add items as they come up with how much
they're needed and a rough size, impact and due date; Upheap ranks everything, says why each item is where it is,
and keeps the ranking current as time passes. Part of the work can be handed to AI agents, which pick up what the
ranking puts first and ask when they need you. One binary serves the API and the web app, on SQLite, MySQL or
Postgres, and the `upheap` command-line client does everything the web app does.

## Try it

With Docker, on SQLite kept on a volume:

```sh
docker compose up --build
# open http://localhost:8080 and create an account
```

Then, with the command-line client (`brew install bzzzzzz/tap/upheap` on macOS with Apple silicon):

```sh
upheap auth login --server http://localhost:8080
upheap item add "Rotate the staging certs" --need expected --effort s --due next-week
upheap item list
```

## Where to go next

- **Use it:** the [user's guide](https://github.com/bzzzzzz/upheap/blob/main/docs/user/README.md) goes from
  installing the CLI to an agent working your list, with pages on how the ranking works, the agent loop, personal
  tokens and the Claude Code plugin.
- **Run it:** the [operator's guide](https://github.com/bzzzzzz/upheap/blob/main/docs/operator/README.md) goes from
  nothing to a running server, with pages on configuration, signing in through a provider, mail and accounts.

## Develop

```sh
# API server on :8137 with a throwaway database
DATABASE_URL="sqlite://data/dev.db?mode=rwc" BASE_URL=http://localhost:5173 BIND_ADDR=127.0.0.1:8137 \
  LOG_FORMAT=pretty cargo run -p server
(cd web && npm run dev)         # http://localhost:5173, proxies /api to :8137
```

The CLI runs from source with `cargo run -p cli -- <command>`, and builds with `cargo build --release -p cli` (the
binary is `target/release/upheap`).

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

The invite-link golden path needs a server with registration closed, so its worker starts a second copy of the same
binary on its own port and a temporary SQLite file: it registers two accounts while registration is open, then
restarts the server with registration closed (`web/e2e/closedServer.ts`).

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

After changing the API, regenerate the spec and the TypeScript types, and commit both:

```sh
cargo run -q -p server -- openapi > openapi.json && (cd web && npm run gen:api)
```

## Layout

How it's built and why, and every other doc for someone changing the code, is indexed in
[docs/README.md](https://github.com/bzzzzzz/upheap/blob/main/docs/README.md).

```
crates/domain     pure logic: types, state machine, ranking, due labels, permissions
crates/store      SeaORM entities, migrations, repositories
crates/service    use cases: authorization, transactions, audit
crates/api-types  request and response types, shared by the API and the CLI
crates/api        axum routes, OpenAPI, sessions, problem+json
crates/cli        the upheap command-line client
crates/mail       sending plain-text mail over SMTP, or to files or the log
crates/server     the binary: config, startup, embedded web app
web/              React app (TypeScript), generated API types only
plugins/upheap    the Claude Code plugin: skills that add items, report status and do the next one, through the CLI
```

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT), at your option.
