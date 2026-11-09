# CLI

Draft. A command-line client for the everyday work in a heap: add items, edit them, move them between states, list
what's in a heap, list heaps, and add and edit tags. Admin work (teams, members, heap settings, parameters) stays in
the web app for now.

The CLI is another client of the API, like the web app. It follows the [thin clients](engineering-principles.md#thin-clients)
rules, so it never ranks, never checks permissions and never works out due labels. The server already returns all of
that.

Command shape, errors, output and code layout follow [Conventions](#conventions) below, where this repo's
[engineering principles](engineering-principles.md) don't already decide.

## Goals and non-goals

Goals:

1. Adding an item is one short line: `upheap item add "Rotate the staging certs" --effort s --due next-week`.
2. `upheap item list` answers "what should I work on?" the way the web app's list does, with the same order and the same
   reasons.
3. Every command works in scripts: stable exit codes, `--json` with the server's own response, and keys on stdout
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
Commands read as a short sentence:

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

**`upheap item add <title>`** takes `--need` (`idea`, `nice_to_have`, `expected`, `critical`, the API's spellings as
everywhere in the CLI; `nice_to_have` when left out), `--description <text>` (`-` reads stdin), `--effort` and `--impact` (`s`, `m`, `l`, `xl`),
`--due <when>` and `--tag <name>` (repeatable). It prints the new item and where it landed: `Added "Rotate the staging
certs" (k3x9a), now 4th of 31`. The position comes from one read of the list after the add, so it's the server's
number; it is "of 2 pinned" or "of 2 blocked" when the item lands in one of those groups, and the read is skipped
with `--quiet` and `--json`. If the read fails the add still succeeded: the line comes without a position and a
warning follows.

**`upheap item edit <item>`** takes the same flags, plus `--title`, `--untag <name>`, and `--no-due`, `--no-effort`,
`--no-impact`, `--no-description` to clear a field. `--tag` and `--untag` add and take off single tags; the CLI reads
the item and sends the full `tag_ids` list the API expects. Nothing given means nothing to do: the command says so and
exits 0.

**State verbs.** `item start`, `item block`, `item done`, `item wontdo` and `item reopen` each send one `state` in a `PATCH` (`reopen` too, as `state: open`, not the reopen endpoint, so all five share one path). One verb per
state reads better than `upheap item edit --state done`, and there's only one way to change a state. The server owns every rule:

- Moving to the state an item is already in changes nothing, so repeating a command is safe. The CLI reports "already
  done" and exits 0.
- `item reopen` on a closed item is how a closed item comes back; the one-week window is the server's
  (`reopen_window_expired`).
- `item done` on a won't-do item fails with `item_closed`. The CLI shows the server's message and the hint to reopen first.
- `item block --by <item>` sets `blocked_by_item_id` in the same request. A bare key there is in the blocked item's
  heap, however that item was named. Cycles and blockers from other heaps are refused by the server, as in the web app.

**`upheap item list`** shows pinned items, then the ranked ones, then grouped blocked items when shown, each row with its position,
key, title, state, effort and impact, due label, tags, and the top contributor. It takes the list query parameters as
flags with the same names and values: `--tag`, `--need`, `--effort`, `--impact`, `--sort`, `--sort-parameter`,
`--open/--in-progress/--blocked <pinned|shown|hidden>` and `--blocked-layout`. `--view <name>` loads a saved view of
the heap and sends its settings, so team views work in the terminal without being edited there. The optional `<text>`
narrows the result by title and description on the client, which is the one kind of filtering the thin-client rules
allow. When `hidden_count` isn't zero, the footer says how many items are hidden and which flag shows them.
Flags given with `--view` replace that setting of the view. `<text>` doesn't go with `--json`, which prints the
server's list unchanged; `--closed` shows the first page of closed items (the newest 50) and takes no list flags. Values
are the API's own spellings (`nice_to_have`, `in_progress`), and `--tag` and `--sort-parameter` take names (or IDs).
`--sort-parameter` implies `--sort parameter`; with any other `--sort` it's exit 2, before any request. `--sort
parameter` needs the parameter, from `--sort-parameter` or the view's own, or it's exit 2 too.

**`upheap item show <item>`** prints every field, the blocker, tags and parameter values. `--history` adds the item's audit
entries (`GET …/audit?entity_id=`), rendered from the server's `phrase` and `changes`: the first page, with a footer
pointing to the web app when there are more. With `--json --history` the output is one object, `{"item": …,
"history": …}`, each holding the server's body unchanged.

### Naming an item

`<item>` is the item's key, the five characters the server gives every item and the web app shows in its links
([architecture.md](architecture.md#handles)): `upheap item done k3x9a`. A full path, `my-team/backlog/k3x9a`, names an
item in another heap; it's a short link without its `/h/`, so a pasted link works too: any `http` or `https` URL
ending in `/h/team/heap/key`, or in the long form the browser's address bar shows after the web app redirects,
`/teams/team/heaps/heap/items/key`. The host is ignored and requests go to the configured server. The path beats
`UPHEAP_HEAP` and the default, but `--heap` naming another heap than the path is exit 2, before any request. A key names the same item for good, in any shell
and on any machine, so the CLI keeps no cache of the last list. `upheap item list` prints each row's key beside its
position, and `--json` and `--quiet` print keys too.

A key that isn't in the heap is the server's `404`. The CLI says so and suggests `upheap item list`.

### Due times

`--due` takes either a rough choice or a date, matching the two kinds of due time in the
[product.md](product.md#items):

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
   (due, contributors, audit phrases) are printed as the server wrote them, except that every control character
   (C0, DEL, C1: escapes, newlines) in what goes to the terminal, stdout or stderr, is shown as U+FFFD. Titles and
   names come from other people through the server, which doesn't strip them, and an escape sequence would run in
   the reader's terminal (clear it, write the clipboard) or a newline forge a row. `Output` does it in one place,
   before adding its own colour codes; `--json` prints the body as it came.
2. **`--json`** prints the server's response body unchanged, so scripts get the documented OpenAPI types and the CLI
   adds no second schema to keep in sync. For `item add`, that's the created item. Piped, it's byte for byte as it
   came. On a terminal, DEL and the C1 controls (U+007F to U+009F), which JSON doesn't escape and a terminal may run,
   are written as `\u007f` to `\u009f`: the same JSON value, nothing for the terminal to interpret. The same goes for
   a problem body on stderr.
3. **`--quiet`** prints only what a script needs next: the item's key for `item add`, nothing for a state change, keys
   one per line for `item list`.
4. Results go to stdout; warnings and errors go to stderr. A reader that stops early (`upheap item list -q | head -1`)
   isn't an error: the command exits 0 without a message.
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
| 5 | The server couldn't be reached or is unavailable | connection errors, timeouts (a response cut off halfway too), `503`, and a proxy's `502` or `504` |
| 6 | The local setup is broken: unreadable config, no keychain | |

Signing in maps onto the same table: a cancel on the consent page, giving up waiting for the browser and a refused
code or refresh token are 4; no loopback port is 6. A heap name that isn't `team/heap` or a slug, a missing heap or
server, a plain `http` server elsewhere and a server that redirects are 2, as they're about what the CLI was told.

`version_conflict` is a `3`. The CLI doesn't retry it: someone else changed the item between the read and the write,
and the user should look before trying again. The message says so.

The categories mirror the server's own (caller, permission, conflict, dependency, unexpected), with permission and
not found together because the server deliberately doesn't tell them apart.

## Signing in

`upheap auth login [--server <url>]` runs the OAuth authorization code flow with PKCE that
[architecture.md](architecture.md#authentication-and-authorization) already describes for the CLI: it opens the
browser, listens on a loopback redirect URI, and receives an access token and a rotating refresh token. The refresh
token goes into the OS keychain; if there's no keychain, login fails with exit 6 rather than writing a token to a plain
file. `upheap auth logout` revokes the token on the server with `POST /oauth/revoke`, which ends the whole sign-in,
and removes it from the keychain. If the server can't revoke it (unreachable, or failing), the sign-in is kept so a
later try still ends it, and the error suggests `upheap auth logout --local`, which forgets it on this machine only and
says it may stay open on the server until its tokens expire. `POST /auth/logout` is the web app's: it ends only a session cookie, and answers `204`
for a bearer token without revoking it.

The server's half is built ([progress.md](progress.md)): the client is `upheap-cli`, its redirect URI is
`http://127.0.0.1:<port><path>` (or `[::1]`), and the endpoints and lifetimes are in
[architecture.md](architecture.md#authentication-and-authorization). See [Server changes](#server-changes).

How the CLI keeps the sign-in:

1. **One keychain entry per server**, service `upheap`, keyed by the server's URL: the refresh token, the last access
   token and when it expires, as JSON. `keyring` 4 picks the macOS keychain, the Windows credential manager, or the
   Secret Service over zbus, which is pure Rust, so building on Linux needs no libdbus. A Linux machine without a
   Secret Service daemon gets exit 6.
2. **The access token is reused** while it has more than a minute left, so most commands make no token request.
3. **Refreshing takes a lock.** Refresh tokens rotate, and presenting a spent one revokes the whole sign-in, so two
   `upheap`s refreshing at once would sign the user out. A refresh holds an exclusive lock on `tokens.lock` in the
   config directory and reads the tokens again once it has it; whoever waited finds the new pair. A refused refresh
   (`invalid_grant`) forgets the sign-in.
4. **Login replaces an earlier sign-in** to the same server, doing what can fail first and the destructive step
   last: it reads `/me` with the new access token, writes the config file, saves the new tokens (under the token
   lock, after reading the stored ones again, so it can't race a refresh), and only then revokes the earlier sign-in,
   best effort. Any failure up to and including the save revokes the new tokens, which never reached the keychain,
   and leaves the earlier sign-in working, so nothing new is left open on the server. Likewise a refresh whose new tokens the keychain won't take revokes them, and
   says to sign in again (exit 6). `--server` is written to the config file once signed in; when it
   changes the file's server, a `heap` set for the old one is cleared, as a heap is one server's, and login says so.
5. **The loopback listener** reads each connection in its own task, with a 10-second limit on the request line,
   because browsers open spare connections (preconnects) that send nothing. It checks `state` itself: a redirect with
   another `state` gets the not-signed-in page and the wait goes on; only the sign-in's own redirect gets the
   signed-in page.
6. **A keychain entry the CLI can't read** (another program's, or another version's) doesn't lock the user out:
   `auth login` overwrites it (there's nothing to revoke), `auth logout --local` forgets it, and other commands fail
   with exit 6 and a hint naming those two.
7. **Tokens go only where they were sent.** A plain `http://` server is refused (exit 2) unless it's on this machine
   (`localhost`, `127.0.0.0/8`, `::1`). Requests to a server on this machine ignore `HTTP_PROXY`, `HTTPS_PROXY` and
   `ALL_PROXY` (reqwest doesn't leave loopback out by itself), so plain-`http` tokens never cross to a proxy; other
   servers use a proxy as the environment says. And the HTTP client follows no redirects: a `3xx` is an error (exit 2) naming
   where the server pointed, so a form with a refresh token or code verifier is never re-sent to another host.

## Configuration

One TOML file, `config.toml` in `upheap` under the platform's config dir (`~/.config` on Linux, `~/Library/Application
Support` on macOS), with two settings: `server` (the base URL)
and `heap` (the default heap). `upheap auth login --server` and `upheap heap use` write them. Flags beat environment variables
(`UPHEAP_SERVER`, `UPHEAP_HEAP`), which beat the file. `upheap heap use` writes the heap as `team/heap`, after checking
it against `GET /me`; when the server it used (`--server`, `UPHEAP_SERVER` or the file) isn't the file's, it writes
that server too, so the heap and its server stay a pair, and says so. Each write holds an exclusive lock on `config.lock`
in the same directory and reads the file again under it, so two `upheap`s changing different settings at once both
keep theirs, and goes to a temporary file in the same
directory that's then renamed over it, so nobody reads half a file. The file is read and written with `toml` and serde; `confique` only reads config, and the CLI
writes it too. Unknown settings are refused, not ignored: the error names the setting, and the hint says to upgrade
`upheap` if a newer one wrote it. With `RUST_LOG` set, the CLI logs to `upheap.log` in the same directory.

## Code layout

A new crate, `crates/cli`, with the `upheap` binary. It talks to the server over HTTP and depends on nothing below `api`
but `domain`, through `api-types`, so it never links the database drivers or the service layer.

```
crates/cli/src/
  main.rs          gather the environment (Deps::system), start the log, call cli::run
  lib.rs           parse args, build Output, dispatch, report the error and its exit code
  error.rs         every failure, its message, hint and exit code
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

Each command file holds its args struct (`item::AddArgs`) and its `run`. The crate is a library
with a thin binary: `main` gathers the environment (`Deps::system`) and calls `cli::run(args, deps)`, which returns the
exit code. `Deps` holds the environment, the token store, the browser, stdin and the two output streams, so tests pass an
in-memory store, a browser that answers the consent page itself, and buffers. Shared flags (`--json`, `--quiet`,
`--server`) are one struct flattened into every command; `--heap` is a second one, flattened only into the commands
that work in a heap (every item command), so it doesn't show where it would mean nothing.

**API types.** The CLI must not hand-write the request and response types ([one source of
truth](engineering-principles.md#general)). They are in `crates/api-types`, which both `api` and `cli` use. It has no IO
and depends on nothing in the workspace but `domain`, so it keeps the conversions between the types and domain types; conversions from the
service layer's views stay in `api` as plain functions. The move was a refactor with no behaviour change, its own
commit before the CLI's.

**Values a later server adds.** Adding a value is an additive change ([engineering
principles](engineering-principles.md#api), 3), so a CLI built before it must still read the response. The
`allowed_actions` lists of every response type skip values the client doesn't know (`deserialize_with`), and
`Factor` (contributors) and `ClosedGroup` read an unknown value as `Unknown`, a variant marked
`#[serde(other, skip_serializing)]`, which keeps it out of the OpenAPI schema and can never be sent. The CLI prints the
contributor's `label`, so an unknown factor reads as the server wrote it. Values the server reads in requests stay
strict: `Need`, `Size`, `ItemState`, `Role`, and the list view's `StateMode`, `BlockedLayout` and `ListSort`, which saved
views share with the list query. A new sort or state mode in a saved view would still stop an older CLI reading the
heap's views; that's left for when it happens, as making it lenient would need a second type. Generating a Rust client from `openapi.json` was the
alternative; the common generators target OpenAPI 3.0 and utoipa writes 3.1, and sharing the types the spec is
generated from is simpler anyway.

Libraries: `clap` (derive), `reqwest` with rustls (ring, as the server's database drivers use), `tokio`, `serde`,
`toml` for the config file, `keyring`, `webbrowser`, `owo-colors`, `tracing` to a logfile. All are boring and widely used.

## Server changes

The CLI needs little from the server beyond what the web app uses; which of it is built is in
[progress.md](progress.md):

1. **The server binary is `upheap-server`**, so the CLI can be `upheap`. The server serves both the API and the web
   app, so `upheap-api` or `upheap-web` would each name only half of it, and `upheap-web` is already the web app's
   npm package. The crate stays `server`, so `cargo run -p server` in CI, Playwright and the README keeps working.
2. **OAuth with PKCE**, `/oauth/authorize`, `/oauth/token`, `/oauth/revoke`, with `upheap-cli` registered as a
   first-party public client, and the web app's consent page at `/oauth/authorize`.
3. **Bearer tokens resolve to a `Principal`**, like session cookies do, and requests with a bearer token don't need
   the `X-CSRF-Protection` header, since CSRF is a cookie problem.
4. **`/me/tokens`**, so a user can see and revoke the CLI's tokens.

Everything else uses endpoints the web app uses today.

## Testing

1. Lookup (heap handles, due choices, tag names) is plain functions over plain values, unit tested.
2. Integration tests in `crates/cli/tests` run each command against the real API router on in-memory SQLite, served
   on a localhost port, and check stdout, stderr and the exit code. They sign in through the real OAuth endpoints:
   most seed the token store with a code they approved and exchanged, and the login tests run `upheap auth login`
   with a browser that answers the consent page.
3. The OAuth flow is tested in the API suite, where it lives. The CLI's half (loopback listener, PKCE verifier) gets a
   unit test with a fake authorization server.

## Conventions

Where common CLI conventions pull in different directions, these are the choices here:

1. **Printing:** a single `Output` passed down, not `println!` scattered through commands.
2. **Errors:** typed `thiserror` errors per layer, `anyhow` only in `main`, as the engineering principles already say.
3. **Exit codes:** categories with their own codes, as above, rather than `1` for every failure.
4. **Command shape:** `<category> <verb>`, with positional required inputs. The "named flags only" rule is for tools
   called by build actions, which `upheap` isn't.
5. **Layout:** `commands/<category>/`, one file per verb or group of related verbs, in a source tree, not `endpoints/`.

## Open questions

1. **Creating tags from `--tag`.** This design refuses unknown names. If people find that annoying, a `--create-tags`
   flag is the cheap fix; matching the web app's behaviour silently is not.
2. **Several servers.** One `server` setting covers self-hosting. Profiles (`--profile work`) can be added later
   without changing any command.
