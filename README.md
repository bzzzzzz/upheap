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
  items with the web app's filters and saved views, add, show, edit and move items between states, list, add and edit
  tags, set up a heap's parameters and options, and set them on items, play the agent: ask for the next item, claim it, and end
  the run (`work`), and print a guide to every command for an agent to read first.

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
| `MAIL_URL` | unset | Where mail goes: `smtps://user:pass@host:465`, `smtp://user:pass@host:587` (STARTTLS), or for development `file:///dir` or `log:`. Unset means no mail |
| `MAIL_FROM` | unset | The sender, `Upheap <upheap@example.com>`. Required with `MAIL_URL` |

### Setting up mail

Mail is optional: without `MAIL_URL` and `MAIL_FROM` the server sends none. To set it up, pick any SMTP provider, set
up its sending domain (the SPF and DKIM records it gives you), and set `MAIL_URL` and `MAIL_FROM`. Use port 465
(`smtps://`) or 587 (`smtp://`, which always requires STARTTLS and never sends in cleartext): many hosts, Cloud Run
included, block port 25. Percent-encode special characters in the user and password (`@` as `%40`, `/` as `%2F`, `?`
as `%3F`, `#` as `%23`). For example, with Resend:

```sh
MAIL_URL='smtps://resend:<API key>@smtp.resend.com:465'
MAIL_FROM='Upheap <upheap@example.com>'
```

Restart the server and check that the `listening` line in its log has `mail` set to the scheme, host and port.
Then send a test mail:

```sh
upheap-server mail test you@example.com
docker compose exec upheap upheap-server mail test you@example.com
```

It takes the same environment variables as the server, and prints `Sent a test mail to … via …` on success. Exit
codes: `0` sent, `2` wrong arguments or not an email address, `6` mail isn't set up or the send failed, `1` anything
else.

For development, `MAIL_URL=log:` writes each mail to the server's log, and `MAIL_URL=file:///tmp/upheap-mail` writes
each as an `.eml` file in that directory, which must exist. Nothing is sent.

### Resetting a password

Whoever runs the server makes a one-time reset link for a user, who opens it, sets a new password and is signed in.
The command takes the same environment variables as the server (at least `DATABASE_URL` and `BASE_URL`):

```sh
upheap-server user reset-link sam          # a username or an email address
docker compose exec upheap upheap-server user reset-link sam
```

