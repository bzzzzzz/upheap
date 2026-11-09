# CLI

Draft. A command-line client for the everyday work in a heap: add items, edit them, move them between states, list
what's in a heap, list heaps, and add and edit tags. Admin work (teams, members, heap settings, parameters) stays in
the web app for now.

The CLI is another client of the API, like the web app. It follows the [thin clients](engineering-principles.md#thin-clients)
rules, so it never ranks, never checks permissions and never works out due labels. The server already returns all of
that.

Command shape, errors, output and code layout follow the Rust CLI guideline shared earlier ("Writing Rust code in
dev-infra"), adapted where this repo's [engineering principles](engineering-principles.md) already decide something.
Where that guideline's sources disagree, [Choices from the CLI guideline](#choices-from-the-cli-guideline) says which
side we took.

## Goals and non-goals

Goals:

1. Adding an item is one short line: `upheap item add "Rotate the staging certs" --effort s --due next-week`.
2. `upheap item list` answers "what should I work on?" the way the web app's list does, with the same order and the same
   reasons.
3. Every command works in scripts: stable exit codes, `--json` with the server's own response, and IDs on stdout
   with `--quiet`.

Not now:

- Teams, members, heap create/rename/archive, parameters, saved view editing, audit browsing beyond one item's history.
- Setting custom parameter values on items. It's a member action and could come next, but it needs option lookup by
  label and isn't needed for the first version.
- An interactive TUI or prompts. Every command takes all its input from flags.
- The local build. Its data lives in a browser, so there's no server for the CLI to reach.

## Commands

The binary is `upheap`, since it's the one people type. The server binary, which has that name today, becomes
`upheap-server` (see [Server changes](#server-changes)). Anyone who wants fewer keystrokes can alias it to `uh`.
Commands read as a short sentence, as the guideline asks:

```
upheap <category> <verb> [<required-input>...] [--<option> ...]
```

Every command is a category and a verb: `item`, `tag`, `heap` or `auth`, then what to do. The same verb means the
same thing in every category, so `upheap item list` and `upheap tag list` both list. It's more typing than top-level
item commands, but the tree stays the same everywhere. Shorter forms or aliases can come later, once usage shows which
are worth it.

| Command | What it does | API |
| --- | --- | --- |
| `upheap item add <title>` | Add an item | `POST /teams/{team}/heaps/{heap}/items` |
| `upheap item list [<text>]` | The ranked list, optionally narrowed by text | `GET /teams/{team}/heaps/{heap}/items` |
| `upheap item list --closed` | Closed items, most recent first | `GET /teams/{team}/heaps/{heap}/items/closed` |
| `upheap item show <item>` | One item in full | `GET …/items/{item}` |
| `upheap item edit <item>` | Change fields | `PATCH …/items/{item}` |
| `upheap item start <item>` | Move to in progress | `PATCH …/items/{item}` |
| `upheap item block <item> [--by <item>]` | Move to blocked, optionally naming the blocker | `PATCH …/items/{item}` |
| `upheap item done <item>` | Close as done | `PATCH …/items/{item}` |
| `upheap item wontdo <item>` | Close as won't do | `PATCH …/items/{item}` |
| `upheap item reopen <item>` | Move back to open, from any state | `PATCH …/items/{item}` |
| `upheap tag list` | Tags in the heap | `GET /teams/{team}/heaps/{heap}/tags` |
| `upheap tag add <name>` | Add a tag | `POST /teams/{team}/heaps/{heap}/tags` |
| `upheap tag edit <tag>` | Rename a tag or change its description | `PATCH …/tags/{id}` |
| `upheap heap list` | Every heap the caller can see, by team | `GET /me` |
| `upheap heap use <heap>` | Make a heap the default | none (writes config) |
| `upheap auth login` / `upheap auth logout` | Sign in through the browser; sign out | OAuth, see [Signing in](#signing-in) |

### Items

**`upheap item add <title>`** takes `--need` (`idea`, `nice-to-have`, `expected`, `critical`; `nice-to-have` when left out), `--description <text>` (`-` reads stdin), `--effort` and `--impact` (`s`, `m`, `l`, `xl`),
`--due <when>` and `--tag <name>` (repeatable). It prints the new item and where it landed: "Added *Rotate the staging
certs*, now 4th of 31". The position comes from one read of the ranked list after the add, so it's the server's
number.

**`upheap item edit <item>`** takes the same flags, plus `--title`, `--untag <name>`, and `--no-due`, `--no-effort`,
`--no-impact`, `--no-description` to clear a field. `--tag` and `--untag` add and take off single tags; the CLI reads
the item and sends the full `tag_ids` list the API expects. Nothing given means nothing to do: the command says so and
exits 0.

**State verbs.** `item start`, `item block`, `item done`, `item wontdo` and `item reopen` each send one `state` in a `PATCH`. One verb per
state reads better than `upheap item edit --state done`, and there's only one way to change a state. The server owns every rule:

- Moving to the state an item is already in changes nothing, so repeating a command is safe. The CLI reports "already
  done" and exits 0.
- `item reopen` on a closed item is how a closed item comes back; the one-week window is the server's
  (`reopen_window_expired`).
- `item done` on a won't-do item fails with `item_closed`. The CLI shows the server's message and the hint to reopen first.
- `item block --by <item>` sets `blocked_by_item_id` in the same request. Cycles and blockers from other heaps are refused
  by the server, as in the web app.

**`upheap item list`** shows pinned items, then the ranked ones, then grouped blocked items when shown, each row with its position,
key, title, state, effort and impact, due label, tags, and the top contributor. It takes the list query parameters as
flags with the same names and values: `--tag`, `--effort`, `--impact`, `--sort`, `--sort-parameter`,
`--open/--in-progress/--blocked <pinned|shown|hidden>` and `--blocked-layout`. `--view <name>` loads a saved view of
the heap and sends its settings, so team views work in the terminal without being edited there. The optional `<text>`
narrows the result by title and description on the client, which is the one kind of filtering the thin-client rules
allow. When `hidden_count` isn't zero, the footer says how many items are hidden and which flag shows them.

**`upheap item show <item>`** prints every field, the blocker, tags and parameter values. `--history` adds the item's audit
entries (`GET …/audit?entity_id=`), rendered from the server's `phrase` and `changes`.

### Naming an item

`<item>` is the item's key, the five characters the server gives every item and the web app shows in its links
([readable-ids.md](readable-ids.md#item-keys)): `upheap item done k3x9a`. A full path, `my-team/backlog/k3x9a`, names an
item in another heap; it's a short link without its `/h/`, so a pasted link works too. A key names the same item for good, in any shell
and on any machine, so the CLI keeps no cache of the last list. `upheap item list` prints each row's key beside its
position, and `--json` and `--quiet` print keys too.

A key that isn't in the heap is the server's `404`. The CLI says so and suggests `upheap item list`.

### Due times

`--due` takes either a rough choice or a date, matching the two kinds of due time in the
[requirements](requirements.md#items):

- **A rough choice** such as `next-week` or `"in a few weeks"`. The CLI fetches `GET /due-choices` and matches the
  label, ignoring case and treating `-` as a space. It sends the choice's date with `firm: false`, so the item reads
  back with the term it was picked by, as in the web app. An unknown choice fails and lists the valid ones.
- **A date**, `YYYY-MM-DD`: an exact deadline, `firm: true`.

Every request carries the `Time-Zone` header with the machine's IANA zone, so dates and labels match what the user
would see in the browser.

### Tags

`upheap tag add <name> [--description <text>]` and `upheap tag edit <tag> [--name <name>] [--description <text>]`.
`<tag>` is the tag's name (compared ignoring case, as the server compares them) or its ID. Tags can't be deleted, so
there's no `upheap tag delete`.

`--tag <name>` on `item add` and `item edit` refers to existing tags only. The web app creates a tag when you type a new name,
but in a terminal a typo would leave a tag nobody can delete. An unknown name fails with the closest existing names
and the `upheap tag add` command that would create it.

### Heaps

`upheap heap list` prints each team with its heaps, marks archived heaps and the current default, all from `GET /me`.

Every command that works in a heap takes the heap from, in order: `--heap <heap>`, the `UPHEAP_HEAP` environment
variable, the default set by `upheap heap use`. With none of them set, the command fails and says how to set one; it
doesn't guess. `<heap>` is `team/heap` in slugs, as in the web app's links: `eugene/personal`, `infra/backlog`. A
heap slug alone works when only one of the caller's teams has a heap by that slug.

## Output

1. **Human output** by default: aligned columns, colour when stdout is a terminal and `NO_COLOR` isn't set. Labels
   (due, contributors, audit phrases) are printed as the server wrote them.
2. **`--json`** prints the server's response body unchanged, so scripts get the documented OpenAPI types and the CLI
   adds no second schema to keep in sync. For `item add`, that's the created item.
3. **`--quiet`** prints only what a script needs next: the item ID for `item add`, nothing for a state change, IDs one
   per line for `item list`.
4. Results go to stdout; warnings and errors go to stderr.
5. Only the command's flow function writes output, through one `Output` value that `main` builds from these flags and
   passes down. API calls and lookups return values and errors; they don't print.

## Errors and exit codes

Errors come back as `application/problem+json` with a stable `code`. The CLI prints the message and the code
("Can't reopen: it was closed more than a week ago (reopen_window_expired)"), adds a hint when there's an obvious next
step, and exits with the code for the category, so scripts branch on the exit code or, with `--json`, on the problem
body printed to stderr.

| Exit | Meaning | From |
| :---: | --- | --- |
| 0 | Done, or nothing to do | |
| 1 | Unexpected: a bug in the CLI or a `500` from the server (with its `request_id`) | |
| 2 | The command line was wrong | clap, and checks that run before any request |
| 3 | The request was refused: bad input or a rule | `400`, `409`, `422`, and the CLI's own lookups (unknown tag, ambiguous item) |
| 4 | Not signed in, not allowed, or not found | `401`, `403`, `404` |
| 5 | The server couldn't be reached or is unavailable | connection errors, timeouts, `503` |
| 6 | The local setup is broken: unreadable config, no keychain | |

`version_conflict` is a `3`. The CLI doesn't retry it: someone else changed the item between the read and the write,
and the user should look before trying again. The message says so.

The categories mirror the server's own (caller, permission, conflict, dependency, unexpected), with permission and
not found together because the server deliberately doesn't tell them apart.

## Signing in

`upheap auth login [--server <url>]` runs the OAuth authorization code flow with PKCE that
[technical-design.md](technical-design.md#authentication-and-authorization) already describes for the CLI: it opens the
browser, listens on a loopback redirect URI, and receives an access token and a rotating refresh token. The refresh
token goes into the OS keychain; if there's no keychain, login fails with exit 6 rather than writing a token to a plain
file. `upheap auth logout` revokes the token on the server and removes it from the keychain.

None of this exists on the server yet ([progress.md](progress.md): "CLI sign-in: OAuth with PKCE" is not started), so
it's the first piece of work. See [Server changes](#server-changes).

## Configuration

One TOML file, `~/.config/upheap/config.toml` (the platform's config dir), with two settings: `server` (the base URL)
and `heap` (the default heap). `upheap auth login --server` and `upheap heap use` write them. Flags beat environment variables
(`UPHEAP_SERVER`, `UPHEAP_HEAP`), which beat the file.

## Code layout

A new crate, `crates/cli`, with the `upheap` binary. It talks to the server over HTTP and depends on nothing below `api`,
so it never links the database drivers or the service layer.

```
crates/cli/src/
  main.rs          parse args, build Output and the API client, dispatch, map errors to exit codes
  config.rs        config file and environment, read once at startup
  client/          the HTTP client: base URL, bearer token, Time-Zone header, problem+json errors
  auth/            the OAuth flow, keychain, token refresh
  lookup/          resolving <heap>, <tag> and due choices to handles, IDs and values
  commands/
    item/          add.rs  list.rs  show.rs  edit.rs  state.rs
    tag/  heap/
    auth/          login.rs  logout.rs
  output/          human and JSON rendering
```

Each command file holds its args struct (`item::AddArgs`) and its `run`, as the guideline asks. Shared flags (`--heap`,
`--json`, `--quiet`, `--server`) are one struct flattened into every command.

**API types.** The CLI must not hand-write the request and response types ([one source of
truth](engineering-principles.md#general)). They live in `api` today, next to axum. The plan is to move the DTOs into a
small `crates/api-types` crate (serde and utoipa only) that both `api` and `cli` use. That's a refactor with no
behaviour change, so it's its own pull request before the CLI. Generating a Rust client from `openapi.json` was the
alternative; the common generators target OpenAPI 3.0 and utoipa writes 3.1, and sharing the types the spec is
generated from is simpler anyway.

Libraries: `clap` (derive), `reqwest` with rustls, `tokio`, `serde`, `confique` for the config file, `keyring`,
`owo-colors`, `tracing` to a logfile. All are boring and widely used.

## Server changes

The CLI needs very little that the server doesn't already do:

1. **Rename the server binary to `upheap-server`**, so the CLI can be `upheap`. The server serves both the API and the
   web app, so `upheap-api` or `upheap-web` would each name only half of it, and `upheap-web` is already the web app's
   npm package. The crate stays `server`, so `cargo run -p server` in CI, Playwright and the README keeps working. What
   changes is the bin name in `crates/server/Cargo.toml`, the `Dockerfile` (build, copy and `ENTRYPOINT`), the doc
   comment in `crates/server/src/main.rs` and `CLAUDE.md`. Nothing has been released, so no one's scripts break. It's
   its own small pull request, ahead of the CLI.
2. **OAuth with PKCE**, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`, with `upheap` registered as a first-party
   public client. Already in the technical design; not built.
3. **Bearer tokens resolve to a `Principal`**, like session cookies do, and requests with a bearer token don't need the
   `X-CSRF-Protection` header, since CSRF is a cookie problem.
4. `/me/tokens`, so a user can see and revoke the CLI's tokens. Also already designed.

Everything else uses endpoints the web app uses today.

## Testing

1. Lookup (heap handles, due choices, tag names) is plain functions over plain values, unit tested.
2. Integration tests in `crates/cli/tests` run each command against the real API router on in-memory SQLite, the same
   way the API suite does, and check stdout, stderr and the exit code. They sign in through a test-only token, not the
   browser flow.
3. The OAuth flow is tested in the API suite, where it lives. The CLI's half (loopback listener, PKCE verifier) gets a
   unit test with a fake authorization server.

## Choices from the CLI guideline

The guideline lists eight conflicts between its sources. These are the ones that matter here:

1. **Printing:** a single `Output` passed down, not `println!` scattered through commands.
2. **Errors:** typed `thiserror` errors per layer, `anyhow` only in `main`, as the engineering principles already say.
3. **Exit codes:** categories with their own codes, as above, rather than `1` for every failure.
4. **Command shape:** `<category> <verb>`, with positional required inputs. The "named flags only" rule is for tools
   called by build actions, which `upheap` isn't.
5. **Layout:** `commands/<category>/`, one file per verb or group of related verbs, in a source tree, not `endpoints/`.

The rest (string interning, Bazel, support contacts in errors) don't apply to this repo.

## Open questions

1. **Creating tags from `--tag`.** This design refuses unknown names. If people find that annoying, a `--create-tags`
   flag is the cheap fix; matching the web app's behaviour silently is not.
2. **Several servers.** One `server` setting covers self-hosting. Profiles (`--profile work`) can be added later
   without changing any command.
