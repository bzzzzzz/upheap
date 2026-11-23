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

| Area          | Choice                                                                                                                                                                                                                                                                                                                                                     |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Backend       | Rust (stable), a Cargo workspace                                                                                                                                                                                                                                                                                                                           |
| HTTP          | axum on tokio, tower middleware                                                                                                                                                                                                                                                                                                                            |
| API contract  | Code-first OpenAPI 3.1 with utoipa. The web app's TypeScript types are generated from it with openapi-typescript, and CI fails on drift                                                                                                                                                                                                                    |
| Data access   | SeaORM 2 over sqlx, with the backend chosen at startup from `DATABASE_URL`: SQLite, MySQL or Postgres                                                                                                                                                                                                                                                      |
| Migrations    | sea-orm-migration, embedded in the binary and run at startup                                                                                                                                                                                                                                                                                               |
| Auth          | argon2id for local passwords; an own session table with hashed random tokens for the web app; own OAuth 2.0 authorization-code endpoints with PKCE for non-web clients, and personal tokens for machines with no browser. One OpenID Connect provider per server, through the `openidconnect` crate, next to local accounts                                |
| Web app       | React, Vite, TanStack Query and Router, Tailwind, React Aria Components, Motion; embedded in the binary with rust-embed                                                                                                                                                                                                                                    |
| Observability | `tracing` with JSON logs, request IDs, `/healthz` and `/readyz`                                                                                                                                                                                                                                                                                            |
| Testing       | `cargo test` with googletest matchers, proptest for ranking invariants, MySQL and Postgres service containers in CI, Playwright end to end                                                                                                                                                                                                                 |
| Packaging     | A static binary in a minimal Alpine image; docker-compose with SQLite on a volume; Cloud Run with Neon Postgres in production ([operations.md](../operator/operations.md)); the CLI and the server as static binaries on GitHub Releases, with a Homebrew cask for the CLI in `bzzzzzz/homebrew-tap` ([operations.md](../operator/operations.md#releasing-the-cli-and-the-server)) |

## Workspace

Dependencies point one way: `server → api → service → store, domain`, with `api → api-types → domain` and
`server → mail` beside it, and `service → mail` for the mail it sends. `domain` depends on nothing in the workspace, so ranking and the state rules are unit- and
property-tested in isolation, and `store` and `api` never depend on each other.
`api-types` holds the request and response types with no IO, so a Rust client such as `cli` shares them without
linking the server. It copies each domain enum as a type of its own on purpose: the API tolerates values it
doesn't know in what it only reads, leaves some values out, and its list of values is the reviewed contract. The
conversions are exhaustive `match`es, so a new value fails the build until it is added, and a macro declaring the
copies was tried and dropped because it would hide that contract. In `domain`, an enum whose codes are stored is
declared with `coded_enum!`, which exists for the one thing a `match` can't check: that the list of every value
holds every variant. Files that nearly every feature edits (`domain::permission`, `api/src/error.rs`, the access
control table test) are meant to be: the build or the test fails until the feature is classified there. The crates and their roles are listed in [README.md](../../README.md#layout); each crate's root
module says what it owns.

## Data model

The tables and columns are in the migrations. These are the conventions they follow.

- **IDs** are UUIDv7 in `CHAR(36)`: sortable by creation time and safe to expose.
- **Handles** name things in URLs, API paths and the CLI ([Handles](#handles)): usernames, team slugs, heap slugs
  (unique per team) and item keys (unique per heap). They are extra columns with unique indexes, and every foreign
  key still points at the UUID, so a handle can change without touching references.
- **A reminder's arrival** is its own column, `items.arrives_at`: the first millisecond of its date in the time zone
  of the request that set it, not worked out from the due's end of day (a day that changes the clocks is not 24
  hours long). It is `NULL` for an item that was never a reminder, and kept when the reminder becomes a task, so the
  task's age still counts from it. A reminder's due is firm and ends the same day. Ranking reads
  `arrives_at.unwrap_or(created_at)` as the item's age, so `arrives_at` has a second use: an item made from another
  with `age: keep` gets the origin's arrival (or its creation) there, and so counts its age from the note's creation.
  It is still `NULL` for every other item, and the API shows it only for reminders.
- **An item's origin** is `items.origin_id`: the note it was split from (`item add --from`), or the item of the run that
  added it, with `created_run_id` set beside it then. `made_in_split` marks the first kind: only those are what the note
  became, so a follow-up a run adds doesn't count toward `nothing_made`, the run's reached check or the Became list. All
  three are set when the item is made and never changed, and stay when either item later moves to another heap or team,
  as a blocker's link does; a reader sees the other end only where they may view its heap. An index on `origin_id`
  serves the lookup of what a note became, which the item and the closed list show as `became`, each with the heap it is
  in now: a handful of items per note, and at most one page of closed items, read in one query.
- **Timestamps** are UTC Unix milliseconds in `BIGINT`, which sidesteps the differences between the databases'
  date types. No timezone is stored anywhere ([Time and timezones](#time-and-timezones)).
- **Structured data** (audit diffs, saved list views) is JSON in `TEXT`, never queried inside the database.
- **Archiving** is a nullable `archived_at`. There are no hard deletes, and foreign keys use `ON DELETE RESTRICT`,
  so even an accidental `DELETE` fails instead of removing related rows. Relationship rows follow the same rule:
  taking a tag off an item sets `item_tags.removed_at`, and clearing a parameter value sets its `option_id` to
  `NULL`. Team membership is the one exception: removing a member deletes the `team_members` row, and the audit log
  keeps the history. The log is also where a removal is remembered: joining through an invite link reads the person's
  last `member_removed` entry, so a link made before it doesn't let them back in.
- **Optimistic concurrency:** every mutable row carries a `version`. Updates require the expected version and fail
  with `409` otherwise. A parameter is one aggregate: its options and tag links have no version of their own, and
  every change to them takes the parameter's version.
- **A task's steps are rows of their own** in `item_steps`, each with its own `version`: a step write never bumps the
  item's, and an item write never bumps a step's, so an agent ticking steps never makes an open edit of the item
  conflict. A step has a `number`, given when it is added (one past the highest the item has had) and never changed
  or reused, which is its name (`k3x9a.3`), and a `position` for its place in the order, kept apart so reordering
  never renames a step. A move (an `after` in the step's patch) writes the item's positions afresh from 0 and bumps
  only the moved step's version; the steps it displaced keep theirs, as when an added step shifts them. Steps are never deleted: a dropped one stays. A task has at most 100, which is the size
  assumption that lets an item response carry all of its steps: only a single-item response does, and the ranked
  and closed lists carry `steps: null` and just the progress, counted in one grouped query per chunk without
  loading the steps. Progress is ticked of open plus ticked, so dropped steps don't count. A step write takes the
  heap lock like any item write, then reads the item and its steps again with locking reads (see
  [Cycle detection and locking](#cycle-detection-and-locking)), and answers with the whole item. Steps of a closed
  item are read-only, and come back editable, unchanged, when it is reopened. A step made from a note
  (`POST …/to-step`) keeps it in `item_steps.origin_id`, set once and never changed; an index on it serves the
  lookup of the steps a note became, which the note and the closed list show as `became_steps`. The note was in the
  task's heap then, but either may have moved to another team since, so a note shows as the step's origin only to
  someone who may view its heap now. A heap made from a note
  (`POST …/heaps` with `origin_id`) keeps it the same way in `heaps.origin_id`, with an index for the lookup of the
  heaps a note became (`became_heaps`) and of the heaps made from the items of a given heap, which the agent queue
  follows. The note is in the heap's team, and making the heap leaves it open: it closes on `became`.
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
- **State and `closed_at`:** in the database, `closed_at` and `closed_by` are set exactly when the state is done,
  won't do or became (a note closed because items were made from it; only `POST …/became` sets it). In `domain`, the state is an enum where only the closed variants carry them, so an open item with a
  close time can't be built. The store converts between the two and rejects rows that break the rule.
- **A tag is owned by a heap or by a team:** exactly one of `tags.heap_id` and `tags.team_id` is set, and the store
  checks it on every read. Both kinds share one table, so `item_tags`, parameter links, saved views and tag orders
  name either by ID with one foreign key, and a team tag needs no copy per heap: a rename is one row written. A
  heap's tags are its own and its team's, read together. Every tag has a `color`, a code from a fixed palette; a tag
  made without one gets the least used among the tags it sits among.
- **A parameter is owned by a heap or by a team:** exactly one of `parameters.heap_id` and `parameters.team_id` is
  set, and the store checks it on every read. A heap's use of a team parameter is a row in `heap_parameters` with the
  heap's weight, rather than a weight on the parameter, which would be one number for every heap; stopping sets
  `stopped_at` and using again clears it, so the weight survives. The store reads a heap's parameters as its own plus
  the team parameters it has a use row for, with that weight, and archived when the team archived the parameter or
  the heap stopped using it. Ranking, item values, filters, saved views, moves and the audit's names read that, so
  they see a used team parameter as the heap's own, and one a heap doesn't use isn't there. A team parameter has two
  kinds of tag link. The team's links, to team tags only, are in `parameter_tags` like a heap parameter's and keep the
  parameter's version. A heap's own links to a team parameter it uses, to its own tags or the team's, are in
  `heap_parameter_tags`, a table of their own because a link belongs to the heap and `parameter_tags` rows have no
  heap, and because it must stay the heap's when its tag becomes a team tag. They carry no version: a write names the
  tags it links and unlinks and leaves the others, so two writes that race both take effect. The write also holds the
  parameter only shared, and bumping its version would need the exclusive lock (see `lock_parameter_shared`). The
  heap's read of its parameters puts them beside the team's links, and a tag that suggests the parameter in that heap
  is in either.
- **Case-insensitive uniqueness** (tag names per owner) is a lowercased key column with a unique index rather than a
  collation, because MySQL compares case-insensitively by default and SQLite doesn't. Where an index can't express
  the rule, as for parameter names, which must be unique only among parameters in use, the service checks under the
  heap lock. So it is for a name across the two kinds of tag: no heap tag in use may share a name with a team tag of
  its team, which the tag writes check under the locks of [Cycle detection and locking](#cycle-detection-and-locking).
  The same holds for a parameter's name across a team's parameters and the parameters of its heaps, which the
  parameter writes check under those locks.
- **A merged heap tag** keeps its row: `tags.merged_into` names the team tag it was merged into (`NULL` for a tag in
  use), and its `name_key` becomes a space and its own ID, which frees the name for a new heap tag, keeps the key
  unique, and can't match a name looked up, since names are trimmed. Its name and description stay, so the history of
  what happened before the merge still names it. Only the history's name lookup reads merged tags; every other read
  leaves them out, as if they were unknown.
- **A merged heap parameter** is archived. Making a heap parameter a team parameter moves each item's value to the
  team parameter and then archives the heap parameter, which the archiving rule allows since no item has a value for
  it any more. Archiving is enough: it keeps the row, name and options, so the history from before still names them
  (its name lookup reads archived parameters), frees the name, and can't be undone. The one that starts the merge
  becomes the team parameter in place (same row and ID, no weight of its own, each heap's weight in its use row), so
  its heap's items, views and history need no change.
- **Insert-only tables:** the application can only insert into `audit_log`. There is no update or delete code path
  for it.
- **The heap's ranking policy** is its row: the need, age, deadline, value and tags weights on `heaps`, plus each
  parameter's weight, and the heap's tag order. The weights and the order change together through one endpoint, in
  one transaction under the heap lock, because a weight only means something next to the others (a factor's share
  is its weight out of the total) and separate writes per row could leave the policy half-saved. The heap's version
  orders these saves, moving on also when one changes only parameter weights. A parameter's own `PATCH` still sets
  its weight alone, guarded by the parameter's version only, so a weights save made meanwhile against other
  parameters still succeeds; making every parameter edit move the heap's version would close that gap but make
  renames conflict with parameter edits. The policy includes the weights of the team parameters the heap uses, which
  are on its use rows. The tag order is a JSON list of tag IDs on `heaps.tag_order`, the highest
  first (`NULL` when empty), rather than a rank column on `tags` or a table of its own: it belongs to the heap, it is
  saved under the heap's version with the weights, and a tag shared by several heaps (a team tag) can then sit at its
  own place in each heap's order without a schema change. A tag the list doesn't name is unranked.
  Effort, impact and need are columns on `items` rather than custom parameters, because they are
  fixed and apply to every heap ([decisions.md](decisions.md#6-effort-impact-and-need-are-item-columns-not-custom-parameters)).
- **A user's username is the slug of their personal team**, so usernames and team slugs share one namespace under
  one unique index. The personal team can't take members, and its slug never changes.
- **Archived options always name their replacement** (`archived_at` and `replaced_by_option_id` are set together),
  so a stored value can always be followed to an option in use.
- **Invite links keep their token readable**, unlike session tokens and OAuth secrets, which are stored hashed: a
  link is sent around for a week and admins copy it again, so the server must be able to show it. A leaked database
  therefore holds working links, each worth a membership for at most a week; revoking one ends it. A link's end is
  derived from `created_at` and one constant in `domain`, not stored.
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

1. **A username is the slug of its user's personal team** (`teams.personal_user_id` points at the owner), so one
   unique index covers both and `/teams/eugene` is always Eugene's personal team. The personal team is private: it
   has exactly one member and refuses members, removal and leaving (`409 personal_team`, and its `allowed_actions`
   don't offer them), and its slug can't change, since usernames never do. Its name can change, and it holds as
   many heaps as the owner likes; anything shared lives in another team.
2. **Format.** Usernames and slugs are lowercase letters, digits and single dashes, starting with a letter and not
   ending with a dash, within the lengths `domain::handle` sets. Input is checked, never fixed silently: `My-Team`
   is `422 invalid_handle` with a hint showing `my-team`. A team or heap created without a slug gets one derived
   from its name, with `-2`, `-3`… where that's taken. Two creates of one name at once can both find the same slug
   free; the one the unique index refuses starts again with the next suffix, so only a slug the caller chose is
   ever `409 handle_taken`. A taken username at sign-up comes with a free one as the hint, made the same way:
   `sam-2` for `sam`.
3. **Item keys** are five characters from Crockford's base32 alphabet in lowercase (no `i`, `l`, `o` or `u`), with
   at least one digit, so they're easy to read aloud and type and can't spell real words: about 30 million per
   heap. They're random, so a key says nothing about when an item was made or how many a heap has. The key is drawn
   inside the item's insert transaction, under the heap lock every item write takes, and checked against the unique
   index on `(heap_id, key)`; a clash is retried, and with a few thousand items one happens about once in several
   thousand inserts. `domain` turns random bytes into a key, and the service draws the bytes from the
   `RandomSource` it's given, like its clock, so tests can script them. Items are never deleted, so a key is never
   reused within a heap, except that one an item took with it when it moved to another heap can be drawn again, about
   one chance in 30 million per insert. An item that moves to another heap keeps its key unless the
   target heap has an item with it, and then gets a new one, drawn under the locks the move takes.
4. **Nothing is reserved.** Every handle sits after a fixed word in the URL (`teams/`, `heaps/`, `items/`), so it
   can't clash with a page the app has now or adds later. Teams start at `/teams/` rather than at the root because
   the root holds the app's own pages and the server's paths (`/api`, `/healthz`, `/oauth`). API paths have the
   same shape under `/api/v1`, so a web URL becomes its API call by adding the prefix.
5. **Short links.** `/h/my-team/backlog/k3x9a` is the short form, one handle per position: team, then heap, then
   item. The web app's router replaces it with the full URL as it opens. Items offer Copy link with the short form; the API has none. The CLI takes an
   item as its key within the default heap, or as the full path `my-team/backlog/k3x9a`, a short link without its
   `/h/`, so a pasted link works too.
6. **Changes.** Usernames are fixed, and an item's key changes only when it moves to a heap that has the key. Team
   and heap slugs can change, and an item's heap can, by a move. A move keeps the item's former
   (heap, key) handles. An item path with one names the item where it is now, for reads and writes alike, unless an
   item holds that handle now, and then that item; of several former holders the last to leave. It is read like any
   item, against the heap it is in now, so after a move to another team it answers `404` to whoever can't view that
   heap. The web app then shows the current address. A renamed team or heap slug still breaks old links.
7. **Sign-up** asks for a username with the display name, email and password; the personal team takes it as its
   slug, and the first heap is `personal`. A first OIDC sign-in asks for a username before creating the
   user.

## Ranking

### Computation

Ranking is a pure function in `domain`: the heap's policy, its non-closed items with their parameter values and
blocking edges, and an `as_of` time go in; positions, contributors and held-back entries come out. Nothing about scores is stored.

1. The service loads everything the heap needs and ranks in memory on each request. Heaps are expected to hold at
   most a few thousand open items, so this is cheap. A per-heap cache invalidated on write can be added if
   measurements call for it.
2. Each factor (need, age, deadline proximity, value per effort, each custom parameter, the tag order) produces a weighted
   component. Age counts from when the item joined the list: its creation, or for an item that was a reminder, its
   `arrives_at` (`ItemForRanking::from`), so a reminder arrives ranked as due that day rather than as an old item.
   The score combines them. The formula is a named version (`FormulaVersion::V4`) behind one
   function, set in code rather than stored per heap: tests pin orderings per version, so a formula change is an
   explicit test update, and since scores are never stored, switching versions needs no migration. The module doc of
   `domain::ranking` explains the current formula.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are refused on write. An item's
   effective score is the greater of its own and the boosted effective scores of the items it blocks, computed by
   one memoized walk, so it's linear in the number of items and carries through chains. Ties go to the item blocking
   the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
4. **Contributors** are the largest weighted components, each with a label and a relative share ("Deadline in a few
   days", "Blocks _Migrate auth_"). Scores themselves are never returned.
   **Held back by** names what the person could change to lift an item: a low need, no due date, unset sizes, a tag
   low in the heap's tag order. A
   factor counts while its weight is above 0 (the need while its weight or the age weight is), and unset sizes are
   labelled with the formula's defaults. It is empty for a blocker ranked by what it blocks; the service also
   empties it for the item listed first in its group, which is the one the item pane calls "Why it's #1", and for
   every item in a group sorted by something other than priority, where those fields don't move it.
5. **Ties** that the chain length doesn't settle go to the older item, then to the ID, so the order is fully
   deterministic.
6. **Placement** (product.md, Priority §8) is the one stored input that isn't a field: a positive multiplier on
   one item, or none. Ranking multiplies the item's score (its own or what it inherits) by it, then applies the
   blocker rule again, so a blocker still never ranks below what it blocks. On a drop the client sends the anchor,
   the side and its list's `as_of`; the service ranks the heap at that `as_of` with the item's own placement left
   out and sets the multiplier so the item lands halfway between the anchor and its neighbour on that side. Placing
   again replaces the number, a move to another heap drops it, and nothing else touches it. The module doc of
   `domain::ranking` has the details. The web app only offers it in the ranked group of a heap's priority list,
   since the API can't know the view a client shows.

A ranking preview ranks the heap with draft weights beside the stored ones, at one `as_of`, and returns the top of
the default view's ranked group with each item's position under both. It stores nothing.

### Across heaps

`GET /me/items` is the ranked list across every active heap the caller may view. Each heap that has any non-closed
item is ranked and grouped under the request's view by the same code as its own list, so a row is what that list
would show. Then each group (pinned, ranked, blocked) is merged across heaps by the item's score in its own heap
times the caller's **heap priority** for that heap, highest first, then heap name, then the item's position in its
heap, the same rule `GET /me/waiting` and the agents' queue order by (the queue also boosts started work, as above). A heap priority is one person's own number
for one heap, 1 to 10: a `heap_priorities` row per person and heap, 1 when there is none, so teammates in a shared
heap each order it their own way. Since a heap's group is already in score order and the priority is the same
positive factor for all of a heap's items, the merge keeps every heap's own order. A change to it is audited in the
person's personal team, not the heap's, because the value is private and the heap's log is read by the team. A heap made from a note starts with a copy of the note's heap's rows, in the transaction that makes it, so it sits where its source did for everyone; the copy isn't a change anyone made, so it is not audited. The view takes no sort, which belongs to one heap. `tag` and `param` filter only with a `team`, as that team's tags and team parameters' options: they are checked against the team, and the options of archived team parameters are dropped, as a heap's list ignores them. A heap that doesn't use a named (non-archived) team parameter is left out, unranked, since none of its items can hold a value for it. Filtering picks rows after each heap's ranking, so the merged order is the unfiltered one with the non-matching rows taken out. An optional `team`, a team's slug, limits the
list to that team's heaps: they are filtered from the viewable heaps before any is ranked or merged, so the order
within them is the unscoped list's, and the cap below applies after. A team that doesn't exist, that the caller isn't
in or that isn't a valid handle is `not_found`, as every handle lookup answers, so teams can't be probed. It returns at most
500 rows, pinned first, with `count` the true total: the size assumption that lets it skip paging.

The closed items and the history across the same heaps (`GET /me/items/closed`, `GET /me/audit`, each with the
optional `team`) are not merges of per-heap reads: one query reads a page over all the heaps, ordered and paged by
the cursor a single heap's read uses, and each row is then built as its own heap builds it. A closed item gets its
heap's tags, parameters and the caller's role there. The history reads each team's heaps with the actions the
caller's role in that team may see, so a member of one team and an admin of another sees in each heap what its own
history shows them. Archived heaps are left out of both, as they are from the list.

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
3. Filters by state, tag, need, size and parameter option, and the sorts, stay on the server, because they change
   which items are pinned and how they are grouped. Ranking always runs over every non-closed item, since a blocker
   inherits from what it blocks even when that item is filtered out or hidden; the view then picks, groups and sorts
   the result, and reports what it left out (`hidden_count`, `unfiltered_count`, `in_progress_count`) so a client
   can say "6 of 14" or offer the next item without a second request. The view's defaults (in progress pinned,
   blocked hidden) are the server's, so every client starts from the same list. Archived parameters and options are
   accepted in filters and sorts, so a saved view keeps working after an option is archived.
4. **Reminders to come** are not ranked: `rank_open_items` splits them off (`Item::is_to_come`) at the server's
   clock, not at the time a list ranks at, so a reminder whose date comes while a list is open joins it on the next
   fetch. No list or queue sees them, and every open count leaves them out (`count_items_by_heap` takes the time to
   compare with). Waiting for me keeps a question on one, after the ranked rows and by the reminder's date. They are
   not hidden from search: the list's `reminders` setting adds them to the response as a group of their own,
   soonest first and filtered like the rest, and a client that searches asks for it. The client still searches the
   text itself.
5. The web app keeps the view in the URL, so it survives reloads and can be shared, but not the search text, which
   is a momentary narrowing. Saved views store the same settings under the query parameters' names, so a client
   turns one into a list query field by field.
6. **Search across heaps** (`GET /me/search?q=`) finds items by title or key in every heap the caller may view,
   closed items included, which the lists can't: they hold open items only, per heap. The title is matched as a
   case-insensitive substring by portable `LOWER(title) LIKE LOWER(pattern) ESCAPE '!'`, with `!`, `%` and `_` in the
   text escaped so they match only themselves. An item whose key is exactly the text comes first, then open items,
   newest created first, then closed items, most recently closed first. At most 50 rows come back, and `truncated`
   says whether more matched. Case folding is the database's `LOWER`: Unicode on Postgres and MySQL, ASCII only on
   SQLite, and MySQL's collation also ignores accents. It is `LIKE`, not full-text, by [decision 12](decisions.md#12-portable-sql-over-per-database-features):
   a person sees thousands of items at most, and the scan is narrowed to their heaps by the `heap_id` index.
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
5. The server lists the rough due choices a picker offers (`GET /due-choices`), each with its date in the request's
   timezone and the label it reads back with, so "Next week" can never come back as "in a few days".

### Cycle detection and locking

1. Every item, parameter and saved-view write first locks the heap row (`SELECT … FOR UPDATE` on MySQL and
   Postgres), before touching any other row, so writes in one heap run one after the other and always take locks in
   the same order. The write checks that the heap isn't archived on the heap row read under that lock, and archiving
   takes the same lock, so no write lands in a heap archived while it waited. Reads that a rule depends on (the
   blocker chain, a parameter and its options, a saved view, an item and its steps for a step write) are locking
   reads too, because on MySQL a plain read before the lock would fix the transaction's snapshot and hide changes
   that committed while it waited. SQLite has one writer at a time, so there the second of two conflicting writes
   fails instead of waiting; the outcome that matters, no inconsistent state, holds either way. The tag IDs a
   write stores are checked with a locking read, since a merge can retire a heap tag while the write waited.
   A heap tag write first takes a shared lock (`FOR SHARE`) on its team's lowest heap, unless that is its own heap,
   then locks its own heap row, then reads. A team tag write reads the team's heap IDs before its
   transaction, then locks each of those heaps in heap ID order, as a move does, and reads after that; if the team's
   heaps read again under the locks differ, a heap was made in between, and the write starts again. A team tag
   write that merges heap tags (making or renaming a team tag into a name the team's heaps have) writes items,
   parameters, saved views and tag orders in every heap of the team, so it needs every heap row; locking the team
   row instead would deadlock with an item write, which holds its heap row and then needs the team row's key for its
   audit entry. A heap tag write and a team tag write of one team meet on the heap row, or on the team's lowest heap
   for a heap made after the team tag write read the team's heaps again (heaps are never deleted and never change
   team, and their IDs are UUIDv7, rising with time, so the lowest stays the lowest while the servers' clocks agree to
   within the age of the team's first heap; every team tag write locks it exclusively), and two team tag writes on
   that heap too. In a team with no heaps a team tag write locks the team row instead, which making the first heap
   waits for (it checks the row's key), and there is no item write to deadlock with. So no two tags take one name.
   A team parameter write takes the same locks as a team tag write, since archiving a team parameter or one of its
   options reads and writes items in every heap of the team, and a heap parameter made or renamed takes the shared
   lock on the team's lowest heap first, so no heap parameter and team parameter take one name. The lock order stays ascending. A merge does a
   handful of queries per item it reaches while it holds every heap lock of the team, so item writes in the team wait
   for it; that is fine for merges of up to a few hundred items, which is the size assumption, and there is no limit.
   The plain reads of a tag write come after its locks, so on MySQL their snapshot sees what committed while it
   waited; the name checks take no locking reads, which would take gap locks that deadlock tag writes of two teams.
   Making a heap parameter a team parameter takes the locks of a team parameter write, since it writes items, saved
   views, parameters and use rows in every heap of the team, and does the same handful of queries per item it reaches
   under the same size assumption.
2. A move of an item to another heap, in any team the caller may edit items in, locks both heaps in heap ID order, as
   the agent's queue locks several, then reads the item with a locking read and checks its version. A run's `became`
   locks the note's heap and the active heaps made from the note the same way, since it defines the tasks in them. Two
   moves in opposite directions can't wait for each other, and the second of two moves of one item finds it changed. The
   item's own blocker is part of its old heap's state, so the moved item stays blocked with nobody picked. The items
   that waited for it keep it as their blocker, now in another heap, so they wait for the person (§3), and a move back
   makes it hold them again; the move writes nothing to them. A blocker in another team than the item it blocks is never
   named, to anyone: the item has no `blocked_by` and its `blocker_gone` says it moved, with no heap, whatever the
   blocker's state there (`gone_blockers`); Waiting for me shows it as a row of its own kind, `blocker_left_team`, with
   no blocker (a new kind, so released clients skip it, as the API adds rather than changes). A note that heaps were
   made from, archived ones included, stays in its team, since a run's `became` defines the tasks in those heaps. Since
   an item's heap can change, a write that finds the heap through a read before the lock (a step, a question, a turn, a
   run) reads the item again under the lock and refuses with `409 version_conflict` when it has left that heap
   (`find_item_locking_in`).
3. Setting a blocker walks the chain inside that transaction and refuses a request that would reach the item
   itself. A blocker is part of the blocked state: leaving the state clears it, and it must be an open item of the
   same heap when it is set, also for an item made blocked (`blocked_by` on create). So is the reason in words
   (`blocked_reason`): leaving the state clears it, and a move to another heap keeps it while the blocker is dropped.
   Closing a blocker as done frees what it blocks in its heap: in the same transaction,
   under the heap lock, each item it blocks goes to open with its own `item_updated` entry naming the blocker
   (`blocker_done`), and gets its next version. Closing it as won't do writes nothing to them: an item whose blocker is
   closed, closed as became, or in another heap after a move (§2), stays blocked and waits for the person, which is read off the item
   and its blocker, never stored (`Item::blocker_gone`). It shows in Waiting for me and its count, in the item's Waits
   for you, and in the agents' waiting count, until the person unblocks it, closes it or picks another blocker.
   Reopening the blocker makes it hold them again; an item it freed stays open, and its history says why. Items
   blocked by a done item before this rule weren't freed; they wait the same way.
4. Ranking still tolerates a cycle in stored data, by ignoring the edge that closes it, so one bad row can't break
   the list.

## API

1. Everything is under `/api/v1`, JSON over HTTPS. The spec is `openapi.json`.
2. Errors are `application/problem+json` with a stable `code`, mapped in one place (`crates/api/src/error.rs`),
   where every error variant must be classified or the build fails. The exception is `POST /oauth/token` and
   `POST /oauth/revoke`, which refuse requests in the RFC 6749 format so any OAuth client library understands them;
   a failure of the server itself there is still a problem response.
3. Paths name things by handle: `{team}` and `{heap}` are slugs, `{item}` a key, `{username}` a username. Tags,
   parameters, options and views keep their IDs, checked against the heap in the path (a team tag's in any heap of
   its team). A team's own tags are listed, made and edited under `/teams/{team}/tags`, and a heap's tags list holds
   both kinds apart. A team's parameters are under `/teams/{team}/parameters`; a heap's parameters list holds the
   team parameters it uses among its own, marked by `kind`, and the team's others apart in `team_available`. A handle that's malformed or names nothing the caller can see is `404`. Responses carry both
   the `id` and the handle; request bodies refer to other things by ID, since clients have the objects in hand.
   Creating a team or heap takes an optional slug, derived from the name when left out.
4. Updates are `PATCH` with partial bodies and the expected `version` in `If-Match`. Endpoints that say what
   something should end up as (archive, unarchive) take no `If-Match`, so repeating or racing them is safe, and a
   write that is already applied succeeds (adding a member who is one, removing someone who isn't). Request bodies
   refuse unknown fields, so a misspelt field fails instead of saving nothing. So does a field newer than the
   server: a client that sends one to an older server gets `400 invalid_body`, and since the rest of its body is
   valid, that code is how it tells, rather than looking for the field in the response.
5. List endpoints paginate with a cursor, except the ranked list ([Search](#search)).
6. Responses are ready to show, so clients compute nothing: `due` as `{ at, firm, label, overdue }` in the
   request's timezone, `contributors`, who did what and when as a `UserRef` and a label ("4 days ago"),
   `closed_group` as a code the client has headings for, `allowed_actions`, and `null` for unset values.
7. Responses return the whole aggregate: a team with its members and heaps, each with their `allowed_actions`, so a
   client re-renders from one response after any change (promoting someone changes what can be done with every
   other admin); a parameter with its options and tag links; `GET /me` with the caller's teams and heaps and their
   counts.
8. Saved views belong to the heap, not to whoever saved them: every member sees, changes and archives them, as with
   tags. An archived view is left out of the list and refuses edits until it's unarchived, which keeps its ID, so
   Undo after archiving gives back the same view a CLI's `--view <id>` names.
9. Archiving an option switches the open items that have it to the replacement and bumps their versions, so an edit
   made from the old value fails with a version conflict instead of undoing the switch. A closed item keeps the
   archived option; when it's reopened, its values are brought up to date the same way, in the reopening's audit
   entry. Archiving an option of a team parameter does this in every heap of the team.
10. A display name change is audited once in each of the user's teams, since the audit log is per team and that is
    where teammates see the name. A user in no team has nowhere to record it.
11. **A write that merges heap tags into a team tag** (making, renaming or promoting a team tag into a name heap
    tags of the team have) is refused with `tag_merge_unconfirmed` and the reach: the heap tags it takes in and how
    many items carry each, with a `token`, a hash of that reach (`domain::tag::reach_text`). The client sends the
    write again with `confirm_reach=<token>`. If the reach changed in between, the token no longer matches and the
    write is refused again with the new reach, so what happens is what the person was shown. Nothing is stored
    between the two requests. A merged heap tag's items, parameter links (a heap's links to team parameters too),
    saved views (archived ones too, so none comes back naming a merged-away tag) and tag-order places all move to the
    team tag in that one transaction ([Data model](#data-model), a merged heap tag).

    **Making a heap parameter a team parameter** works the same way, with `parameter_merge_unconfirmed` and its reach
    (`domain::parameter::reach_text`): the parameter it starts from, then each same-named heap parameter it takes in,
    each with its item count, the options it adds and the options it switches (a live option that matches one the team
    parameter archived, whose open items get its replacement). A reach of the starting parameter alone still needs the
    confirm, since the move to the team can't be undone. Each parameter's tag links become its heap's links to the
    team parameter (`heap_parameter_tags`), never team links, so no other heap gets suggestions. The starting
    parameter's own links end, as it is the team's now, and continue as its heap's links; a merged parameter keeps its
    link rows. The reach lists the tags per row, but they aren't part of its token: links can be edited after the
    merge, so a link changed in between carries over as it is at the confirm. Repeating the write on a team parameter
    the heap uses succeeds with no confirm and changes nothing.

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

1. Every resource resolves to exactly one team: heap → team; item, heap tag, heap parameter, option and view →
   heap → team; a team tag or team parameter → team. A team tag is the first resource a team owns beside its heaps
   and members ([decisions.md](decisions.md#44-team-tags-belong-to-the-team-one-row-used-by-every-heap)), and a team
   parameter the second ([decisions.md](decisions.md#45-team-parameters-belong-to-the-team-each-heap-opts-in-and-keeps-its-own-weight)).
   The service resolves the owning team first, then loads the caller's membership in it.
2. A caller who isn't a member gets `404`, the same as for a resource that doesn't exist, so an outsider can't probe
   for IDs. A member without the permission gets `403`.
3. Every repository query that loads heap-level data filters by the resolved heap, so an ID from another heap can
   never leak in; one sent in a request body is refused with `422`. A tag ID is checked against the heap's tags, its
   own and its team's, so a team tag's ID passes in every heap of its team and in no other. A team parameter's ID passes in a heap that
   uses it, and for a write on the parameter itself (not its weight there) it is `404` under a heap's path.
4. Finding users to add (`GET /users`) is for admins of a shared team (everyone is admin of their personal team,
   which doesn't count) and is built so it can't list addresses: a name matches by word prefix, an email only when
   typed in full, and a match shows the email only when the email matched.

5. An invite link is the one thing opened by holding its token, signed in or not. That shows the team's name and the
   maker's name, whatever the link's state, and nothing else of the team. Joining checks the link at that moment, under
   the team lock, so a revocation that commits while a join waits for the lock wins.

### Archived heaps

1. An archived heap is read-only for everyone, admins included. Every write use case on a heap and its contents
   first checks that the heap is active, under the heap lock, and refuses with `409` otherwise.
2. Unarchiving is the only write allowed on an archived heap, and only an admin can do it. `allowed_actions` is
   empty for anything in an archived heap, apart from `unarchive` for admins.

### Rules that depend on state

Rules such as a team keeping at least one admin, reopening within a week, closed items being read-only, no blocking
cycles, an archived option needing a replacement, a parameter being archived only when unused, and names being
unique run in the same transaction as the write, after the permission check, with the rows they depend on locked as
described under [Cycle detection and locking](#cycle-detection-and-locking). Each failure is a `409` or `422` with
a stable code; the codes are in `crates/api/src/error.rs`, and the rules themselves in `domain`. The team rule is
the one that locks the team row rather than a heap: every team write locks it before any other read, then counts
the remaining admins.

### Non-web client tokens

1. Access tokens issued to the CLI carry the user's full permissions; there are no narrower scopes yet. Scoped
   tokens could be added without changing the permission model, since a scope would only ever narrow a role's
   permissions.
2. Access tokens are short-lived and refresh tokens rotate on each use. A family is one sign-in by a client: the
   pair the code was exchanged for and every pair refreshed from it, one row in `oauth_token_families`. Refreshing
   uses the refresh token up and issues a new pair in the same family, so a client in use stays signed in.
3. A refresh token presented a second time means someone holds a copy, so the whole family is revoked, the
   legitimate client included, which then signs in again. A code presented twice within its lifetime revokes the
   family its first exchange issued (RFC 6749 §4.1.2); past that it's only refused, so a code left in a browser's
   history can't end the sign-in later.
4. Single use is a conditional update (`used_at IS NULL`), so of two racing requests one wins and the other counts
   as reuse. A refresh locks the family row and a revocation updates it, so the two run one after the other. On
   SQLite, which has no row locks, the loser of such a race may fail with a server error rather than be refused as
   reuse; tokens are never issued twice either way.
5. Revoking either token of a family (`POST /oauth/revoke`) revokes the family, and answers `200` for unknown tokens
   too (RFC 7009). `POST /auth/logout` ends only a session cookie.
6. A user sees and ends their own sign-ins at `/me/sign-ins`, with a session cookie or a bearer token. The list is
   the user's live families: not revoked, and holding a refresh token that is unused and unexpired. Each carries
   `last_used_at`, the creation time of the family's newest token (the sign-in or its latest refresh), and
   `device_name`, which the client sent with the code exchange. It also carries the wording, in the request's time
   zone: `label` (the device name, else the client's name), `signed_in_label` and `last_used_label`.
   `DELETE /me/sign-ins/{id}` revokes the family as `/oauth/revoke` does, answers `204` again for one that ended
   within the last day (after that it's gone, point 7), and `404` for an unknown id or another user's, which it
   doesn't tell apart. The device name is the client's claim and only a label: nothing decides by it, and
   it is validated as text (`domain::oauth::DeviceName`), not trusted. It comes with the code exchange and not the
   authorization request, so the consent page never shows it: a familiar name there could make a phishing request
   look legitimate.
7. Codes, families and their tokens are authentication plumbing, so each successful code exchange deletes a few
   that ended over a day ago: expired codes, and families that are revoked or have no refresh token left unused and
   unexpired. The day keeps a retry hearing why it was refused and ending a sign-in again succeeding. Used refresh
   tokens of a live family stay, since they detect reuse.

### Personal tokens

A machine with no browser and no keychain, such as a cloud session, a container or CI, can't run the sign-in of
[Non-web client tokens](#non-web-client-tokens), and an OAuth refresh token can't be copied there: it rotates, and a
reused one revokes the whole sign-in. So a person makes a personal token in the web app and gives it to the machine.

1. The secret is `uph_` and a random 256-bit value, made as session tokens are, and only its SHA-256 is stored. The
   prefix tells it from an OAuth token, so the bearer check looks in one table, and lets secret scanners find a leaked
   one. It is shown once, in the answer to making it.
2. It never rotates, which is the point: the copy on the machine stays good until it expires or is revoked. The person
   picks the expiry when making it, and "never" is allowed.
3. A token is live when it isn't revoked and hasn't expired. The rows of revoked and expired ones are kept, so a request
   that presents one can be told which it was. They are tiny, and only the holder of the secret can ask.
4. The endpoints that make, list and revoke tokens take the session cookie only, so a token can't make another, and a
   copy that leaks can't be used to keep access after it is revoked.
5. Using a token records `last_used_at`, with a minute's precision: one conditional update, so a burst of requests
   writes once a minute at most.
6. A token has all the permissions of its person, as a command-line sign-in has. A scope that narrowed it would be
   added the way [Non-web client tokens](#non-web-client-tokens) point 1 says.
7. A password change or reset revokes every live personal token, in the same transaction and counted in the sign-ins it
   ends, as it ends the web sessions and the command-line sign-ins. Someone who resets a compromised account must cut
   off a token that was made with the stolen session.

## Audit log

1. Each use case runs in one database transaction, performs the change and inserts the audit entry before
   committing.
2. `action` is a typed enum (`ItemCreated`, `OptionArchived`, `MemberPromoted`, …) and `diff` stores the changed
   fields as before/after pairs.
3. Each action is shown to those who hold the permission it takes (`audit_permission` in `domain::permission`), and
   the read filters on it in SQL so paging stays exact. Members see item, tag and saved view changes, changes to a
   heap's context, and display name changes; admins additionally see heap, parameter, team and membership changes.
   Leaving a team is a membership change, so only admins see it, although any member may leave.
4. The heap log reads what is recorded against the heap. The team log reads the team's own entries and the heaps'
   own entries (created, renamed, archived, context changed), but not what happens inside the heaps. A team tag's
   entries (made, edited, merged) are the team's own, with no heap, so the team log reads them; a merge also writes
   one on each item it changed, in that item's heap, so the item's History says its tag was merged. With
   `entity_id`, either reads one entity's history; the team log with a team tag's ID is that tag's history, and its
   merge entry shows the heap tags it took in (heap, name, items). The heap log then reads that entity's entries
   from every heap of every team the reader is a member of, each team's limited to the actions the reader's role in that team may see,
   in one query, so paging stays exact. A move within a team (`ItemMoved`) is recorded in the heap the item went to,
   and its history from before it still shows after it, with the old heap's tags and parameters named. The log is
   per team and its entries are never copied or re-homed, so a move to another team writes two: one in the heap the
   item left, with the item's title (`item_title`, never shown as a change) and no target, and one in the heap it
   went to, with the heap and nothing of the source team's. Someone in both teams sees the whole trail, and someone
   in one sees that team's part. The old team's log names an item that is no longer in any of its heaps by the
   `item_title` of its latest `ItemMoved` entry there. A turn the move takes back, because its person may not edit
   items in the target heap, is recorded in the source heap.
5. Entries come ready to show: the actor's and the entity's current names, a time label in the request's timezone,
   a `phrase` to read after the actor's name, and `changes`, one per changed field with the values before and after
   as text and IDs replaced by current names. The raw `diff` is returned too. An edited text people write (an
   item's title and context, a step's, a heap's context) also comes with `text_diff`, a word diff computed when the
   log is read (`domain::text_diff`), so nothing extra is stored and old entries get it too. It is the server's, not
   the web app's, so every client shows the same diff, and none when too little stayed the same.
6. A step's changes are recorded against its item (`StepAdded`, `StepUpdated`, with the item as the entity), so they
   read in the item's History and the heap's Activity, shown to those who may edit items. The diff names the step
   by number and title, and each changed field as `step_title`, `step_context` or `step_state`. A comment is
   recorded the same way (`CommentAdded`, "commented on"), with its number and its whole text, so History shows what
   was said. A question's are
   recorded the same way (`QuestionAsked`, `QuestionAnswered`), shown to those who may edit items, with the
   question's number and text and, once answered, the labels picked and the comment; changing a reply is
   `ReplyChanged`, with the picks and comment before and after. So are a question withdrawn
   by closing its item or taking it back from the agent that asked it (`QuestionWithdrawn`) and a turn
   (`TurnOpened`, with its reach, goal, line and source, `TurnTakenBack`, with the goal and why: the person took it
   back, started the item, closed it, or archived the heap, `TurnReached`, with the goal and the stage before and
   after, `TurnRetried`, and `TurnRedone`, with the goal, the stage before and after and the line). Closing an item writes its take back, then a withdrawal for each open question, then its
   own update.
7. Invite links are never deleted. Making and revoking one are team entries (`InviteLinkCreated`,
   `InviteLinkRevoked`) shown to admins, and a join through one is a membership entry (`MemberJoined`) naming the
   link, so a link's own History is its entries by `entity_id`. A link ending by time is not an act and has no entry.
8. The web app shows history where things are edited: under the item, in a collapsed History on each tag,
   parameter, heap, member and invite link, and the whole logs on the heap's and the team's Activity pages. The
   account's own history is under Account's Password card ([Authentication](#authentication-and-authorization)).
   Parameters and heaps offer `view_history` only to those who may see their entries, so members aren't shown
   histories that are always empty.

## Configuration and deployment

1. Configuration comes from environment variables, validated at startup; the variables and their defaults are in
   [configuration.md](../operator/configuration.md). No session secret is needed, because session tokens are random and
   stored hashed rather than signed. The operator's contact line goes out with the public password-reset options
   (`GET /password-resets`), because the sign-in page already reads them before anyone signs in.
2. On startup the server runs pending migrations, then serves the API and the web app on one port. On Postgres the
   run holds a transaction-scoped advisory lock, so instances starting together apply each migration once; MySQL and
   SQLite aren't run with several instances.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at Postgres or MySQL. TLS terminates at a reverse proxy, or at Cloud Run in
   front of a Neon database ([operations.md](../operator/operations.md)).
5. **Health checks:** `/healthz` returns `200` while the process is up. `/readyz` returns `200` only once migrations
   have finished and the database answers a ping, so orchestrators route traffic only to ready instances. Neither
   needs authentication, and neither reveals versions or configuration.
6. **Mail** is optional. The `mail` crate builds the mailer at startup from `MAIL_URL` and `MAIL_FROM` (lettre with
   rustls, no connection pool). SMTP always uses TLS, implicit or STARTTLS. `file://` and `log:` are the development
   transports, and the only places a mail's text is recorded. A failed send isn't retried. `service` sends the mail
   it needs through that mailer (the reset mail, the email change link and the notice of a change, and the confirmation link, see Authentication).
7. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server
   with a clear message before it listens. The database gets time first: a database that scales to zero (Neon) can
   take longer to wake than one try's 10 s timeout, so the connect is retried with backoff, every error alike, until
   `DATABASE_CONNECT_DEADLINE_SECS` has passed. A try may start just before the deadline and run its full timeout,
   so the deadline sits about 15 s under the startup probe's limit, leaving room for that try and the migrations. On SIGTERM it stops accepting connections and drains in-flight requests.

## CLI

`crates/cli` is `upheap`, another client of the API. Its conventions (command shape, output, exit codes,
configuration, signing in, layout and tests) are the [CLI rules](engineering-principles.md#cli) of the engineering
principles; the exit codes are in `crates/cli/src/error.rs`, and the commands and flags in `upheap --help`. This
section is how it maps onto this API.

1. **Scope.** The everyday work in a heap: items (add, show with `--history`, edit, the state verbs, list), a
   task's steps (add, tick, drop, edit), an item's questions (ask, request, review, propose-done) and comments (add,
   list), tags (list, add, edit, promote; a heap's, or the team's with `--team`), parameters (list, add, edit,
   archive, and their options; a team's with `--team`, and a heap's `use` and `stop-using` of one), heaps (list, pick a default) and signing in. Teams, members, heap settings and
   saved-view editing stay in the web app. `parameter edit` sends one write per kind of change (name and weight,
   tags, option order), each at the version the last returned, because the API has no single endpoint for them; a
   failure midway leaves the earlier ones saved and the CLI says which.
   A tag write that would merge heap tags is refused (exit 3) with the reach and the exact command that confirms it,
   with `--confirm <token>` ([API](#api) §11): the CLI has no prompts, and the command names the reach shown.
2. **Naming.** Items are named by key ([Handles](#handles)), or by `team/heap/key` for another heap, which is a short
   link without its `/h/`. A pasted link works, short or in the long form the address bar shows after the redirect;
   its host is ignored and requests go to the configured server. A path that names another heap than `--heap` is
   exit 2. Heaps are `team/heap`, or a heap slug alone when only one of the caller's teams has a heap by it. A step
   is its item and its number, `<item>.<number>` (`k3x9a.3`, `team/heap/k3x9a.3`), split at the last `.` after the
   last `/`, since a link's host has dots and a key never does; `step add --after` also takes the bare number.
   Questions and comments are named the same way in output, `k3x9a.q2` and `k3x9a.c3`.
3. **State changes are one `PATCH` with a `state`,** `reopen` too (as `state: open`, not the reopen endpoint), so the
   server's rules apply as in the web app: the same state is a no-op, the reopen window and `item_closed` are its
   refusals. `item block --by` sets `blocked_by_item_id` and `--reason` sets `blocked_reason` in the
   same request; a bare key there is in the blocked item's heap.
4. **Due times** are a rough choice, matched by label against `GET /due-choices` (ignoring case, `-` as a space) and
   sent with `firm: false`, or a `YYYY-MM-DD` date with `firm: true`. The server owns the list so that a choice
   always reads back with the label it was picked by ([Time and timezones](#time-and-timezones) §5). Every request carries the
   `Time-Zone` header with the machine's zone, so dates and labels read as they would in the browser. The `--due`
   help names the choices from `domain::due`, the rule the server lists them by, since help is read offline; the
   dates and the match still come from the server.
5. **Lists.** `item list` takes the list's query parameters as flags with the same names and values. `--view` loads
   a saved view and sends its settings, with any flag given replacing that setting. The optional text narrows the
   result on the client, as the web app's search does ([Search](#search)). `--tag` takes names, resolved against
   the heap's tags, its own and its team's: in a heap a name is one tag, so a name needs no kind. With
   `--create-tags`, the names the heap doesn't have are added as heap tags first, right before the item is written,
   and they stay if that write fails. `item add` reads the list once more to say where the item landed, a read
   `--quiet` and `--json` skip; if it fails, the add still stands and a warning says so. `--step` goes in the
   create request, so the steps are written with the item and before any turn it opens; against a server that drops
   them, the CLI posts them one by one, and a failure there leaves the item with the steps sent so far, and the CLI
   says which: a note on stderr, or with `--json` the item as it stands on stdout beside the problem body.
6. **Signing in** is the flow of [Non-web client tokens](#non-web-client-tokens), with the machine's hostname as the
   device name. `auth logout` revokes with `POST /oauth/revoke`; `POST /auth/logout` ends only a session cookie and
   answers `204` to a bearer token without revoking it. The keychain holds one entry per server, service `upheap`,
   keyed by the server's URL: the refresh token, and the last access token with its expiry, as JSON. `keyring` uses
   the Secret Service over zbus on Linux, which is pure Rust, so a static binary needs no libdbus. On macOS the CLI
   runs `/usr/bin/security` instead, with the secret on its standard input. A keychain entry lets the program that
   made it read it without asking, and macOS tells programs apart by code signature, so an entry `upheap` made
   itself was closed to its next build: a dialog after every upgrade and every rebuild, which a command run by a
   script or an agent waits on for ever. An entry `security` made stays open to `security`. The cost: any program
   running as the user can read it the same way, without a dialog.
   `UPHEAP_TOKEN`, when set, wins over all of it: the CLI sends that personal token and reads no keychain, takes no
   lock and needs no config directory, and a token never needs refreshing. A `401` while it is set means the
   environment holds a token the server refuses, which `auth login` can't fix, so it is exit 6, not 4, and the message
   says whether the token was revoked, has expired or isn't known. With the reason `password_changed`, the CLI says
   the sign-in, or the token in `UPHEAP_TOKEN`, ended because the account's password changed.
7. **`work run` is the agent loop** over the `/work` routes. It claims the next item (`POST /work/runs`), reads the
   heap and the item, and starts the agent tool behind `AgentTools` (`crates/cli/src/agent/`) with a brief on its
   standard input and `UPHEAP_RUN`, `UPHEAP_SERVER` and `UPHEAP_HEAP` in its environment. The code that starts the
   process, feeds it, watches it for silence and stops it is `process.rs` there, shared by the tools; each tool has
   a module of its own, `claude.rs` and `copilot.rs`, for its command line and what its stream says. While the tool runs it
   sends a heartbeat every 5 minutes by the wall clock, checked every 30 seconds, so one goes out soon after the
   machine wakes from sleep (the monotonic clock stands still in sleep). A failed heartbeat is tried again at each
   check until one is heard, logged each time, and said on stderr from the second in a row, so the one that goes out
   before the network is back after a wake isn't a false alarm. The heartbeat says the loop is up. Claude's streamed
   output says claude is working: it is read by the loop only, saved to the run's log
   (`<config dir>/runs/<run id>.jsonl`, pruned after 7 days) and never sent to the server. Copilot's stream
   (`--output-format json`) is read for one event only, `session.usage_checkpoint`, whose `totalNanoAiu` is the
   session's running usage: the last one is the run's cost, `totalNanoAiu / 1e11` dollars, since a billion nano AI
   units make an AI credit and 100 credits are taken as a dollar (the owner's working figure, not GitHub's published
   price). The other lines are only logged. When the tool shows no
   output for 30 minutes the loop stops it and fails the run with its own message, "The agent showed no activity
   for 30 minutes." That timer uses the monotonic clock on purpose: it stands still while the machine sleeps, as
   claude does, so a sleep isn't silence. The loop stops the tool when the server answers
   `run_over`. A run's model is read from the tool's own stream when it names one, else it is the `--model` the loop
   was given (which it passes to the tool as it is): Claude's `system` event with subtype `init` carries `model`, and
   Copilot's usage checkpoint carries `data.accountingSnapshot.currentModel`, the last one winning. `finish` sends
   the tool's name and that model along with the tool's times, for a run whose tool was started. A run that was
   stopped or taken back left no report, so it records the `--model` given, if any. When the tool ends it sends
   `finish` with the cost the tool reported and, only if the tool itself failed, why: the server decides whether the
   run reached its goal, asked, handed back, is watching, hit the limit, or failed, and
   the loop never works that out. The exception is the agent tool's usage limit, recognised for Claude only: a run whose claude stream had a rejected
   `rate_limit_event` (not overage use) or `api_error` `usage_limit_reached` on its result is sent with `limited`, and
   ends as `limited`, not failed, so the turn's failed runs are left alone. The loop then holds every worker until the
   reset plus a minute (30 minutes when no time is known), by the wall clock like heartbeats, with live runs ending as
   usual; the queue then claims that item first (see the queue's order above). A Copilot run that stops at its limit
   is a failed run like any other.
   With `--once` the loop says when to run again and exits. With `--workers N` the loop runs N workers on one task, each claiming its own item;
   the server's claim keeps them apart, and one Ctrl-C reaches them all. Above 1 worker it needs `--root`, because
   runs in one directory would trample each other. The tool gets what its goal needs and nothing more: to prepare an
   item it may run the `upheap` commands a run uses, read files under the directory it runs in and use the web; to do a task it may also edit files, and every other command follows the
   person's own Claude settings, since Upheap keeps no tool list of its own. Copilot's rules can't allow one
   subcommand of a program, only the whole program (`shell(upheap item:*)` allows nothing, `shell(upheap:*)` does),
   while a deny rule can scope one; and with `UPHEAP_RUN` set the server already refuses whatever is outside the run's
   own item. So a Copilot run is allowed all of `upheap` and denied `upheap auth` and `upheap heap use`, which write on the
   person's own machine, last, since a
   deny wins over every allow, `--allow-all-tools` included; to do a task it has file edits accepted too, and
   the person's saved Copilot approvals decide the rest. With `--root` the loop names one
   directory for each item, `<root>/<team>/<heap>/<key>` from the claimed run; the loop makes it if it is
   missing and the tool writes its own instructions into it (`CLAUDE.md` for Claude, `AGENTS.md` for Copilot, with the same text),
   and starts there. The path is the whole
   record, with no index file to fall out of step. Later runs on the item start in the same directory as it was
   left, so a clone and its caches are paid for once; the agent's session does not carry over. Each start rewrites
   `.upheap-run` in it, naming the server, and that file's modification time is when a run last started or ended there. The
   run holds a shared lock on that file until it ends. The loop sweeps the
   root when it starts and about hourly before a claim, one worker at a time, and removes a directory whose
   marker is 7 days old, or whose item is closed or answers 404 (deleted, or no longer the person's) while
   its heap still reads fine: the server answers 404 for a heap the caller can't see too, so another account's
   directories on a shared root are left alone. Only the server the marker names is asked; another server's
   directories go only for age. Any other failure to read an item keeps its directory, so a server error or a changed permission never deletes work,
   and a server out of reach ends the sweep. Only directories at that depth with a marker are looked at, so the
   old `<key>-<run id>` ones and anything else are never touched, and a directory a run of this loop is in or
   about to start in is skipped. A removal takes the marker's lock exclusively, so a run in another loop on the same
   root keeps its directory too, checks the marker wasn't rewritten since the sweep listed it, renames the directory
   away under the lock that guards that set, and deletes it after releasing both, since deleting a clone is slow. A
   run that opened the marker just before finds it moved once it gets the lock, and makes the directory again. There the tool is allowed any command, edit and read, passed on its
   command line, since Claude ignores a settings file's allow rules in a directory nobody has trusted. That is
   wide for now, so in such a directory the limits above don't hold. The runner can add allow rules of their own with
   `--allow` (every run) and `--allow-prepare` / `--allow-do` (one access), passed on the command line after Upheap's
   own and before the denies of `upheap auth` and `upheap heap use` that every run gets, so a tool the runner's instructions ask for can be
   allowed for the loop without loosening every session on the machine. With `--instructions` the brief also carries
   the runner's own instructions, between Upheap's and the heap's context: a heap's context is shared by everyone who
   works the heap and says what the work is, and how it is done is for the one who does it, so each
   runner keeps that in files of its own (a shared one and one for its machine, say) and it wins over the context on how. The tool runs in its own process
   group, so a Ctrl-C typed in the terminal reaches the loop and not the tool: the first lets the run end and stops
   the loop, the second stops the tool, and the run counts as failed unless it had asked a question. Stopping asks the tool
   to end and kills its group after a few seconds if it hasn't: Claude runs each command in a group of its own and
   ends them only when asked, not when killed. `SIGTERM` does what the second Ctrl-C does; `SIGHUP` is left
   alone, so `nohup` keeps a loop going.
   Once the tool is gone, whatever it started and left running (a dev server, a watcher, a command Claude didn't end
   because it was killed) is ended too, since it would keep its CPU and ports and go on writing into the item's
   directory. Those processes are found by the `UPHEAP_RUN` entry in their environment, which they inherit and which
   survives `setsid` and reparenting, where a process group or a process tree doesn't, and there are no cgroups on
   macOS. Those of the run's id and the user's own get `SIGTERM`, then `SIGKILL` after 3 seconds. The sweep does the same
   to the processes whose working directory is in a directory it is about to delete, once it is renamed away, since
   a process still writing there makes the delete fail; that also covers what runs from before this existed. Reading
   other processes' environment and directory is done through `sysinfo`, as `unsafe` is forbidden. The loop reads
   the wall clock for every request, not the time it started, so a loop that lives for hours, through the machine's
   sleep, refreshes its access token like any command.
8. **A newer server's values.** A new enum value is additive
   ([engineering principles](engineering-principles.md#the-contract), The contract §4), so a CLI built before it
   still reads the response. The `allowed_actions` lists of every response type skip values the client doesn't know
   (`deserialize_with`), and `Factor` (contributors), `HeldBackGap`, `ClosedGroup` and the audit log's `AuditAction`
   read an unknown value as `Unknown`, a variant
   marked `#[serde(other, skip_serializing)]`, which keeps it out of the OpenAPI schema and can never be sent; the
   CLI prints a contributor's `label`, so an unknown factor reads as the server wrote it. An item's `Kind`, `Stage` and
   state (`ItemState`, which gained `became`), and a step's `StepState`, are read in responses and sent in edits, so they take the same `Unknown` variant and the server refuses it in a
   request when converting to the domain type, with `422 value_unknown` naming the field, rather than at parsing
   (`400 invalid_body`). Other values the server reads in requests stay strict: `Need`, `Size`, `Role`,
   and the list view's `StateMode`, `BlockedLayout` and `ListSort`. Saved views share the last three with the list query, so a new sort or state mode in a saved view
   would still stop an older CLI reading the heap's views; making them lenient the way `Kind` is, left for when it
   happens.

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
   localhost port, and checks stdout, stderr and the exit code. The tests sign in through the real OAuth endpoints,
   most by seeding the token store with tokens from a code they approved and exchanged, the login tests by running
   `upheap auth login` with a browser that answers the consent page. The `UPHEAP_TOKEN` tests make a personal token
   through the API and run with an empty keychain and no config directory.

The commands are in [README.md](../../README.md#develop).
