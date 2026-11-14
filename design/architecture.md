# Architecture

How Upheap is built, and why. Companion to [product.md](product.md). The coding rules are in
[engineering-principles.md](engineering-principles.md), and the choices that could have gone another way, with
their reasons, are in [decisions.md](decisions.md).

This doc explains mechanisms and reasons. What the code states, it doesn't repeat: the endpoints are in
`openapi.json` (`upheap-server openapi` prints it), the error codes in `crates/api/src/error.rs`, the schema in
`crates/store/src/migration`, the configuration variables and the crate layout in [README.md](../README.md).

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

| Area | Choice |
| --- | --- |
| Backend | Rust (stable), a Cargo workspace |
| HTTP | axum on tokio, tower middleware |
| API contract | Code-first OpenAPI 3.1 with utoipa. The web app's TypeScript types are generated from it with openapi-typescript, and CI fails on drift |
| Data access | SeaORM 2 over sqlx, with the backend chosen at startup from `DATABASE_URL`: SQLite, MySQL or Postgres |
| Migrations | sea-orm-migration, embedded in the binary and run at startup |
| Auth | argon2id for local passwords; an own session table with hashed random tokens for the web app; own OAuth 2.0 authorization-code endpoints with PKCE for non-web clients. External identity providers (OIDC) are planned, not built |
| Web app | React, Vite, TanStack Query and Router, Tailwind, React Aria Components, Motion; embedded in the binary with rust-embed |
| Observability | `tracing` with JSON logs, request IDs, `/healthz` and `/readyz` |
| Testing | `cargo test` with googletest matchers, proptest for ranking invariants, MySQL and Postgres service containers in CI, Playwright end to end |
| Packaging | A static binary in a minimal Alpine image; docker-compose with SQLite on a volume; Cloud Run with Neon Postgres in production ([operations.md](operations.md)); the CLI as static binaries on GitHub Releases, with a Homebrew cask in `bzzzzzz/homebrew-tap` ([operations.md](operations.md#releasing-the-cli)) |

## Workspace

Dependencies point one way: `server → api → service → store, domain`, with `api → api-types → domain` beside it, and `server → mail`.
`domain` depends on nothing in the workspace, so ranking
and the state rules are unit- and property-tested in isolation, and `store` and `api` never depend on each other.
`api-types` holds the request and response types with no IO, so a Rust client such as `cli` shares them without
linking the server. The crates and their roles are listed in [README.md](../README.md#layout); each crate's root
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
- **An item's origin** is `items.origin_id`: the note it was split from (`item add --from`), or the item of the run
  that added it, with `created_run_id` set beside it then. `made_in_split` marks the first kind: only those are what
  the note became, so a follow-up a run adds doesn't count toward `nothing_made`, the run's reached check or the
  Became list. All three are set when the item is made and never changed, and stay when either item later moves to
  another heap, as a blocker's link does. An index on `origin_id` serves the lookup of what a note became, which the
  item and the closed list show as `became`, each with the heap it is in now: a handful of items per note, and at
  most one page of closed items, read in one query.
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
  later never renames a step. Steps are never deleted: a dropped one stays. A task has at most 100, which is the size
  assumption that lets an item response carry all of its steps: only a single-item response does, and the ranked
  and closed lists carry `steps: null` and just the progress, counted in one grouped query per chunk without
  loading the steps. Progress is ticked of open plus ticked, so dropped steps don't count. A step write takes the
  heap lock like any item write, then reads the item and its steps again with locking reads (see
  [Cycle detection and locking](#cycle-detection-and-locking)), and answers with the whole item. Steps of a closed
  item are read-only, and come back editable, unchanged, when it is reopened.
- **An item's questions are rows of their own** in `item_questions`, built like steps: each has its own `version`, so
  a question write never bumps the item's and an item write never bumps a question's, and a `number`, one past the
  highest the item has had and never reused, which is its name (`k3x9a.q2`). Questions are never deleted: an answered
  one stays, with its reply (`reply_picked`, indices into `choices`, and `reply_comment`), and one withdrawn when its
  item was closed stays, withdrawn (`withdrawn_at`, `withdrawn_by`). An item has at most 100,
  so a single-item response carries all of them and a list row carries `questions: null`. Asking and replying take
  the heap lock, then read the item and its questions with locking reads, like a step write, and answer with the
  whole item. A closed item takes neither, and closing an item withdraws its open questions in the same transaction,
  one `question_withdrawn` entry each, so reopening doesn't bring them back and nothing waits for the person on a
  question no one can answer. An item waits for the person while it is open, in an active heap, and has an open question; that is
  derived, not stored: `waits_for_you` on every item response comes from the questions a single-item response already
  carries, and for list rows from one grouped count of open questions per listed item, as step progress is.
  **`GET /me/waiting`** lists the open questions of open items in the active heaps of the caller's teams, one row per
  question, across heaps: one query for them (joined to their items), then one ranking per heap that has any, the
  same inputs and policy the heap's list ranks by (`RankedItem.score`, blocker inheritance included), and the users
  who asked. Rows are ordered by the item's score in its own heap, highest first, then heap name, the item's
  position and the question number; heap weights, when they come, multiply the score. A question on a reminder to
  come has no score: its rows come after the ranked ones, by the reminder's date, soonest first. It returns at most
  500 rows, with `count` the true total, which is the size assumption that lets it skip paging. A request is a question
  whose choices the server fixes as Done and Can't be done, so every kind shares one reply shape.
  `details` and `reply_comment` are `MEDIUMTEXT` on MySQL.
  **A review and a done proposal are question kinds** with fixed choices too (Approved or Changes asked; Close as done
  or Not yet), asked only by an agent in a run whose turn goes to done, on a task. A review's link, piece (the link
  itself when none is named) and round live on its row; the round is fixed when it is asked, as one more than the
  item's answered reviews of that piece, so later events never change it, and a withdrawn or open review doesn't count.
  A done proposal is refused, with every blocker listed, while any question or step is open or a reviewed piece's
  latest round isn't approved (`check_close_ready`); a task with no reviews can be proposed closed.
  An agent never closes a task: its `PATCH` to done is always refused. The person's reply Close as done closes the task
  in the reply's transaction, by the same path as their own Done: the open turn ends as reached by the person, other
  open questions are withdrawn, and the items it blocks are freed.
- **An item's comments are rows of their own** in `item_comments`: an append-only thread under the item's context,
  so an agent or a person adds to an item without rewriting its context, which could change the author's words and
  clash with their edits. A comment has no `version` and is never changed or deleted, and adding one never bumps the
  item's `version`, so it never conflicts with an edit and needs no `If-Match`. Its `number` is one past the highest the
  item has had, given under the heap lock with a locking read like a step's, backed by a unique index on `(item_id,
  number)`, and never reused; it is the comment's name (`k3x9a.c3`). Any item takes one, open or closed, in an active
  heap. An item has at most 200, and the 201st is refused (`too_many_comments`): the size assumption that lets a
  single-item response carry all of them, at most 200 times 20,000 characters, while a list row carries `comments:
  null`; raising it means paging them. Moving an item keeps its comments, which hang off its ID. `body` is
  `MEDIUMTEXT` on MySQL.
- **An item's turns are rows of their own** in `item_turns`: an item handed to an agent, with a reach (one state, or
  all the way), the stable state it goes to (`goal`, worked out from the item when the turn opens, or picked by the
  person: defined, for a note as written; then stored), the person's optional line (`MEDIUMTEXT` on MySQL), who opened it and whether the person sent it or the heap's
  prepare setting did, then when and by whom it ended and how (taken back, handed back, or reached) and how many of its runs in a
  row failed (`failed_runs`).
  Turns are never deleted, and a write to one never bumps the item's `version`, so sending an item doesn't conflict
  with someone editing it, and sending and taking back need no `If-Match`. A turn sent all the way still goes one
  stable state: when a run reaches it, the next turn opens in the same transaction with the same reach, line and
  opener, going to where the item is by then, so the item never has a moment with the person. An item has at most
  one open turn (`ended_at` null); the service keeps it so under the heap lock every item write takes, reading with locking reads,
  so no constraint is needed. A turn is taken back by the person's own request, and implied by starting or closing the
  item (in the same transaction as the change, with its own `turn_taken_back` entry first) and by archiving the heap
  (one entry for each). Who has an item is derived, not stored: `with_agent` is the item being open, its heap active,
  an open turn, and no open question; list rows get their open turns from one grouped query, like question counts.
  `heaps.agents_prepare` makes `create_item` open a turn of one state on every new item, whoever added it. Neither
  changes ranking.
- **A turn is worked in runs,** rows of their own in `item_runs`: one go an agent has at the turn of an item, with who
  claimed it, when it started and was last heard from (`heard_at`), the item's stage then, and, once over, how it
  ended (`reached`, `asked`, `failed`, `taken_back` or `handed_back`), what it said it reached and made (`result`, `links`), why it
  failed, and its cost in micro-dollars. `work run` also sends the agent tool's own start and exit times with `finish`
  (`tool_started_at`, `tool_ended_at`, each set once, null for older runs and for runs whose tool never started).
  Like turns, runs are never deleted and never bump the item's `version`.
  `GET /work/next` and `POST /work/runs` pick the same item: the first of the given heaps' items that are open and not
  blocked, in an active heap, with an open turn that isn't stuck, no open question and no live run, ordered by the
  item's score in its own heap (`rank_open_items`, once per heap), then heap name, then position. A claim takes the
  locks of every given heap in ID order, as the item writes do, so two claims at once get different items.
  **There is no lock to give up: a claim lapses.** A run is live while it has not ended and was heard from less than
  `CLAIM_LAPSE` (30 minutes) ago, by a heartbeat or a write made in it. A run past that counts as ended, failed,
  wherever it is read, and is written as failed lazily, the next time a claim over its heap, its own `finish` or a
  take back touches it; no job sweeps for them. A turn is **stuck** when `STUCK_AFTER` (3) of its runs in a row
  failed: a count on the turn, set back to 0 by a run that asked a question or by Retry. A lapsed run not written yet
  counts in that number too: wherever a turn is read, the queue, the item, a list row, Waiting for me, `Turn::counting_lapse`
  adds one to its count when it has such a run, so a turn whose third run went quiet is stuck at once, though nobody
  has claimed since. Retry writes that lapsed run as failed, then resets the count, so it doesn't count twice. A stuck
  turn is out of the queue, its item `waits_for_you` and no agent `with_agent`, and Waiting for me lists it as a row of
  its own kind with the latest failure as the reason (the lapse text, when a lapsed run is the latest). Taking a turn back ends its live or lapsed runs as `taken_back`, which is no
  failure, and withdraws the open questions asked in a run of it; the person's own stay open. A turn also ends as
  reached, with no run, when its item's stage is at the turn's goal (`Turn::goal_met_by`; never for done): at the
  person's edit that sets the stage (`how: stage_set`), or, as a safety net for any other way an item got there, when
  a claim finds it (`already_met`) before choosing, so a met turn never costs a run; `next` skips such turns without
  writing. Runs and questions end as for a take back, and a turn sent all the way opens the next. `work hand-back`
  ends the run as `handed_back` (its reason is the run's `result`) and the turn likewise, with no follow-on.
  **Writes in a run:** a client sends `Upheap-Run: <run ID>`, which `CurrentUser` reads into `Principal::run`. A write made in a run must be on the allowlist in
  `extract.rs` (the item's `PATCH`, steps, questions, comments, creating an item, closing a note as became, and the run's own heartbeat, reached, hand-back and finish) or it is
  `run_forbidden`; the service then checks, under the heap lock, that the run is the caller's, live, with its turn
  open, and on that item (`check_run_write`), refuses any `PATCH` field but the context, `kind: task` and starting,
  and touches `heard_at`. Starting an item in a run keeps its turn.
  **Adding an item in a run** (`check_run_add`) may target any heap the person can edit, in any of their teams. It
  locks the heap of the run's item and the target heap in heap ID order (once when they are the same, as
  `claim_run` and `move_item` do), then reads the run under lock and checks it is live; it refuses a reminder and an
  item with no context. The item gets `origin_id` (the run's item) and `created_run_id`, starts `with_context` and
  opens no Prepare turn. A run may add steps to an item whose `created_run_id` is that run, or that was split from
  the run's item (`check_run_step_add`), and no other write to it. An item shows its origin only to someone who can view the origin's heap; `added_in_run`
  shows to everyone. **A run splits its note:** an item made with an `origin` (`item add --from`) in a run must name
  the run's own item, with a need no higher than the note's, and may start blocked only by an item a run of the same turn split
  from the note (`made_in_split`, `origin_id`, and `created_run_id` naming a run of that turn). A person's split sets `origin_id` alone. `POST …/became` in a run
  checks that no question is open and every open task split from the note, in the note's heap, has a context and
  a step
  (`split_reached_check`); then, in one transaction, those tasks become defined, the note closes as became, and the
  run and turn end as reached with no follow-on on the note. If that turn was sent all the way, each of those tasks
  not started and without an open turn gets one in the same transaction (`Turn::passed_on`: the same reach, line, source and opener,
  to done); a follow-up a run added isn't split from the note, so it gets none. A later `work reached` on that run succeeds and does nothing. Audit entries of a write in a run carry
  `audit_log.run_id`, and so does a question (`item_questions.run_id`). The audit API sends an entry's run (`run`, with
  the run's item and its heap now when the caller may view that heap) and the web app marks the entry as the agent's;
  a question's run isn't shown yet. `work reached` checks
  the item as it is now against the goal (`reached_check`) and ends the run and the turn together, moving the item's
  stage to the goal, unless the person has taken it further meanwhile: it never moves back. A run may name a stage
  at or past the goal (`reported_goal`): in a turn to with context, `defined` checks and moves to defined instead, so
  a note that comes out defined enough is a defined task in one turn. The turn's goal doesn't change; the
  `turn_reached` entry's stages before and after show the skip. Each refusal of a run
  has a code of its own, and the one for a write to another item names the run's item in a field, so the CLI's hint
  never has to read a message.
  **Redo** is the person's way to say a turn that reached a stable state short of done isn't right: it marks that turn
  `redone` (`redone_at`, `redone_by`), puts the item's stage back to where the turn's first run found it, and opens a
  turn to the same reach and goal with the person's line, in one transaction. Only the item's latest turn can be
  redone, and only once; the kind isn't changed back and what the agent wrote stays.
- **State and `closed_at`:** in the database, `closed_at` and `closed_by` are set exactly when the state is done,
  won't do or became (a note closed because items were made from it; only `POST …/became` sets it). In `domain`, the state is an enum where only the closed variants carry them, so an open item with a
  close time can't be built. The store converts between the two and rejects rows that break the rule.
- **Case-insensitive uniqueness** (tag names per heap) is a lowercased key column with a unique index rather than a
  collation, because MySQL compares case-insensitively by default and SQLite doesn't. Where an index can't express
  the rule, as for parameter names, which must be unique only among parameters in use, the service checks under the
  heap lock.
- **Insert-only tables:** the application can only insert into `audit_log`. There is no update or delete code path
  for it.
- **The heap's ranking policy** is its row: the need, age, deadline and value weights on `heaps`, plus each
  parameter's weight. The weights change together through one endpoint, in one transaction under the heap lock.
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

| Thing | Handle | Unique within | Example | Who sets it |
| --- | --- | --- | --- | --- |
| User | username | everyone, shared with teams | `eugene` | the user, once, at sign-up |
| Team | slug | everyone, shared with users | `my-team` | admins; derived from the name if not given |
| Heap | slug | its team | `backlog` | admins; derived from the name if not given |
| Item | key | its heap | `k3x9a` | the server, at random |

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
   ever `409 handle_taken`.
3. **Item keys** are five characters from Crockford's base32 alphabet in lowercase (no `i`, `l`, `o` or `u`), with
   at least one digit, so they're easy to read aloud and type and can't spell real words: about 30 million per
   heap. They're random, so a key says nothing about when an item was made or how many a heap has. The key is drawn
   inside the item's insert transaction, under the heap lock every item write takes, and checked against the unique
   index on `(heap_id, key)`; a clash is retried, and with a few thousand items one happens about once in several
   thousand inserts. `domain` turns random bytes into a key, and the service draws the bytes from the
   `RandomSource` it's given, like its clock, so tests can script them. Items are never deleted, so a key is never
   reused within a heap, except that one an item took with it when it moved to another heap can be drawn again, about
   one chance in 30 million per insert. An item that moves to another heap of its team keeps its key unless the
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
   and heap slugs can change, and an item's heap can, by a move. An old link stops working when any of them does;
   redirects from old handles are out of scope for now.
7. **Sign-up** asks for a username with the display name, email and password; the personal team takes it as its
   slug, and the first heap is `personal`. A first OIDC sign-in will ask for a username before creating the
   user.

## Ranking

### Computation

Ranking is a pure function in `domain`: the heap's policy, its non-closed items with their parameter values and
blocking edges, and an `as_of` time go in; positions, contributors and held-back entries come out. Nothing about scores is stored.

1. The service loads everything the heap needs and ranks in memory on each request. Heaps are expected to hold at
   most a few thousand open items, so this is cheap. A per-heap cache invalidated on write can be added if
   measurements call for it.
2. Each factor (need, age, deadline proximity, value per effort, each custom parameter) produces a weighted
   component. Age counts from when the item joined the list: its creation, or for an item that was a reminder, its
   `arrives_at` (`ItemForRanking::from`), so a reminder arrives ranked as due that day rather than as an old item.
   The score combines them. The formula is a named version (`FormulaVersion::V2`) behind one
   function, set in code rather than stored per heap: tests pin orderings per version, so a formula change is an
   explicit test update, and since scores are never stored, switching versions needs no migration. The module doc of
   `domain::ranking` explains the current formula.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are refused on write. An item's
   effective score is the greater of its own and the boosted effective scores of the items it blocks, computed by
   one memoized walk, so it's linear in the number of items and carries through chains. Ties go to the item blocking
   the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
4. **Contributors** are the largest weighted components, each with a label and a relative share ("Deadline in a few
   days", "Blocks *Migrate auth*"). Scores themselves are never returned.
   **Held back by** names what the person could change to lift an item: a low need, no due date, unset sizes. A
   factor counts while its weight is above 0 (the need while its weight or the age weight is), and unset sizes are
   labelled with the formula's defaults. It is empty for a blocker ranked by what it blocks; the service also
   empties it for the item listed first in its group, which is the one the item pane calls "Why it's #1", and for
   every item in a group sorted by something other than priority, where those fields don't move it.
5. **Ties** that the chain length doesn't settle go to the older item, then to the ID, so the order is fully
   deterministic.

A ranking preview ranks the heap with draft weights beside the stored ones, at one `as_of`, and returns the top of
the default view's ranked group with each item's position under both. It stores nothing.

### Across heaps

`GET /me/items` is the ranked list across every active heap the caller may view. Each heap that has any non-closed
item is ranked and grouped under the request's view by the same code as its own list, so a row is what that list
would show. Then each group (pinned, ranked, blocked) is merged across heaps by the item's score in its own heap,
highest first, then heap name, then the item's position in its heap, the same rule `GET /me/waiting` orders by.
Since a heap's group is already in score order, the merge keeps every heap's own order. Heap weights, when they
come, multiply the score. The view takes no tag, parameter or sort, which belong to one heap. It returns at most
500 rows, pinned first, with `count` the true total: the size assumption that lets it skip paging.

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
6. If heaps grow beyond this size, search moves to the server with per-database full-text indexes behind a `q=`
   parameter.

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

1. Every item, tag, parameter and saved-view write first locks the heap row (`SELECT … FOR UPDATE` on MySQL and
   Postgres), before touching any other row, so writes in one heap run one after the other and always take locks in
   the same order. The write checks that the heap isn't archived on the row read under that lock, and archiving
   takes the same lock, so no write lands in a heap archived while it waited. Reads that a rule depends on (the
   blocker chain, a parameter and its options, a saved view, an item and its steps for a step write) are locking
   reads too, because on MySQL a plain read before the lock would fix the transaction's snapshot and hide changes
   that committed while it waited. SQLite has
   one writer at a time, so there the second of two conflicting writes fails instead of waiting; the outcome that
   matters, no inconsistent state, holds either way.
2. A move of an item to another heap of its team locks both heaps in heap ID order, as the agent's queue locks
   several, then reads the item with a locking read and checks its version. Two moves in opposite directions can't
   wait for each other, and the second of two moves of one item finds it changed. The item's own blocker is part of
   its old heap's state, so the moved item stays blocked with nobody picked. The items that waited for it keep it as
   their blocker, now in another heap of the team, so they wait for the person (§3), and a move back makes it hold
   them again; the move writes nothing to them. Since an item's heap can change, a write that finds the
   heap through a read before the lock (a step, a question, a turn, a run) reads the item again under the lock and
   refuses with `409 version_conflict` when it has left that heap (`find_item_locking_in`).
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
   parameters, options and views keep their IDs, checked against the heap in the path. A handle that's malformed or
   names nothing the caller can see is `404`. Responses carry both the `id` and the handle; request bodies refer to
   other things by ID, since clients have the objects in hand. Creating a team or heap takes an optional slug,
   derived from the name when left out.
4. Updates are `PATCH` with partial bodies and the expected `version` in `If-Match`. Endpoints that say what
   something should end up as (archive, unarchive) take no `If-Match`, so repeating or racing them is safe, and a
   write that is already applied succeeds (adding a member who is one, removing someone who isn't). Request bodies
   refuse unknown fields, so a misspelt field fails instead of saving nothing.
5. List endpoints paginate with a cursor, except the ranked list ([Search](#search)).
6. Responses are ready to show, so clients compute nothing: `due` as `{ at, firm, label, overdue }` in the
   request's timezone, `contributors`, who did what and when as a `UserRef` and a label ("4 days ago"),
   `closed_group` as a code the client has headings for, `allowed_actions`, and `null` for unset values.
7. Responses return the whole aggregate: a team with its members and heaps, each with their `allowed_actions`, so a
   client re-renders from one response after any change (promoting someone changes what can be done with every
   other admin); a parameter with its options and tag links; `GET /me` with the caller's teams and heaps and their
   counts.
8. Saved views belong to the heap, not to whoever saved them: every member sees, changes and archives them, as with
   tags. An archived view is left out of the list and refuses edits; there is no unarchive, since saving the view
   again is as quick.
9. Archiving an option switches the open items that have it to the replacement and bumps their versions, so an edit
   made from the old value fails with a version conflict instead of undoing the switch. A closed item keeps the
   archived option; when it's reopened, its values are brought up to date the same way, in the reopening's audit
   entry.
10. A display name change is audited once in each of the user's teams, since the audit log is per team and that is
    where teammates see the name. A user in no team has nowhere to record it.

## Authentication and authorization

1. **Web app:** a cookie session (`HttpOnly`, `Secure`, `SameSite=Lax`). The cookie holds a random 256-bit token;
   the database stores only its SHA-256, so a leaked database can't be used to sign in, and no session secret is
   needed. Every state-changing request, sign-in included, must carry an `X-CSRF-Protection` header. Browsers only
   let same-origin scripts set custom headers (cross-origin ones need a CORS preflight, which the server never
   approves), so the header together with `SameSite=Lax` blocks cross-site request forgery without a per-session
   token.
2. **Non-web clients:** the OAuth 2.0 authorization code flow with PKCE (S256) against the server's own endpoints
   (RFC 6749, 7636 and 7009).
   - The client opens the browser on the web app's `/oauth/authorize` page and listens on a loopback redirect URI.
     The page checks the request with the API before it shows anything, so it never offers a request the server
     would refuse. The user signs in if needed and chooses Allow, which approves the request with the session cookie
     and CSRF header (a bearer token can't approve more access for itself), or Cancel. The client then exchanges the
     code at `/oauth/token` for an access token and a refresh token.
   - Only registered first-party public clients are allowed, and the registry is a constant in `domain::oauth`, not
     a table. Redirect URIs are loopback only (RFC 8252), on any port, with no `localhost`, other hosts, user info,
     query or fragment, so adding `?code=…` always gives the URI that was approved.
   - Codes and tokens are random 256-bit values stored as SHA-256 hashes, like session tokens. Codes work once and
     briefly; access tokens are short-lived and refresh tokens rotate ([Non-web client tokens](#non-web-client-tokens)).
   - `Authorization: Bearer` resolves to the same `Principal` a cookie does, on every endpoint. A request with a
     bearer token is authenticated by it alone, with no fallback to the cookie, and needs no CSRF header: a
     cross-site page can't set that header either, and CSRF is a cookie problem. Other `Authorization` schemes, such
     as the `Basic` credentials a browser sends to a proxy, are ignored and the cookie rules apply.
   - No page of the web app may be framed by another site (`X-Frame-Options: DENY`, `Content-Security-Policy:
     frame-ancestors 'none'`), so a page can't hide the consent page under something that gets the user to click
     Allow.
   - A request that's wrong in some other way than its client or redirect URI is shown as an error on the page, not
     sent back to the client as RFC 6749 suggests: the only client is first-party, so such a request is a bug the
     person at the browser should see, and the CLI gives up waiting after a few minutes.
   - A proxy that puts HTTP Basic auth in front of the server works for the web app but not for the CLI, since a
     request carries one `Authorization` header. Such a proxy has to let `/api` through.
3. **External identity providers** (planned): generic OIDC, configured per deployment. A first sign-in creates the
   user, after asking for a username. An identity is matched by `(provider, subject)`, never by email alone.
4. **Local accounts:** passwords are hashed with argon2id, sign-in takes an email address or a username, and
   attempts are rate-limited per account, keyed by its email address whichever was typed, or per unknown address or
   username.
   - A fixed number of attempts per window (five per 15 minutes). Later attempts get `429` with `Retry-After`,
     refused before any password is checked, so they cost no argon2 work. A successful sign-in clears the count.
   - Unknown addresses are counted the same way, so a refusal doesn't reveal whether an account exists.
   - Counts live in the database (`sign_in_attempts`), keyed by the SHA-256 of the normalised address, so they hold
     across instances. Each attempt is one upsert in a transaction, so concurrent attempts are each counted.
     Addresses are compared exactly on every database (`utf8mb4_bin` on MySQL), or MySQL would find one account
     under many spellings, each with its own attempts.
   - There is no per-IP limit, because the server sits behind a proxy and doesn't know client addresses. Spraying
     one password across many accounts is left to the reverse proxy.
   - A password change (`POST /me/password`, the session cookie only) takes the current password. Each try counts
     against the same limit per session, not per account, so someone using up the account's sign-in attempts can't
     block the owner's change; a wrong current password also adds one to the account's sign-in count. Then, in one
     transaction with the new hash and its audit entry, it deletes all the user's sessions and opens a new one for
     the caller, so a copy of the old cookie stops working too, revokes all their token families, and expires their
     unused authorization codes. It also cancels the user's working reset link.
   - A forgotten password is reset with a **reset link**, because the operator who runs the server is the one who can
     vouch for a person when it has no mail set up. The operator makes it with `upheap-server user reset-link`
     (`create_password_reset_link`), which opens the database and builds the use cases like the server but runs no
     migration and listens on nothing. The link is `{BASE_URL}/reset/{token}`: a 256-bit random token whose SHA-256 is
     stored (`password_reset_links`), shown once. It works once, for an hour, and one is working per user: making a
     new one cancels the earlier one, and so does a password change, in the same transaction. Its state is derived
     from `used_at`, `cancelled_at` and `expires_at` (`domain::password_reset`), and using it is a conditional update,
     so of two requests racing for one link exactly one wins. Setting a password with it
     (`POST /password-resets/{token}`, no sign-in) ends every sign-in of the user as a change does, opens a session for
     the caller whoever they were, and clears the user's sign-in attempts, since the holder of the link has shown who
     they are. Not rate limited, because a 256-bit token can't be guessed. Making a link is audited with the user as
     its actor, since an entry needs an account and the operator has none; its diff says `made_by: operator`.
   - A user's **account history** is the audit entries about them in their personal team, where their account entries
     go: a password change, a reset link made for them (actor: the user, for the reason above) and a password set
     with one. `GET /me/history` reads exactly those three actions for the caller, with no way to name another user or
     team, so only the user sees them and team admins never do. It is not the team log: the server words each entry
     as one sentence with no actor ("Password changed. 2 other sign-ins ended.") from the entry's diff, and the
     sentence is made on reading, so a wording fix reaches old entries. Cookie or bearer token may read it.
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

1. Every resource resolves to exactly one team: heap → team; item, tag, parameter, option and view → heap → team.
   The service resolves the owning team first, then loads the caller's membership in it.
2. A caller who isn't a member gets `404`, the same as for a resource that doesn't exist, so an outsider can't probe
   for IDs. A member without the permission gets `403`.
3. Every repository query that loads heap-level data filters by the resolved heap, so an ID from another heap can
   never leak in; one sent in a request body is refused with `422`.
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
   own entries (created, renamed, archived, context changed), but not what happens inside the heaps. With
   `entity_id`, either reads one entity's history. The heap log then reads that entity's entries from every heap of
   the team, because an item's entries are recorded in the heap it is in after the write: a move (`ItemMoved`) is
   recorded in the heap the item went to, and its history from before it still shows after it, with the old heap's
   tags and parameters named. The log is per team, so an item can only move within its team.
5. Entries come ready to show: the actor's and the entity's current names, a time label in the request's timezone,
   a `phrase` to read after the actor's name, and `changes`, one per changed field with the values before and after
   as text and IDs replaced by current names. The raw `diff` is returned too.
6. A step's changes are recorded against its item (`StepAdded`, `StepUpdated`, with the item as the entity), so they
   read in the item's History and the heap's Activity, shown to those who may edit items. The diff names the step
   by number and title, and each changed field as `step_title`, `step_context` or `step_state`. A comment is
   recorded the same way (`CommentAdded`, "commented on"), with its number and its whole text, so History shows what
   was said. A question's are
   recorded the same way (`QuestionAsked`, `QuestionAnswered`), shown to those who may edit items, with the
   question's number and text and, once answered, the labels picked and the comment. So are a question withdrawn
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
   [README.md](../README.md#configuration). No session secret is needed, because session tokens are random and
   stored hashed rather than signed.
2. On startup the server runs pending migrations, then serves the API and the web app on one port. On Postgres the
   run holds a transaction-scoped advisory lock, so instances starting together apply each migration once; MySQL and
   SQLite aren't run with several instances.
3. **Default deployment:** the Docker image with SQLite on a mounted volume.
4. **Production:** `DATABASE_URL` points at Postgres or MySQL. TLS terminates at a reverse proxy, or at Cloud Run in
   front of a Neon database ([operations.md](operations.md)).
5. **Health checks:** `/healthz` returns `200` while the process is up. `/readyz` returns `200` only once migrations
   have finished and the database answers a ping, so orchestrators route traffic only to ready instances. Neither
   needs authentication, and neither reveals versions or configuration.
6. **Mail** is optional. The `mail` crate builds the mailer at startup from `MAIL_URL` and `MAIL_FROM` (lettre with
   rustls, no connection pool). SMTP always uses TLS, implicit or STARTTLS. `file://` and `log:` are the development
   transports, and the only places a mail's text is recorded. A failed send isn't retried.
7. **Startup and shutdown:** invalid configuration, an unreachable database or a failed migration stops the server
   with a clear message before it listens. On SIGTERM it stops accepting connections and drains in-flight requests.

## CLI

`crates/cli` is `upheap`, another client of the API. Its conventions (command shape, output, exit codes,
configuration, signing in, layout and tests) are the [CLI rules](engineering-principles.md#cli) of the engineering
principles; the exit codes are in `crates/cli/src/error.rs`, and the commands and flags in `upheap --help`. This
section is how it maps onto this API.

1. **Scope.** The everyday work in a heap: items (add, show with `--history`, edit, the state verbs, list), a
   task's steps (add, tick, drop, edit), an item's questions (ask, request, review, propose-done) and comments (add, list), tags
   (list, add, edit), parameters (list, add, edit, archive, and their options), heaps (list, pick a default) and
   signing in. Teams, members, heap settings and saved-view editing stay in the web app. `parameter edit` sends one
   write per kind of change (name and weight, tags, option order), each at the version the last returned, because
   the API has no single endpoint for them; a failure midway leaves the earlier ones saved and the CLI says which.
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
   sent with `firm: false`, or a `YYYY-MM-DD` date with `firm: true`
   ([decisions.md](decisions.md#26-the-due-picker-offers-rough-choices-the-server-owns)). Every request carries the
   `Time-Zone` header with the machine's zone, so dates and labels read as they would in the browser. The `--due`
   help names the choices from `domain::due`, the rule the server lists them by, since help is read offline; the
   dates and the match still come from the server.
5. **Lists.** `item list` takes the list's query parameters as flags with the same names and values. `--view` loads
   a saved view and sends its settings, with any flag given replacing that setting. The optional text narrows the
   result on the client, as the web app's search does ([Search](#search)). `--tag` takes names, resolved against
   the heap's tags. `item add` reads the list once more to say where the item landed, a read `--quiet` and `--json`
   skip; if it fails, the add still stands and a warning says so. `--step` is sent as one step add per step after the
   create, so a failure leaves the item with the steps sent so far, and the CLI says which: a note on stderr, or
   with `--json` the item as it stands on stdout beside the problem body.
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
7. **`work run` is the agent loop** over the `/work` routes. It claims the next item (`POST /work/runs`), reads the
   heap and the item, and starts the agent tool behind `AgentTools` (`crates/cli/src/agent/`) with a brief on its
   standard input and `UPHEAP_RUN`, `UPHEAP_SERVER` and `UPHEAP_HEAP` in its environment. While the tool runs it
   sends a heartbeat every 5 minutes by the wall clock, checked every 30 seconds, so one goes out soon after the
   machine wakes from sleep (the monotonic clock stands still in sleep). A failed heartbeat is tried again at each
   check until one is heard, logged each time, and said on stderr from the second in a row, so the one that goes out
   before the network is back after a wake isn't a false alarm. The heartbeat says the loop is up. Claude's streamed
   output says claude is working: it is read by the loop only, saved to the run's log
   (`<config dir>/runs/<run id>.jsonl`, pruned after 7 days) and never sent to the server. When the tool shows no
   output for 30 minutes the loop stops it and fails the run with its own message, "The agent showed no activity
   for 30 minutes." That timer uses the monotonic clock on purpose: it stands still while the machine sleeps, as
   claude does, so a sleep isn't silence. The loop stops the tool when the server answers
   `run_over`. When the tool ends it sends `finish` with the cost the tool reported and, only if the tool itself
   failed, why: the server decides whether the run reached its goal, asked, handed back, or failed, and
   the loop never works that out. With `--workers N` the loop runs N workers on one task, each claiming its own item;
   the server's claim keeps them apart, and one Ctrl-C reaches them all. Above 1 worker it needs `--root`, because
   runs in one directory would trample each other. The tool gets what its goal needs and nothing more: to prepare an
   item it may run the `upheap` commands a run uses, read files under the directory it runs in and use the web; to do a task it may also edit files, and every other command follows the
   person's own Claude settings, since Upheap keeps no tool list of its own. With `--root` the loop names one
   directory for each item, `<root>/<team>/<heap>/<key>` from the claimed run; the loop makes it if it is
   missing and the tool writes its own instructions into it (for Claude, `CLAUDE.md`), and starts there. The path is the whole
   record, with no index file to fall out of step. Later runs on the item start in the same directory as it was
   left, so a clone and its caches are paid for once; the agent's session does not carry over. Each start rewrites
   `.upheap-run` in it, naming the server, and that file's modification time is when a run last started or ended there. The
   run holds a shared lock on that file until it ends. The loop sweeps the
   root when it starts and about hourly before a claim, one worker at a time, and removes a directory whose
   marker is 7 days old, or whose item is closed or answers 404 (deleted, moved, or no longer the person's) while
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
   own and before the deny of `upheap auth` that every run gets, so a tool the runner's instructions ask for can be
   allowed for the loop without loosening every session on the machine. With `--instructions` the brief also carries
   the runner's own instructions, between Upheap's and the heap's context: a heap's context is shared by everyone who
   works the heap and says what the work is, and how it is done is for the one who does it, so each
   runner keeps that in a file of its own and it wins over the context on how. The tool runs in its own process
   group, so a Ctrl-C typed in the terminal reaches the loop and not the tool: the first lets the run end and stops
   the loop, the second stops the tool, and the run counts as failed unless it had asked a question. Stopping asks the tool
   to end and kills its group after a few seconds if it hasn't: Claude runs each command in a group of its own and
   ends them only when asked, not when killed. `SIGTERM` does what the second Ctrl-C does; `SIGHUP` is left
   alone, so `nohup` keeps a loop going. The loop reads
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
   `upheap auth login` with a browser that answers the consent page.

The commands are in [README.md](../README.md#develop).