It prints the link, `{BASE_URL}/reset/…`, alone on stdout, and on stderr whom it is for and when it ends (it works
once, for 60 minutes). A newer link for the same user cancels the earlier one. It runs no migrations and starts no
server, so run it once the server of the same version has started. Exit codes: `0` made, `2` wrong arguments, `3` no
such user, `5` the database can't be reached, `1` anything else, a database a version behind included.

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
cargo run -p cli -- heap add "Roadmap"                           # in your personal team; or --team infra, --slug plan
cargo run -p cli -- heap use personal                           # or team/heap
cargo run -p cli -- heap show                                   # the heap's name, weights and context
cargo run -p cli -- heap edit --context-file context.md         # or - for stdin, or --no-context
cargo run -p cli -- heap weights --need 20 --parameter Area=30   # ranking weights, admins only; heap show prints them
cargo run -p cli -- item list --effort s,m --tag infra
cargo run -p cli -- item list --all-heaps
cargo run -p cli -- item list --json | jq '.ranked[].item.key'
cargo run -p cli -- item add "Rotate the staging certs" --effort s --due next-week
cargo run -p cli -- item add "Order the new certificate" --task -q    # added as a task; -q prints its key
cargo run -p cli -- item add "Rotate certs" --task --context-file context.md --step "Order it" --step "Install it"
cargo run -p cli -- item add "Renew the domain" --reminder 2027-03-12   # out of the list until that date
cargo run -p cli -- item list --reminders shown                  # adds the reminders to come, in a group of their own
cargo run -p cli -- item show k3x9a --history                    # name an item by key, team/heap/key or a link
cargo run -p cli -- item block k3x9a --reason "Waits for the vendor's quote"   # or --by another item
cargo run -p cli -- item edit k3x9a --context-file context.md    # the item's context; or - for stdin, or --no-context
cargo run -p cli -- item done k3x9a
cargo run -p cli -- item to-task k3x9a                           # a note or a reminder becomes a task, keeping its key and history
cargo run -p cli -- item add "Rotate staging" --task --from k3x9a --need expected --age keep   # split a note: a task made from it
cargo run -p cli -- item became k3x9a                            # the note is closed as became, once items are made from it
cargo run -p cli -- item move k3x9a other-heap                   # to another heap of its team
cargo run -p cli -- step add k3x9a "Run in staging"             # a task's steps; add --after k3x9a.1 to place one
cargo run -p cli -- step tick k3x9a.1                            # a step is its item's key and its number
cargo run -p cli -- question ask k3x9a "Which CA?" --choice Internal --choice "Let's Encrypt"  # a question: free text, or --choice to pick; --many, --step, --details-file
cargo run -p cli -- question request k3x9a "Put a token in the vault."  # a request: replied to with Done or Can't be done
cargo run -p cli -- question review k3x9a https://github.com/o/r/pull/12 "Paint the fence"  # an agent in a run asks for review; replied to with Approved or Changes asked; --piece, --details-file
cargo run -p cli -- question propose-done k3x9a "Painted and checked"  # an agent in a run proposes closing; the person replies Close as done or Not yet
cargo run -p cli -- comment add k3x9a "Used the internal CA."   # a comment on the item; or --file notes.md, or - for stdin
cargo run -p cli -- comment list k3x9a                          # an item's comments, oldest first
cargo run -p cli -- question reply k3x9a.q1 --pick Internal  # reply: --pick a choice by label or number, --comment or --comment-file
cargo run -p cli -- work next                                    # what an agent would work on next, without taking it
cargo run -p cli -- work claim --quiet                           # take it: starts a run and prints the run's ID
cargo run -p cli -- work run --tool claude                       # the agent loop: claim, start claude on the item, record the run, repeat; Ctrl-C stops it
cargo run -p cli -- work run --tool claude --heap work --once    # one run in a heap, or nothing when there is nothing to do
cargo run -p cli -- work run --tool claude --root ~/agent-runs   # each item in its own kept directory under the root, set up for claude
cargo run -p cli -- work run --tool claude --root ~/agent-runs --workers 3  # up to 3 runs at once, each on its own item
cargo run -p cli -- work run --tool claude --instructions ~/how-i-work.md  # tell every run how you want work done on this machine
cargo run -p cli -- work run --tool claude --allow mcp__claude_ai_Claude_Docs --allow-do 'Bash(npm:*)'  # let runs use a tool your settings don't allow; --allow-prepare / --allow-do for one kind of run only
UPHEAP_RUN=<id> cargo run -p cli -- item edit k3x9a --context-file context.md  # with a run, writes are the run's
UPHEAP_RUN=<id> cargo run -p cli -- work reached --result "Wrote the context"  # the turn's goal is reached
UPHEAP_RUN=<id> cargo run -p cli -- work finish --cost-usd 0.12  # the run is over; what it cost
cargo run -p cli -- tag add infra --description "Servers and the network"
cargo run -p cli -- parameter list                              # a heap's parameters and their options
cargo run -p cli -- parameter add Area --option Backend --option Frontend  # an admin adds one; --weight, --tag; edit and archive too
cargo run -p cli -- parameter option archive Area Backend --replace-with Frontend  # open items with it move to the other
cargo run -p cli -- item edit k3x9a --parameter Area=Backend    # set one; --no-parameter Area clears it
cargo run -p cli -- guide show                                  # every command and its flags in one read, with instructions for agents
cargo run -p cli -- auth logout
```

The server address and default heap are kept in `config.toml` under the platform's config directory, and
`UPHEAP_SERVER`, `UPHEAP_HEAP`, `--server` and `--heap` override them. With `UPHEAP_RUN` set to a run's ID, every
request is made inside that run: it may write the item's context, steps, questions and comments, make a note or a
reminder a task and start it, on the run's own item only, and is refused anything else with exit 3. `work run`
starts `claude` on each item it claims, with `UPHEAP_RUN` set, in the directory it was started in. To prepare an
item (give it its context, or make it a defined task), `claude` may run the `upheap` commands a run uses (not
`auth`), read files under that directory and use the web, and nothing else. To do a task, it may also edit files,
and run what your own and the project's Claude settings allow. With `--root`, each run starts in a directory
under the root instead, `<root>/<team>/<heap>/<key>`, with a `CLAUDE.md` that says how to work there, and `claude`
may run any command, edit and read, for preparing as well: a run there has to clone and build what it works on. The
directory is kept and used again for the item's next run, with what the last run left in it. The loop removes it
once the item is closed or can't be found, or after 7 days with no run on the item, checking when it starts and
about every hour. Old `<key>-<run id>` directories from earlier versions are left for you to clear. Claude's
streamed output for each run is saved in `<config dir>/runs/<run id>.jsonl` (kept 7 days), and a `claude` that shows
no output for 30 minutes is stopped and its run failed. `claude` prints nothing while a single tool call runs, so a
tool call longer than 30 minutes (a raised Bash timeout, a slow MCP tool) counts as silence. With `--instructions <file>`, every run is also told what the file
says: your own way of working on this machine (which skills to use, how to check and review a change), which
comes before the heap's context on how the work is done. The file is read once, when the loop starts. With
`--allow <rule>`, every run's `claude` may also use what the rule allows (Claude's rule syntax, such as
`mcp__claude_ai_Claude_Docs`), without allowing it in your own sessions; `--allow-prepare` and `--allow-do` add a
rule to runs that prepare an item or do a task only. Each flag takes one rule and can be repeated; a rule with a
comma or a space, or one starting with `-`, is refused. Every run is still denied `upheap auth`, though a rule as
wide as `Bash` lets a run get around that through a shell. Exit codes: 0 done, 2 bad command line, 3 refused, 4 not
signed in or not found, 5 server unreachable, 6 local setup broken (no keychain, or no agent tool), 1 anything else.

### Start an agent loop

To have an agent work your items:

1. Install `upheap` (above) and Claude Code (`claude`), signed in.
2. Sign in to your server: `upheap auth login --server https://<your server>`.
3. Make or pick a heap: `upheap heap add "Website"`, then `upheap heap use website`.
4. Write its context: where the repository is, and how work is done and reviewed there. `upheap heap edit
   --context-file context.md`.
5. In the web app, open the heap's settings and turn on "Agents prepare new notes", so each note you add goes to an
   agent. The CLI has no flag for it.
6. Run `upheap work run --tool claude --root ~/agent-runs` and leave it running. Ctrl-C stops it. The flags are
   above, with the other commands.
7. Add a note (`upheap item add "..." --need expected`), answer what the agent asks (in the web app, or with
   `upheap question reply`), and send items on from the web app with "Send to an agent".

### From Claude Code

This repository is also a Claude Code plugin marketplace with one plugin, `upheap`. Its skills work through the
CLI, so they need `upheap` installed and signed in. Each runs when you type its command or name Upheap in a
request.

- `/upheap:add` adds notes, tasks and reminders to a heap from a conversation, and a new heap when you ask for
  one.
- `/upheap:status` says what's in progress, what waits for you, and what's next and why.
- `/upheap:do-next` takes the next item: a note gets its context and becomes a task or stays a note; a task gets
  done, or part of it, or you're told how to do it when an agent can't.

`status` and `do-next` take a heap (`/upheap:status bzzzz/personal`) or `all`.

```
/plugin marketplace add bzzzzzz/upheap
/plugin install upheap@upheap
```

The plugin is in [plugins/upheap](plugins/upheap).

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
