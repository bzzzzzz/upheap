# Architecture

How Upheap is built, and why. Companion to [product.md](product.md). The coding rules are in
[engineering-principles.md](engineering-principles.md), and the choices that could have gone another way, with
their reasons, are in [decisions.md](decisions.md).

This doc explains mechanisms and reasons. What the code states, it doesn't repeat: the endpoints are in
`openapi.json` (`upheap-server openapi` prints it), the error codes in `crates/api/src/error.rs`, the schema in
`crates/store/src/migration`, the configuration variables in
[configuration.md](../operator/configuration.md) and the crate layout in [README.md](../../README.md#layout).

## Principles

1. **Thin clients.** The server does all computation: ranking, contributors, filtering, sorting, fuzzy due labels,
   permission checks, and state rules such as whether an item can still be reopened. Clients render what the API
   returns and send intent back. They do two things themselves: text search over the list they already hold
   ([Search](#search)) and animation. A new client, such as the CLI, needs nothing beyond HTTP and the generated
   API types.
2. **The server says what's allowed.** Responses carry `allowed_actions`, so clients never re-implement role or
   state rules to decide which controls to show.
3. **Database-agnostic.** Portable SQL only: no JSON column types, no database-specific full-text search, no
   triggers or stored procedures. Everything runs on SQLite, MySQL and Postgres, and CI tests all three.
4. **Nothing is deleted, and everything is audited.** No `DELETE` of domain data. Every write and its audit entry
   commit in one transaction.
5. **Single binary.** One executable serves the API and the embedded web app.

## Stack

| Area          | Choice                                                                                                                                                                                                                                                                                                                      |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Backend       | Rust (stable), a Cargo workspace                                                                                                                                                                                                                                                                                            |
| HTTP          | axum on tokio, tower middleware                                                                                                                                                                                                                                                                                             |
| API contract  | Code-first OpenAPI 3.1 with utoipa. The web app's TypeScript types are generated from it with openapi-typescript, and CI fails on drift                                                                                                                                                                                     |
| Data access   | SeaORM 2 over sqlx, with the backend chosen at startup from `DATABASE_URL`: SQLite, MySQL or Postgres                                                                                                                                                                                                                       |
| Migrations    | sea-orm-migration, embedded in the binary and run at startup                                                                                                                                                                                                                                                                |
| Auth          | argon2id for local passwords; an own session table with hashed random tokens for the web app; own OAuth 2.0 authorization-code endpoints with PKCE for non-web clients, and personal tokens for machines with no browser. One OpenID Connect provider per server, through the `openidconnect` crate, next to local accounts |
| Web app       | React, Vite, TanStack Query and Router, Tailwind, React Aria Components, Motion; embedded in the binary with rust-embed                                                                                                                                                                                                     |
| Observability | `tracing` with JSON logs, request IDs, `/healthz` and `/readyz`                                                                                                                                                                                                                                                             |
| Testing       | `cargo test` with googletest matchers, proptest for ranking invariants, MySQL and Postgres service containers in CI, Playwright end to end                                                                                                                                                                                  |
| Packaging     | A static binary in a minimal Alpine image; docker-compose with SQLite on a volume; Cloud Run with Neon Postgres in production; the CLI and the server as static binaries on GitHub Releases, with a Homebrew cask for the CLI ([operations.md](../operator/operations.md))                                                  |

## Workspace

Dependencies point one way: `server → api → service → store, domain`, with `api → api-types → domain` and
`server → mail` beside it, and `service → mail` for the mail it sends. `domain` depends on nothing in the
workspace, so ranking and the state rules are unit- and property-tested in isolation, and `store` and `api` never
depend on each other. `api-types` holds the request and response types with no IO, so a Rust client such as `cli`
shares them without linking the server. It copies each domain enum as a type of its own on purpose: the API
tolerates values it doesn't know in what it only reads, leaves some values out, and its list of values is the
reviewed contract. The conversions are exhaustive `match`es, so a new value fails the build until it is added, and
a macro declaring the copies was tried and dropped because it would hide that contract. In `domain`, an enum whose
codes are stored is declared with `coded_enum!`, which exists for the one thing a `match` can't check: that the list
of every value holds every variant. Files that nearly every feature edits (`domain::permission`, `api/src/error.rs`,
the access control table test) are meant to be: the build or the test fails until the feature is classified there.
Each crate's root module says what it owns.

## Data model

The tables and columns are in the migrations. These are the conventions they follow.

- **IDs** are UUIDv7 in `CHAR(36)`: sortable by creation time and safe to expose.
- **Handles** name things in URLs, API paths and the CLI ([Handles](#handles)). They are extra columns with unique
  indexes, and every foreign key still points at the UUID, so a handle can change without touching references.
- **A reminder's arrival** is stored, not worked out from its due: the first moment of its date in the time zone of
  the request that set it, since a day that changes the clocks is not 24 hours long. It is kept when the reminder
  becomes a task, and ranking counts an item's age from it, so the task's age still counts from the reminder.
- **An item's origin** is the note it was split from, or the item of the run that added it. Only the first kind is
  what the note became, so a follow-up a run adds doesn't count toward what the note made. Both are set when the
  item is made and never changed, and stay when either item moves, as a blocker's link does; a reader sees the
  other end only where they may view its heap.
- **Timestamps** are UTC Unix milliseconds in `BIGINT`, which sidesteps the differences between the databases'
  date types. No timezone is stored anywhere ([Time and timezones](#time-and-timezones)).
- **Structured data** (audit diffs, saved list views) is JSON in `TEXT`, never queried inside the database.
- **Archiving** is a nullable `archived_at`. There are no hard deletes, and foreign keys use `ON DELETE RESTRICT`,
  so even an accidental `DELETE` fails instead of removing related rows. Relationship rows follow the same rule:
  taking a tag off an item sets `item_tags.removed_at`, and clearing a parameter value sets its `option_id` to
  `NULL`. Team membership is the one exception: removing a member deletes the `team_members` row, and the audit log
  keeps the history. The log is also where a removal is remembered: joining through an invite link reads the person's
  last removal, so a link made before it doesn't let them back in.
- **Optimistic concurrency:** every mutable row carries a `version`. Updates require the expected version and fail
  with `409` otherwise. A parameter is one aggregate: its options and tag links have no version of their own, and
  every change to them takes the parameter's version.
- **A task's steps are rows of their own**, each with its own `version`: a step write never bumps the item's, and an
  item write never bumps a step's, so an agent ticking steps never makes an open edit of the item conflict. A step's
  `number` is its name (`k3x9a.3`) and is never changed or reused; its place in the order is kept apart, so
  reordering never renames a step. Steps are never deleted: a dropped one stays. A task has at most 100, which is
  the size assumption that lets a single-item response carry all of them; lists carry only the progress. A step
  write takes the heap lock like any item write ([Cycle detection and locking](#cycle-detection-and-locking)). A
  step or a heap made from a note keeps the note as its origin, as an item does.
- **An item's questions are rows of their own** in `item_questions`, built like steps: a question write never bumps
  the item's `version` and an item write never bumps a question's, and a question's `number` is never reused.
  Questions are never deleted: an answered one keeps its reply, and closing an item withdraws its open ones in the
  same transaction, so nothing waits for the person on a question no one can answer. Whether an item waits for the
  person is derived, never stored: a single-item response reads it from the questions it carries, and a list from
  one grouped count of open questions per listed item, as step progress is. A request, a review and a done proposal
  are questions whose choices the server fixes, so every kind shares one reply shape. An agent never closes a task:
  it proposes closing, and the person's Close as done closes the task in the reply's transaction, by the same path
  as their own Done.
- **An item's comments are rows of their own** in `item_comments`: an append-only thread under the item's context,
  so an agent or a person adds to an item without rewriting its context, which could change the author's words and
  clash with their edits. A comment has no `version` and is never changed or deleted, and adding one never bumps the
  item's `version`, so it never conflicts with an edit and needs no `If-Match`. A comment written in a run keeps the
  run, so clients show it as the agent's without reading the history.
- **An item's turns are rows of their own** in `item_turns`: an item handed to an agent, for one stable state or all
  the way to done. A turn write never bumps the item's `version`, so sending an item and taking it back never
  conflict with an edit and need no `If-Match`, and turns are never deleted. An item has at most one open turn,
  which the service keeps under the heap lock every item write takes, so no constraint is needed; starting or
  closing the item, or archiving its heap, takes the open turn back in the same transaction. A turn sent all the way
  still goes one stable state at a time, but when a run reaches it the next turn opens in the same transaction, so
  the item never has a moment with the person between stages. The turns of one send share its `send_id`, which is
  how the queue knows a send was worked on before. Who has an item (`with_agent`) and a listed item's agent state are
  derived from its turn, the turn's runs and its open questions, never stored. A heap's prepare setting and a send
  made while adding both open their turn in `create_item`'s transaction, so an item is never added unsent and never
  gets two turns. The request's steps are written in that transaction too, before the turn, so an agent never finds
  the task without them.
- **Runs** are rows of their own in `item_runs`: one go an agent has at a turn, claimed from the queue. Like turns,
  they are never deleted and never bump the item's `version`.
  **There is no lock to give up: a claim lapses.** A run is live while it hasn't ended and was heard from, by a
  heartbeat or a write made in it, less than `CLAIM_LAPSE` ago. Past that it counts as ended, failed, wherever it is
  read, and is written as failed only when something next touches it (a claim over its heap, its own `finish`, a
  take back); no job sweeps for them, so an agent that dies costs nothing but the wait. A turn is stuck after
  `STUCK_AFTER` failed runs in a row, and every read of a turn counts a lapsed run not yet written
  (`Turn::counting_lapse`), so a turn whose last run went quiet is stuck at once, everywhere. A stuck turn leaves the
  queue and waits for the person.
  **A write in a run** carries `Upheap-Run`. What a run may write is listed in one place, `RUN_ALLOWED_WRITES` in
  `crates/api/src/extract.rs`, and every other write is the person's. The service then checks, under the heap lock,
  that the run is the caller's, live, with its turn open, and that the item is its own or one it made. Ending a run
  is one transaction with what it reports (the item's stage, a note closing as became, the next turn of a send), so
  a run that dies half-way leaves nothing half-done.
  **A watch** lets a run wait on a pull request without holding a claim: the run ends, and the turn stays open with
  the address on it, out of the queue but not waiting for the person. The server never reaches GitHub: the loop that
  runs the agents reads each watched pull request with the person's own `gh`, and fires the watch when something
  came, which adds a comment and puts the item back in the queue.
- **State and `closed_at`:** in the database, `closed_at` and `closed_by` are set exactly when the state is closed
  (done, won't do or became). In `domain`, the state is an enum where only the closed variants carry them, so an
  open item with a close time can't be built. The store converts between the two and rejects rows that break the
  rule.
- **A tag is owned by a heap or by a team**, in one table, so everything that names a tag (items, parameter links,
  saved views, tag orders) does so by ID with one foreign key, and a team tag needs no copy per heap: a rename is
  one row written. A heap's tags are its own and its team's.
- **A parameter is owned by a heap or by a team.** A heap's use of a team parameter is a row of its own with the
  heap's weight, rather than a weight on the parameter, which would be one number for every heap; stopping keeps the
  row, so the weight survives using it again. Everything that reads a heap's parameters (ranking, values, filters,
  views, moves, the audit's names) sees a used team parameter as the heap's own, and one the heap doesn't use isn't
  there. A heap's own tag links to a team parameter are in a table of their own: the link belongs to the heap, and
  must stay the heap's when its tag becomes a team tag. Those links carry no version, so two heaps' link writes that
  race both take effect, and the write needs only a shared lock on the parameter.
- **Case-insensitive uniqueness** (tag names per owner) is a lowercased key column with a unique index rather than a
  collation, because MySQL compares case-insensitively by default and SQLite doesn't. Where an index can't express
  the rule (a parameter name unique only among those in use, a name unique across a team's tags or parameters and
  its heaps'), the service checks under the locks of [Cycle detection and locking](#cycle-detection-and-locking).
- **A merged heap tag** keeps its row and names the team tag it was merged into. Its name key is changed so the name
  is free for a new heap tag, and its name stays, so the history from before the merge still names it. Only the
  history reads merged tags.
- **A merged heap parameter** is archived, once its items' values have moved to the team parameter. That keeps its
  row and options for the history, frees the name, and can't be undone. The parameter a merge starts from becomes
  the team parameter in place, so its heap's items, views and history need no change.
- **Insert-only tables:** the application can only insert into `audit_log`. There is no update or delete code path
  for it.
- **The heap's ranking policy** (its factor weights, its parameters' weights and its tag order) is saved through one
  endpoint, in one transaction under the heap's version, because a weight only means something next to the others
  and separate writes could leave the policy half-saved. A parameter's own edit still sets its weight alone under
  the parameter's version; making every parameter edit move the heap's version would close that gap but make
  renames conflict with parameter edits. The tag order is a list on the heap rather than a rank on each tag, so a
  team tag sits at its own place in each heap's order. Effort, impact and need are item columns rather than custom
  parameters, because they are fixed and apply to every heap
  ([decisions.md](decisions.md#6-effort-impact-and-need-are-item-columns-not-custom-parameters)).
- **Archived options always name their replacement**, so a stored value can always be followed to an option in use.
- **Invite links keep their token readable**, unlike session tokens and OAuth secrets, which are stored hashed: a
  link is sent around for a week and admins copy it again, so the server must be able to show it. A leaked database
  therefore holds working links, each worth a membership for at most a week; revoking one ends it. A link's end is
  derived from its creation, not stored.
- **Portable SQL in practice.** The differences met so far, which new SQL has to respect: in `ON CONFLICT DO
  UPDATE` Postgres refuses a bare column name that could mean the existing row or `excluded`, so upserts name
  their columns with the table; Postgres pads `char(n)` and reads it back padded, so every `char` column holds
  values of exactly its length; identifiers are quoted per backend; SQLite's `LOWER` lowercases ASCII only and
  Postgres's all of Unicode, so a comparison lowercases both sides in SQL, never one side in Rust; and after any
  failed statement Postgres rejects every later statement in the transaction, so a use case that catches a conflict
  and carries on in the same transaction must use a savepoint (none does today).

## Handles

UUIDs are good keys but bad names: nobody can read `/heaps/0196f3a2-…/items/0196f3b8-…` or type it into a
terminal. So every user, team, heap and item also has a short handle, used in URLs, API paths and the CLI, while
the UUIDs stay the database keys.

| Thing | Handle   | Unique within               | Example   | Who sets it                                |
| ----- | -------- | --------------------------- | --------- | ------------------------------------------ |
| User  | username | everyone, shared with teams | `eugene`  | the user, once, at sign-up                 |
| Team  | slug     | everyone, shared with users | `my-team` | admins; derived from the name if not given |
| Heap  | slug     | its team                    | `backlog` | admins; derived from the name if not given |
| Item  | key      | its heap                    | `k3x9a`   | the server, at random                      |

Display names, team names and heap names stay free text that can repeat. Only handles are unique.

1. **A username is the slug of its user's personal team**, so one unique index covers both and `/teams/eugene` is
   always Eugene's personal team. The personal team is private: it has exactly one member and refuses members,
   removal and leaving, and its slug can't change, since usernames never do. Anything shared lives in another team.
2. **Format.** Input is checked, never fixed silently: `My-Team` is refused with `my-team` as the hint. A team or
   heap created without a slug gets one derived from its name, with `-2`, `-3`… where that's taken. Two creates of
   one name at once can both find the same slug free; the one the unique index refuses starts again with the next
   suffix, so only a slug the caller chose is ever refused as taken.
3. **Item keys** are five random characters, easy to read aloud and type, that can't spell real words, and say
   nothing about when an item was made or how many a heap has. The key is drawn under the heap lock every item write
   takes and checked against the unique index; a clash is retried. The service draws the bytes from the random
   source it's given, like its clock, so tests can script them. An item that moves keeps its key unless the target
   heap has an item with it.
4. **Nothing is reserved.** Every handle sits after a fixed word in the URL (`teams/`, `heaps/`, `items/`), so it
   can't clash with a page the app has now or adds later. Teams start at `/teams/` rather than at the root because
   the root holds the app's own pages and the server's paths (`/api`, `/healthz`, `/oauth`). API paths have the
   same shape under `/api/v1`, so a web URL becomes its API call by adding the prefix.
5. **Short links.** `/h/my-team/backlog/k3x9a` is the short form, one handle per position: team, then heap, then
   item. The web app's router replaces it with the full URL as it opens; the API has none. The CLI takes the same
   path without its `/h/`, so a pasted link works too.
6. **Changes.** Usernames are fixed, and an item's key changes only when it moves to a heap that has the key. Team
   and heap slugs can change. A move keeps the item's former handles, so an old item path still names it where it is
   now, unless another item holds that handle now; it is read against the heap it is in now, so after a move to
   another team it answers `404` to whoever can't view that heap. A renamed team or heap slug still breaks old links.
7. **Sign-up** asks for a username, which the personal team takes as its slug. A first OIDC sign-in asks for one
   before creating the user.

## Ranking

### Computation

Ranking is a pure function in `domain`: the heap's policy, its non-closed items with their parameter values and
blocking edges, and an `as_of` time go in; positions, contributors and held-back entries come out. Nothing about
scores is stored. The module doc of `domain::ranking` explains the formula.

1. The service loads everything the heap needs and ranks in memory on each request. Heaps are expected to hold at
   most a few thousand open items, so this is cheap. A per-heap cache invalidated on write can be added if
   measurements call for it.
2. Each factor produces a weighted component, and the score combines them. The formula is a named version behind
   one function, set in code rather than stored per heap: tests pin orderings per version, so a formula change is an
   explicit test update, and since scores are never stored, switching versions needs no migration.
3. **Blocker inheritance:** an item's effective score is the greater of its own and the boosted effective scores of
   the items it blocks, computed by one memoized walk, so it's linear in the number of items and carries through
   chains. Ties go to the item blocking the longer chain, so a blocker never ranks below what it blocks, even when
   every weight is zero.
4. **Contributors** are the largest weighted components, each with a label and a relative share ("Deadline in a few
   days", "Blocks _Migrate auth_"). Scores themselves are never returned. **Held back by** names what the person
   could change to lift an item. The service empties it for the item listed first in its group, which the item pane
   calls "Why it's #1", and for every item in a group sorted by something other than priority, where those fields
   don't move it.
5. **Ties** that the chain length doesn't settle go to the older item, then to the ID, so the order is fully
   deterministic.
6. **Placement** (product.md, Priority §8) is the one stored input that isn't a field: a multiplier on one item. On a
   drop the client sends the anchor, the side and its list's `as_of`, and the service ranks the heap at that `as_of`
   without the item's own placement to work the multiplier out. The web app only offers it in the ranked group of a
   heap's priority list, since the API can't know the view a client shows.

A ranking preview ranks the heap with draft weights beside the stored ones, at one `as_of`, and stores nothing.

### Across heaps

`GET /me/items` is the ranked list across every active heap the caller may view. Each heap is ranked and grouped by
the same code as its own list, so a row is what that list would show. Then each group is merged across heaps by the
item's score in its own heap times the caller's **heap priority** for that heap, the same rule Waiting for me and the
agents' queue order by. A heap priority is one person's own number for one heap, so teammates in a shared heap each
order it their own way. Since it is the same positive factor for all of a heap's items, the merge keeps every heap's
own order. A change to it is audited in the person's personal team, not the heap's, because the value is private and
the heap's log is read by the team. A heap made from a note starts with a copy of the note's heap's priorities, so it
sits where its source did for everyone; the copy isn't a change anyone made, so it is not audited.

Filtering picks rows after each heap's ranking, so the merged order is the unfiltered one with the non-matching rows
taken out. Tags and parameters are a team's, so they filter only with a `team`, which also limits the list to that
team's heaps before any is ranked. A team the caller can't see answers as every handle lookup does, so teams can't
be probed. The list has a cap instead of paging, under the size assumption in `MAX_ACROSS_HEAPS_ROWS`.

The closed items and the history across the same heaps are not merges of per-heap reads: one query reads a page
over all the heaps, paged by the cursor a single heap's read uses, and each row is then built as its own heap builds
it. The history reads each team's heaps with the actions the caller's role in that team may see.

### Session-stable order

The score is a deterministic function of item data and time, so a stable session needs no server-side state, only
a fixed time.

1. The first list request returns an `as_of` timestamp with the ranked list. The client sends the same `as_of` on
   later requests: refetches after an edit, filter changes.
2. Items whose data hasn't changed keep their positions, because the time input is frozen. An edited item is ranked
   with its new data, so it moves right away, and so do teammates' edits on the client's next refetch, which happens
   on focus and on a short interval (server push can replace polling later). The web app animates every move.
3. Reloading drops `as_of`, so the list re-ranks at the current time. The server ignores an `as_of` more than about
   a day old.
4. Every label in the list (due, created, closed) is worked out at `as_of` too, so the list reads as one moment.
   Responses for a single item use the current time.

### Search

1. The ranked list endpoint returns every open item of the heap in one response, with no pagination. At the
   expected size that's fine, and it's what keeps client-side search and stable positions simple.
2. The client searches text itself, over the titles and descriptions it already has, keeping the ranked order and
   making no request per keystroke.
3. Filters and sorts stay on the server, because they change which items are pinned and how they are grouped.
   Ranking always runs over every non-closed item, since a blocker inherits from what it blocks even when that item
   is filtered out or hidden; the view then picks, groups and sorts the result, and reports what it left out, so a
   client can say "6 of 14" without a second request. The view's defaults are the server's, so every client starts
   from the same list. Archived parameters and options are accepted in filters and sorts, so a saved view keeps
   working after an option is archived.
4. **Reminders to come** are not ranked, and are split off at the server's clock, not at the time a list ranks at,
   so a reminder whose date comes while a list is open joins it on the next fetch. No list, queue or open count sees
   them. They are not hidden from search: a client that searches asks for them as a group of their own.
5. The web app keeps the view in the URL, so it survives reloads and can be shared, but not the search text, which
   is a momentary narrowing. Saved views store the same settings under the query parameters' names, so a client
   turns one into a list query field by field.
6. **Search across heaps** finds items by title or key in every heap the caller may view, closed items included,
   which the lists can't. It is `LIKE`, not full-text, by
   [decision 12](decisions.md#12-portable-sql-over-per-database-features): a person sees thousands of items at most,
   and the scan is narrowed to their heaps by the heap index. Case folding is the database's `LOWER`, so it is ASCII
   only on SQLite.
7. If heaps grow beyond this size, the list's search moves to the server with per-database full-text indexes behind a
   `q=` parameter; if a person's items outgrow `LIKE`, the search across heaps moves to full-text indexes too.

### Time and timezones

1. All instants are stored and sent as UTC timestamps. The server stores no timezone, for users or anywhere else.
2. Clients send the device's IANA timezone on every request in a `Time-Zone` header. The server uses it for
   presentation values: fuzzy due labels, formatted dates, "overdue", the closed list's groups. A missing header
   means UTC.
3. Dates without a time, such as a due date, are stored as the end of that day in the request's timezone, converted
   to UTC.
4. The one-week reopen window is exactly 7 × 24 hours from `closed_at`, so it doesn't depend on any timezone.
5. The server lists the rough due choices a picker offers, each with its date in the request's timezone and the
   label it reads back with, so "Next week" can never come back as "in a few days".

### Cycle detection and locking

1. **One lock order.** Every item, parameter and saved-view write first locks the heap row, before touching any
   other row, so writes in one heap run one after the other and always take locks in the same order. The write
   checks that the heap isn't archived under that lock, and archiving takes the same lock. Reads that a rule depends
   on are locking reads too, because on MySQL a plain read before the lock would fix the transaction's snapshot and
   hide changes that committed while it waited. SQLite has one writer at a time, so there the second of two
   conflicting writes fails instead of waiting; no inconsistent state holds either way.
   A write that reaches every heap of a team (a team tag or team parameter write, which may merge or archive across
   the heaps) locks each heap of the team in heap ID order. Locking the team row instead would deadlock with an item
   write, which holds its heap row and then needs the team row's key for its audit entry. A heap tag or heap
   parameter write first takes a shared lock on the team's lowest heap, so it meets a team-wide write there even in
   a heap made after that write read the team's heaps; heap IDs rise with time, so the lowest stays the lowest. A
   merge holds every heap lock of the team while it works, which is fine for merges of up to a few hundred items:
   that is the size assumption. Name checks take no locking reads, which would take gap locks that deadlock the tag
   writes of two teams. `service::access` has the details.
2. **A move** to another heap locks both heaps in heap ID order, as the agent's queue locks several, so two moves in
   opposite directions can't wait for each other. The items that waited for the moved item keep it as their blocker,
   so they wait for the person (§3), and a move back makes it hold them again. A blocker in another team than the
   item it blocks is never named, to anyone. Since an item's heap can change, a write that finds the heap through a
   read before the lock reads the item again under the lock and refuses when it has left that heap.
3. **Blocking.** Setting a blocker walks the chain inside that transaction and refuses a request that would reach
   the item itself. A blocker must be an open item of the same heap when it is set, and leaving the blocked state
   clears it. Closing a blocker as done frees what it blocks, in the same transaction and each with its own audit
   entry. Closing it as won't do frees nothing: an item whose blocker is closed that way, or is in another heap after
   a move (§2), stays blocked and waits for the person until they unblock it, which is read off the item and its
   blocker, never stored.
4. Ranking still tolerates a cycle in stored data, by ignoring the edge that closes it, so one bad row can't break
   the list.

## API

1. Everything is under `/api/v1`, JSON over HTTPS. The spec is `openapi.json`.
2. Errors are `application/problem+json` with a stable `code`, mapped in one place (`crates/api/src/error.rs`),
   where every error variant must be classified or the build fails. The exception is the OAuth token and revoke
   endpoints, which refuse in the RFC 6749 format so any OAuth client library understands them.
3. Paths name things by handle. Tags, parameters, options and views keep their IDs, checked against the heap in the
   path. A handle that's malformed or names nothing the caller can see is `404`. Responses carry both the `id` and
   the handle; request bodies refer to other things by ID, since clients have the objects in hand.
4. Updates are `PATCH` with partial bodies and the expected `version` in `If-Match`. Endpoints that say what
   something should end up as (archive, unarchive) take no `If-Match`, so repeating or racing them is safe, and a
   write that is already applied succeeds. Request bodies refuse unknown fields, so a misspelt field fails instead of
   saving nothing. So does a field newer than the server, which is how a client tells it is talking to an older one.
5. List endpoints paginate with a cursor, except the ranked list ([Search](#search)).
6. Responses are ready to show, so clients compute nothing: due dates with their labels in the request's timezone,
   contributors, who did what and when with a label ("4 days ago"), `allowed_actions`, and `null` for unset values.
7. Responses return the whole aggregate, so a client re-renders from one response after any change (promoting
   someone changes what can be done with every other admin).
8. Saved views belong to the heap, not to whoever saved them: every member sees, changes and archives them, as with
   tags. Unarchiving keeps a view's ID, so Undo after archiving gives back the same view a CLI's `--view <id>` names.
9. Archiving an option switches the open items that have it to the replacement and bumps their versions, so an edit
   made from the old value fails with a version conflict instead of undoing the switch. A closed item keeps the
   archived option, and is brought up to date when it's reopened.
10. A display name change is audited once in each of the user's teams, since the audit log is per team and that is
    where teammates see the name.
11. **A write that merges into a team tag or team parameter** is refused first with its reach: what it takes in and
    how many items it changes, with a `token`, a hash of that reach. The client sends the write again with the
    token. If the reach changed in between, the token no longer matches and the write is refused again with the new
    reach, so what happens is what the person was shown. Nothing is stored between the two requests. Making a heap
    parameter a team parameter needs the confirm even when it takes in nothing else, since it can't be undone.

## Authentication and authorization

Every secret the server hands out (a session, an OAuth code or token, a personal token, a reset, email change or
confirmation link) is a random 256-bit value of which the database stores only the SHA-256, so a leaked database
can't be used to sign in and no signing secret is needed. Sign-in rows (sessions, attempts, sign-ins in flight) are
authentication plumbing, not domain data, so ending one deletes it. Every sign-in, registration and reset opens its
session in one place, which is where a turned-off account is refused.

1. **Web app:** a cookie session (`HttpOnly`, `Secure`, `SameSite=Lax`). Every state-changing request, sign-in
   included, carries a custom header. Browsers only let same-origin scripts set one, and the server never approves a
   CORS preflight, so the header and `SameSite=Lax` block cross-site request forgery without a per-session token.
2. **Non-web clients:** the OAuth 2.0 authorization code flow with PKCE against the server's own endpoints.
   - The client opens the web app's consent page and listens on a loopback redirect. The page checks the request with
     the API before it shows anything, and Allow approves it with the session cookie, never a bearer token: a client
     can't approve more access for itself.
   - Only registered first-party public clients are allowed, and the registry is a constant, so adding one is
     reviewed code. Redirect URIs are loopback only (RFC 8252), on any port, with no query or fragment, so the
     redirect with the code always goes to the URI that was approved.
   - Access tokens are short-lived and refresh tokens rotate; a reused refresh token revokes its whole family
     ([Non-web client tokens](#non-web-client-tokens)). A bearer token may also be a personal token
     ([Personal tokens](#personal-tokens)). A refusal says why (revoked, expired, ended by a password change), so the
     CLI can tell the person.
   - A bearer token resolves to the same principal a cookie does. A request that carries one is authenticated by it
     alone, never by the cookie, and needs no CSRF header, since CSRF is a cookie problem. Other `Authorization`
     schemes are ignored, as a browser adds them itself.
   - No page of the web app may be framed, so another site can't hide the consent page under something that gets
     the user to click Allow.
   - A proxy that puts HTTP Basic auth in front of the server works for the web app but not for the CLI, since a
     request carries one `Authorization` header. Such a proxy has to let `/api` through.
3. **External identity provider:** one generic OpenID Connect provider per server. Discovery, PKCE, the nonce and
   the ID token's checks are security code, so the `openidconnect` crate does them.
   - **Start and callback.** The start keeps the nonce and verifier on a row keyed by the hash of a random `state`,
     and puts the `state` in a cookie, so the callback accepts an answer only from the browser that started, and only
     for a few minutes.
   - **Discovery is fetched on every start and callback,** not cached and not at startup: sign-ins are rare, keys
     rotate, and a provider that is down never stops the server or local sign-in.
   - **Matching is by `(issuer, subject)`, never by email.** The issuer is the string as configured, and the subject
     is the claim the operator names. A new identity needs a verified email that no account has: an email that
     matches a local account is refused rather than linked ([decisions.md](decisions.md) §38). The person then picks
     a username, and their account is made in one transaction, also when registration is closed.
   - **Teams from groups.** The operator maps teams to provider groups in configuration. The map is resolved at
     startup and never stored on a team, so a team is group-managed while the map names it. At each sign-in the
     person's membership in each mapped team follows the token's groups claim, in the transaction that opens the
     session, with an audit entry per change. A claim that can't be read is unknown and changes nothing; it is not an
     empty list, which takes the person out of every mapped team. The last-admin rule doesn't apply: the provider has
     already said the person is out, and the operator makes an admin with a command. Writers of memberships lock the
     mapped teams in one order, so sign-ins and SCIM pushes never wait on each other in a cycle.
   - **Provisioning over SCIM.** With a SCIM token set, the provider pushes users and groups to `/scim/v2`, outside
     `/api/v1` and `openapi.json`: the RFCs are the contract and providers are the only clients
     ([decisions.md](decisions.md#42-with-scim-set-up-the-providers-pushes-decide-a-persons-groups-at-a-scim-endpoint-of-their-own)).
     The pushes are kept as sent, as the provider's data. A SCIM user is linked to an account by the issuer and its
     `externalId` as the subject, never email, looked up each time, so a person who signs in later is linked from
     then on. A team follows the pushes once every one of its groups has been pushed, and the token until then, so
     pushing groups one at a time, or people before their groups, empties nothing; at sign-in a pushed team follows
     the pushes, not the token, so a token minted a moment earlier can't undo a push. A push that deactivates a
     linked person ends every sign-in of theirs in the same transaction; a sign-in that races it waits on the same
     lock, so it is either ended by the push or refused after it.
   - **Provider only.** With password sign-in off, every password path in the service layer refuses, so the API and
     the operator's commands agree. A local user's sign-ins end in two ways, and both are needed: the server's start
     ends them, so turning passwords back on doesn't revive them, and the auth check refuses them, because during a
     rolling deploy an older instance can still make one. Passwords are never deleted, so turning the setting back on
     is the recovery.
   - **Signing out at the provider** is opt-in. Only then is the raw ID token kept with the session, since it has to
     be sent back as a hint; it can't sign anyone in to Upheap. Sign-out ends the Upheap session first, and never
     fails because of the provider.
   - An invite passes its page through the round trip, so the provider flow has no invite logic.
4. **Local accounts:** passwords are hashed with argon2id, and sign-in takes an email address or a username.
   - **Attempts are limited per account**, keyed by its address whichever was typed, and per unknown address alike,
     so a refusal doesn't reveal whether an account exists. A refused attempt costs no argon2 work. Counts live in the
     database, so they hold across instances. There is no per-IP limit: behind a proxy the server doesn't know client
     addresses, so spraying one password across accounts is left to the proxy.
   - **Mail is limited on every path that sends it**, with the sign-in limit's numbers under a key of its own, so
     that no one address or account can be made to send much mail, and asking for mail never uses up sign-in
     attempts. Registration is the exception (see `register`). Mail goes out after the transaction commits, so an
     answer never waits for the mail provider.
   - **A password change** takes the current password, counted per session so someone using up the account's
     attempts can't block the owner. It ends every other sign-in and cancels any working reset link.
   - **A reset link** is made by the operator, or mailed when the server has mail. It works once, briefly, and one
     works per user. Using it ends every sign-in as a change does and signs the holder in, since the link proves who
     they are.
   - **Email links can't be read, only used.** Mail scanners and previews fetch every link in a mail, so an email
     change or confirmation link opens a page with a button, and only the press, a `POST`, changes anything.
   - **Changing the address** takes the password, and the address changes only when the link mailed to the new one
     is applied. A taken address gets the same answer as a free one, so asking reveals nothing about accounts.
     Applying cancels the working reset link, since one mailed to the old address must not keep working.
   - **Confirming the address.** With mail set up, a new password account confirms its address before it can sign
     in. Sign-in checks it only after the password verified, so a wrong password reveals nothing; with mail off an
     unconfirmed account signs in. A mistyped address can be corrected before confirming, which cancels the working
     reset link so whoever reads the wrong mailbox can't take the account.
   - **Account history** is the audit entries about a user in their personal team, worded by the server. Only the
     user reads theirs, and an operator reads any account's; team admins never do.
   - **The operator role** is set only from the server's shell, and read on every request, so taking it away takes
     effect at once. To anyone else the operator endpoints answer `404`, so the pages don't show that operators
     exist. An operator turns off and on, and makes reset links for, local accounts only: the provider owns the state
     of its accounts. Turning an account off keeps everything it owns and ends its sign-ins in one transaction.
5. **Authorization:** the API resolves a cookie or a bearer token into one `Principal`, and the service layer
   enforces access ([Access control](#access-control)).

## Access control

### Permissions as code

1. The roles table in [product.md](product.md#roles-and-permissions) is one typed enum in `domain::permission`,
   with a function that says whether a role grants each permission. One permission can back several rows of the
   table when they always grant the same roles; if the rows ever diverge, they get separate permissions.
2. Every service use case declares the permission it requires and checks it first, before reading or writing
   anything else. Handlers never check permissions; they pass the `Principal` to the service.
3. `allowed_actions` in responses comes from the same functions, so the checks and what clients show can't drift
   apart.

### Resource scoping

1. Every resource resolves to exactly one team, through its heap or directly (a team's tags and parameters). The
   service resolves the owning team first, then loads the caller's membership in it.
2. A caller who isn't a member gets `404`, the same as for a resource that doesn't exist, so an outsider can't probe
   for IDs. A member without the permission gets `403`.
3. Every repository query that loads heap-level data filters by the resolved heap, so an ID from another heap can
   never leak in; one sent in a request body is refused. A team tag's or team parameter's ID passes in the heaps of
   its team that see it, and in no other.
4. Finding users to add is for admins of a shared team and is built so it can't list addresses: a name matches by
   word prefix, an email only when typed in full, and a match shows the email only when the email matched.
5. An invite link is the one thing opened by holding its token, signed in or not. That shows the team's name and the
   maker's name and nothing else of the team. Joining checks the link under the team lock, so a revocation that
   commits while a join waits for the lock wins.

### Archived heaps

1. An archived heap is read-only for everyone, admins included. Every write on a heap and its contents checks that
   the heap is active under the heap lock.
2. Unarchiving is the only write allowed on an archived heap, and only an admin can do it, so `allowed_actions`
   offers nothing else there.

### Rules that depend on state

Rules such as a team keeping at least one admin, closed items being read-only, no blocking cycles and unique names
run in the same transaction as the write, after the permission check, with the rows they depend on locked as
described under [Cycle detection and locking](#cycle-detection-and-locking). The rules themselves are in `domain`,
and each failure has a stable code in `crates/api/src/error.rs`. The team rule is the one that locks the team row
rather than a heap: every team write locks it before any other read, then counts the remaining admins.

### Non-web client tokens

1. Access tokens issued to the CLI carry the user's full permissions; there are no narrower scopes yet. Scoped
   tokens could be added without changing the permission model, since a scope would only ever narrow a role's
   permissions.
2. Access tokens are short-lived and refresh tokens rotate on each use, within the family of one sign-in, so a client
   in use stays signed in.
3. A refresh token presented a second time means someone holds a copy, so the whole family is revoked, the
   legitimate client included, which then signs in again. A code presented twice within its lifetime revokes the
   family its first exchange issued (RFC 6749 §4.1.2); past that it's only refused, so a code left in a browser's
   history can't end the sign-in later.
4. Single use is a conditional update, so of two racing requests one wins and the other counts as reuse. On SQLite,
   which has no row locks, the loser of such a race may fail with a server error rather than be refused as reuse;
   tokens are never issued twice either way.
5. Revoking either token of a family revokes the family (RFC 7009). Signing out of the web app ends only its session.
6. A user sees and ends their own sign-ins, with a session cookie or a bearer token. The device name on each is the
   client's claim and only a label: nothing decides by it.
7. Codes, families and their tokens are authentication plumbing, so they are deleted a day after they end. Used
   refresh tokens of a live family stay, since they detect reuse.

### Personal tokens

A machine with no browser and no keychain, such as a cloud session, a container or CI, can't run the sign-in of
[Non-web client tokens](#non-web-client-tokens), and an OAuth refresh token can't be copied there: it rotates, and a
reused one revokes the whole sign-in. So a person makes a personal token in the web app and gives it to the machine.

1. The secret is made as session tokens are, and only its hash is stored. Its prefix tells it from an OAuth token,
   so the bearer check looks in one table, and lets secret scanners find a leaked one.
2. It never rotates, which is the point: the copy on the machine stays good until it expires or is revoked. The
   person picks the expiry, and "never" is allowed.
3. Revoked and expired tokens are kept, so a request that presents one can be told which it was.
4. Tokens are made, listed and revoked with the session cookie only, so a token can't make another, and a copy that
   leaks can't be used to keep access after it is revoked.
5. A token has all the permissions of its person, as a command-line sign-in has. A scope that narrowed it would be
   added the way [Non-web client tokens](#non-web-client-tokens) point 1 says.
6. A password change or reset revokes every live personal token, as it ends the web sessions and the command-line
   sign-ins: someone who resets a compromised account must cut off a token made with the stolen session.

## Audit log

1. Each use case runs in one database transaction, performs the change and inserts the audit entry before
   committing.
2. `action` is a typed enum and `diff` stores the changed fields as before/after pairs.
3. Each action is shown to those who hold the permission it takes (`audit_permission` in `domain::permission`), and
   the read filters on it in SQL so paging stays exact. Members see what happens to items; admins also see heap,
   parameter, team and membership changes.
4. The heap log reads what is recorded against the heap, and the team log the team's own entries and the heaps' own
   (created, renamed, archived), not what happens inside them. Either can read one entity's history. The log is per
   team and its entries are never copied or re-homed, so a move to another team writes one entry in each: someone in
   both teams sees the whole trail, and someone in one sees that team's part. The entry in the heap the item left
   keeps its title, so the old team's log can still name it.
5. Entries come ready to show: current names for the actor, the entity and every ID in the diff, a time label in the
   request's timezone, and a phrase for each change. An edited text people write also comes with a word diff
   computed when the log is read, so nothing extra is stored and old entries get it too. It is the server's, not the
   web app's, so every client shows the same diff.
6. A step's, comment's, question's and turn's changes are recorded against its item, so they read in the item's
   History and the heap's Activity.
7. Invite links are never deleted, and a join through one names the link, so a link's own History is its entries. A
   link ending by time is not an act and has no entry.
8. The web app shows history where things are edited, and the whole logs on the heap's and the team's Activity
   pages. `view_history` is offered only to those who may see a thing's entries, so members aren't shown histories
   that are always empty.

## Configuration and deployment

1. Configuration comes from environment variables, validated at startup; the variables and their defaults are in
   [configuration.md](../operator/configuration.md). No session secret is needed, because session tokens are random
   and stored hashed rather than signed.
2. On startup the server runs pending migrations, then serves the API and the web app on one port. On Postgres the
   run holds a transaction-scoped advisory lock, so instances starting together apply each migration once; MySQL and
   SQLite aren't run with several instances.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at Postgres or MySQL. TLS terminates at a reverse proxy, or at Cloud Run in
   front of a Neon database ([operations.md](../operator/operations.md)).
5. **Health checks:** `/healthz` answers while the process is up, and `/readyz` only once migrations have finished
   and the database answers, so orchestrators route traffic only to ready instances. Neither needs authentication,
   and neither reveals versions or configuration.
6. **Mail** is optional. SMTP always uses TLS. The development transports are the only places a mail's text is
   recorded. A failed send isn't retried.
7. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server
   with a clear message before it listens. A database that scales to zero (Neon) can take longer to wake than one
   try's timeout, so the connect is retried with backoff until a deadline set under the startup probe's limit. On
   SIGTERM it stops accepting connections and drains in-flight requests.

## CLI

`crates/cli` is `upheap`, another client of the API. Its conventions (command shape, output, exit codes,
configuration, signing in, layout and tests) are the [CLI rules](engineering-principles.md#cli) of the engineering
principles; the commands and flags are in `upheap --help`. This section is how it maps onto this API, and why.

1. **Scope.** The everyday work in a heap: items, steps, questions, comments, tags, parameters, heaps and signing
   in. Teams, members, heap settings and saved-view editing stay in the web app. Where the API takes one change in
   several writes, the CLI makes them in order and says which were saved when one fails. The CLI has no prompts, so
   a write the server wants confirmed (a tag merge, [API](#api) §11) is refused with the exact command that
   confirms it.
2. **Naming.** Items are named by key ([Handles](#handles)), by `team/heap/key` for another heap, or by a pasted
   link, whose host is ignored: requests go to the configured server. A step is `<item>.<number>`, split at the last
   `.` after the last `/`, since a link's host has dots and a key never does.
3. **State changes are one `PATCH` with a `state`,** reopening too, so the server's transition rules apply exactly
   as in the web app and the CLI keeps none of its own.
4. **Due times.** The server owns the list of rough choices, so a choice always reads back with the label it was
   picked by ([Time and timezones](#time-and-timezones) §5). Every request carries the machine's `Time-Zone`, so
   dates and labels read as they would in the browser.
5. **Lists** take the list's query parameters as flags with the same names and values, and a saved view is sent as
   those settings. Text search narrows on the client, as the web app's does ([Search](#search)).
6. **Signing in** is the flow of [Non-web client tokens](#non-web-client-tokens). On macOS the keychain is reached
   through `/usr/bin/security`, not a keychain library: macOS lets the program that made an entry read it without
   asking and tells programs apart by code signature, so an entry `upheap` made was closed to its next build. Every
   upgrade or rebuild put up a dialog, which a command run by a script or an agent waits on for ever. The cost: any
   program running as the user can read the entry the same way. `UPHEAP_TOKEN`, a personal token, wins over the
   keychain; a `401` to it is a broken setup, not a sign-in `auth login` could fix.
7. **`work run` is the agent loop** over the `/work` routes: it claims an item, starts the agent tool on it with a
   brief, keeps the claim alive and reports how the tool ended. The server decides how the run ended; the loop never
   works that out. What the loop relies on:
   - **Two clocks.** The heartbeat and the usage-limit hold go by the wall clock, checked often, so they resume soon
     after the machine wakes; the monotonic clock stands still in sleep. The silence timer that stops a tool with no
     output for 30 minutes uses the monotonic clock on purpose, since a sleeping tool isn't a silent one.
   - **The tool's stream stays local.** It is read for the cost, the model and the usage limit, kept in the run's
     log, and never sent to the server.
   - **Permissions are narrow by default.** The tool gets what its goal needs, the person's own settings decide the
     rest, and the server refuses whatever lies outside the run's item. `upheap auth` and `upheap heap use` are
     denied last, since they write on the person's machine and a deny wins over every allow. In a directory made
     for the run the tool may do anything, because Claude ignores a settings file's allow rules in a directory
     nobody has trusted.
   - **Directories are kept per item**, so a clone and its caches are paid for once. The path is the whole record,
     and the sweep removes a directory only when its age, or the server, says the item is gone. A failure to read
     keeps it, and a 404 counts only when the heap still reads, so another account's directories on a shared root
     are left alone.
   - **Stopping.** The tool runs in its own process group, so Ctrl-C reaches the loop first: one lets the run
     finish, a second stops the tool. Stopping asks before it kills, since Claude ends the commands it started only
     when asked. Whatever the run left running is found by `UPHEAP_RUN` in its environment, which survives `setsid`
     and reparenting where a process group doesn't, and macOS has no cgroups. `SIGHUP` is left alone, so `nohup`
     works.
   - **The runner's instructions** go in the brief between Upheap's and the heap's context. The heap's context is
     shared and says what the work is; how it is done is the runner's, so theirs wins on how.
8. **A newer server's values.** A new enum value is additive
   ([engineering principles](engineering-principles.md#the-contract), The contract §4), so a CLI built before it
   must still read the response: the shared response types read a value they don't know as `Unknown`, which is
   never sent. The enums the server reads in requests stay strict.

## Testing

1. **Domain:** unit tests for each ranking factor, the state machine and the due labels, and proptest invariants:
   ranking is deterministic, an item blocking another never ranks below it, and editing one item under a fixed
   `as_of` doesn't change the relative order of the others.
2. **Access control:** a table-driven test runs every endpoint as a non-member, a member and an admin, and every
   write endpoint against an archived heap. Expected statuses come from the permission table, so an endpoint added
   without a row fails the test.
3. **Store and service:** the store and API suites (the API suite drives the services) run against in-memory
   SQLite, and again against MySQL and Postgres in CI. `TEST_DATABASE_URL` points them at a server, and each test
   creates its own database there. Service containers rather than testcontainers keep `cargo test` free of Docker
   and the tests free of container start-up.
4. **API contract:** `openapi.json` and the generated TypeScript types are checked in, and CI fails if either
   changes without being regenerated.
5. **End to end:** Playwright covers the golden paths (sign up and sign in, add and rank, block and unblock, close
   and reopen, work a task through its steps, parameter management) against the real binary on in-memory SQLite, at phone, tablet and desktop
   widths. Each test registers its own account, so tests share no data and run in parallel; everything beyond these
   paths is tested lower down.
6. **CLI:** `crates/cli/tests` runs each command against the real API router on in-memory SQLite, served on a
   localhost port, and checks stdout, stderr and the exit code. The tests sign in through the real OAuth endpoints.

The commands are in [README.md](../../README.md#develop).
