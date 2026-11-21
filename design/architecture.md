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
| Auth | argon2id for local passwords; an own session table with hashed random tokens for the web app; own OAuth 2.0 authorization-code endpoints with PKCE for non-web clients, and personal tokens for machines with no browser. One OpenID Connect provider per server, through the `openidconnect` crate, next to local accounts |
| Web app | React, Vite, TanStack Query and Router, Tailwind, React Aria Components, Motion; embedded in the binary with rust-embed |
| Observability | `tracing` with JSON logs, request IDs, `/healthz` and `/readyz` |
| Testing | `cargo test` with googletest matchers, proptest for ranking invariants, MySQL and Postgres service containers in CI, Playwright end to end |
| Packaging | A static binary in a minimal Alpine image; docker-compose with SQLite on a volume; Cloud Run with Neon Postgres in production ([operations.md](operations.md)); the CLI and the server as static binaries on GitHub Releases, with a Homebrew cask for the CLI in `bzzzzzz/homebrew-tap` ([operations.md](operations.md#releasing-the-cli-and-the-server)) |

## Workspace

Dependencies point one way: `server → api → service → store, domain`, with `api → api-types → domain` and
`server → mail` beside it, and `service → mail` for the mail it sends. `domain` depends on nothing in the workspace, so ranking and the state rules are unit- and
property-tested in isolation, and `store` and `api` never depend on each other.
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
- **An item's questions are rows of their own** in `item_questions`, built like steps: each has its own `version`, so
  a question write never bumps the item's and an item write never bumps a question's, and a `number`, one past the
  highest the item has had and never reused, which is its name (`k3x9a.q2`). Questions are never deleted: an answered
  one stays, with its reply (`reply_picked`, indices into `choices`, and `reply_comment`), which the person may change
  while the item is open (`reply_changed` marks it, and `replied_by`/`replied_at` are then the change's), and one withdrawn when its
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
  position and the question number, where the score is multiplied by the caller's heap priority (Across heaps). A question on a reminder to
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
- **An item's turns are rows of their own** in `item_turns`: an item handed to an agent, with the send it belongs to
  (`send_id`, the ID of the send's first turn: a turn a person or the prepare setting opens starts a send, and the
  next stage of an all-the-way send and a split's turn for its task carry it on), a reach (one state, or
  all the way), the stable state it goes to (`goal`, worked out from the item when the turn opens, or picked by the
  person: defined, for a note as written, which all the way opens at defined anyway; then stored), the person's optional line (`MEDIUMTEXT` on MySQL), who opened it and whether the person sent it or the heap's
  prepare setting did, then when and by whom it ended and how (taken back, handed back, or reached), its watch, and how many of its runs in a
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
  The agent state of a listed item splits that: `stuck` for a stuck turn, `waits` for an open turn that isn't
  `with_agent`, `watching` for one that is and has a watch, `running` or `queued` for one that is and hasn't, by whether the
  turn has a live run (read at request time), and `none` without a turn. Rows carry it as `agent_state`. A turn's target (`going_to`) is its goal, except that a turn sent
  all the way is `done`; the turn carries it as `going_to`, for the row's badge. The ranked list
  filters by both, so it reads turns and open questions for every non-closed item, not only the listed ones, before
  it filters (still none in an archived heap). Saved views don't store these filters.
  `heaps.agents_prepare` makes `create_item` open a turn of one state on every new item, whoever added it. Neither
  changes ranking. The agent queue (`work next`, `work queue`, `work claim`, `work run`) reads the heaps it is given and every
  active heap made from an item in one of them, and from an item in those in turn (`resolve_queue_heaps`), so a made
  heap's items are given with the heap that held their note; the limit on how many heaps a request names counts only
  the named ones.
- **A turn is worked in runs,** rows of their own in `item_runs`: one go an agent has at the turn of an item, with who
  claimed it, when it started and was last heard from (`heard_at`), the item's stage then, and, once over, how it
  ended (`reached`, `asked`, `failed`, `taken_back`, `handed_back` or `limited`), what it said it reached and made (`result`, `links`), why it
  failed, and its cost in micro-dollars. `work run` also sends the agent tool's own start and exit times with `finish`
  (`tool_started_at`, `tool_ended_at`, each set once, null for older runs and for runs whose tool never started), and
  the tool's name and model (`agent_tool`, `agent_model`: free text cut to 100 characters, null for older runs, set by
  `finish` and replaced by a later one).
  Like turns, runs are never deleted and never bump the item's `version`.
  A run that ends as `reached` also records what it reached (`reached`: `with_context` or `defined`, the item's stage
  after the reach, or `became` for a note that became items, heaps or a step), null for every other end and for runs
  that ended before it was recorded. The Agents page's two lists are built from runs and turns, not the audit log:
  history is the runs that ended as reached in the last 30 days, and Your decision is the items whose latest turn
  ended as reached by such a run and that are still at that stage, not started and not closed.
  `GET /work/next` and `POST /work/runs` pick the same item: the first of the given heaps' items that are open and not
  blocked, in an active heap, with an open turn that isn't stuck or watching, no open question and no live run, ordered first by
  whether the open turn's latest run ended `limited` (those come before all others, so the item a usage limit cut off
  is the one claimed once the limit resets), then by the item's score in its own heap (`rank_open_items`, once per heap) times the caller's heap priority, times
  `STARTED_BOOST` (3) when a run has already worked on the open turn's send, however it ended, then heap name, then
  position. The boost lets half-done work go on before new work of about the same importance while something much
  more important still passes it; it is the queue's alone, so the person's lists and Waiting for me order by the
  plain score. `GET /work/queue` returns the whole list in that order, read-only like `/work/next`, with each item's
  position in its group of the heap's default list and the heap's priority. It is unpaged: the queue is at most the
  open items of the given heaps, which are few; past a few hundred items it would need a cap. A claim takes the locks
  of every given heap in ID order, as the item writes do, so two claims at once get different items.
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
  a claim finds it (`already_met`) before choosing, so a met turn never costs a run, or when a run's became
  defines a task split from its note (`became`, below); `next` skips such turns without
  writing. Runs and questions end as for a take back, and a turn sent all the way opens the next. `work hand-back`
  ends the run as `handed_back` (its reason is the run's `result`) and the turn likewise, with no follow-on.
  `work watch <pr-url>` ends the run as `watching` (the address is its one link) and leaves the turn open, with a
  **watch** stored on it: `watch_url` and `watch_since`, both null or both set, so a watch lives and dies with its open turn and
  an item has at most one. It is refused while the item has an open question, and a watching turn is out of the queue
  without being waiting for the person. The server never reaches GitHub: the loop lists the open watches
  (`GET /work/watches`), reads each pull request with the person's `gh` about once a minute, and when something is
  newer than `watch_since` (a comment, a review, a failed check, a merge or a close) fires the watch
  (`POST /work/watches/{turn}/fire`), which clears it and adds a comment on the item, written as the person running the
  loop with the comment's usual audit entry.
  **Writes in a run:** a client sends `Upheap-Run: <run ID>`, which `CurrentUser` reads into `Principal::run`. A write made in a run must be on the allowlist in
  `extract.rs` (the item's `PATCH`, steps, questions, comments, creating an item, closing a note as became, and the run's own heartbeat, reached, hand-back, watch and finish) or it is
  `run_forbidden`; the service then checks, under the heap lock, that the run is the caller's, live, with its turn
  open, and on that item (`check_run_write`), refuses any `PATCH` field but the context, `kind: task` and starting,
  and touches `heard_at`. Starting an item in a run keeps its turn.
  **Adding an item in a run** (`check_run_add`) may target any heap the person can edit, in any of their teams. It
  locks the heap of the run's item and the target heap in heap ID order (once when they are the same, as
  `claim_run` and `move_item` do), then reads the run under lock and checks it is live; it refuses a reminder and an
  item with no context. The item gets `origin_id` (the run's item) and `created_run_id`, starts `with_context` and
  opens no Prepare turn. A run may add steps to an item whose `created_run_id` is that run, or that was split from
  the run's item (`check_run_step_add`), and no other write to it. The one write outside those is `POST …/to-step`:
  the run's own note becomes a step of any open task in its heap, which ends the run and the turn as reached. The other
  is `POST …/heaps`: a run makes a heap from its own item, with its context, and goes on (`check_run_make_heap`); it
  never edits a heap afterwards. An item shows its origin only to someone who can view the origin's heap; `added_in_run`
  shows to everyone. **A run splits its note:** an item made with an `origin` (`item add --from`) in a run must name
  the run's own item, with a need no higher than the note's, and may start blocked only by an item a run of the same turn split
  from the note (`made_in_split`, `origin_id`, and `created_run_id` naming a run of that turn). A person's split sets `origin_id` alone. `POST …/became` in a run
  checks that no question is open and every open task split from the note, in the note's heap, has a context and
  a step
  (`split_reached_check`); then, in one transaction, those tasks become defined, the note closes as became, and the
  run and turn end as reached with no follow-on on the note. A task's own open turn that defined meets (the heap's
  prepare turn, or one the person sent) ends as reached there too, as a stage edit ends it (`end_met_turn`, how
  `became`). A heap made from the note counts as made: the same transaction defines the open tasks in it, and the
  run's `became` first checks that each such heap has a context, an open item made from the note, and a context on
  each open note made from it (`made_heaps_check`). A made heap the person archived is ignored. The tasks in a made heap get no turn from the note's. If the note's turn was sent all the way, each of those tasks not started and without an open turn gets
  one in the same transaction (`Turn::passed_on`: the same reach, line, source and opener, to done), but not one
  whose ended turn the person sent: that task stays as they left it; a follow-up a run added isn't split from the note, so it gets none. A later `work reached` on that run succeeds and does nothing. Audit entries of a write in a run carry
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
- **A tag is owned by a heap or by a team:** exactly one of `tags.heap_id` and `tags.team_id` is set, and the store
  checks it on every read. Both kinds share one table, so `item_tags`, parameter links, saved views and tag orders
  name either by ID with one foreign key, and a team tag needs no copy per heap: a rename is one row written. A
  heap's tags are its own and its team's, read together.
- **A parameter is owned by a heap or by a team:** exactly one of `parameters.heap_id` and `parameters.team_id` is
  set, and the store checks it on every read. A heap's use of a team parameter is a row in `heap_parameters` with the
  heap's weight, rather than a weight on the parameter, which would be one number for every heap; stopping sets
  `stopped_at` and using again clears it, so the weight survives. The store reads a heap's parameters as its own plus
  the team parameters it has a use row for, with that weight, and archived when the team archived the parameter or
  the heap stopped using it. Ranking, item values, filters, saved views, moves and the audit's names read that, so
  they see a used team parameter as the heap's own, and one a heap doesn't use isn't there.
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
- **Insert-only tables:** the application can only insert into `audit_log`. There is no update or delete code path
  for it.
- **The heap's ranking policy** is its row: the need, age, deadline, value and tags weights on `heaps`, plus each
  parameter's weight, and the heap's tag order. The weights and the order change together through one endpoint, in
  one transaction under the heap lock. The policy includes the weights of the team parameters the heap uses, which
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
   The score combines them. The formula is a named version (`FormulaVersion::V3`) behind one
   function, set in code rather than stored per heap: tests pin orderings per version, so a formula change is an
   explicit test update, and since scores are never stored, switching versions needs no migration. The module doc of
   `domain::ranking` explains the current formula.
3. **Blocker inheritance:** the blocking edges form a forest, because cycles are refused on write. An item's
   effective score is the greater of its own and the boosted effective scores of the items it blocks, computed by
   one memoized walk, so it's linear in the number of items and carries through chains. Ties go to the item blocking
   the longer chain, so a blocker never ranks below what it blocks, even when every weight is zero.
4. **Contributors** are the largest weighted components, each with a label and a relative share ("Deadline in a few
   days", "Blocks *Migrate auth*"). Scores themselves are never returned.
   **Held back by** names what the person could change to lift an item: a low need, no due date, unset sizes, a tag
   low in the heap's tag order. A
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
   entry. Archiving an option of a team parameter does this in every heap of the team.
10. A display name change is audited once in each of the user's teams, since the audit log is per team and that is
    where teammates see the name. A user in no team has nowhere to record it.
11. **A write that merges heap tags into a team tag** (making, renaming or promoting a team tag into a name heap
    tags of the team have) is refused with `tag_merge_unconfirmed` and the reach: the heap tags it takes in and how
    many items carry each, with a `token`, a hash of that reach (`domain::tag::reach_text`). The client sends the
    write again with `confirm_reach=<token>`. If the reach changed in between, the token no longer matches and the
    write is refused again with the new reach, so what happens is what the person was shown. Nothing is stored
    between the two requests. A merged heap tag's items, parameter links, saved views and tag-order places all move
    to the team tag in that one transaction ([Data model](#data-model), a merged heap tag).

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
   - A bearer token may also be a personal token ([Personal tokens](#personal-tokens)), told apart by its `uph_`
     prefix. A revoked or expired one is refused with the code `token_revoked` or `token_expired`, so the CLI can say why.
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
3. **External identity provider:** one generic OpenID Connect provider per server, set with environment variables
   ([README.md](../README.md#signing-in-through-a-provider)). Discovery, PKCE, the nonce and the ID token's signature
   and claims are security code, so the `openidconnect` crate does them.
   - **Start and callback.** The start (`GET /auth/oidc/start`, a link the browser follows) stores a row in
     `oidc_sign_ins` keyed by the SHA-256 of a random `state`, holding a nonce, a PKCE verifier and the app path to
     return to, and sets the `upheap_oidc` cookie to the `state`, scoped to `/api/v1/auth/oidc`. The callback accepts
     the provider's answer only from the browser that started: the cookie must equal the `state`, and the row must
     exist and be under 15 minutes old. It then exchanges the code with the verifier and checks the ID token's
     signature against the provider's keys, `iss`, `aud`, `exp` and the row's nonce. Every refusal sends the browser
     back to sign-in with a code that the page words, and deletes the row.
   - **Discovery is fetched on every start and callback,** not cached and not at startup: sign-ins are rare, keys
     rotate, and a provider that is down never stops the server or local sign-in.
   - **Matching is by `(issuer, subject)`, never by email.** `user_identities` ties a user to the subject at the
     issuer string as configured. The subject is the claim `UPHEAP_OIDC_SUBJECT_CLAIM` names (`sub` by default). An
     identity still keyed on the token's `sub` is re-keyed to the claim at sign-in, in the known person's transaction.
     A known identity gets a normal session; name and email aren't refreshed from the provider later.
   - **A new identity needs a verified email that no account has,** read from the ID token's claims; an email that
     matches a local account is refused rather than linked ([decisions.md](decisions.md) §38). The callback keeps the
     verified name and email on the row for 15 more minutes and sends the browser to the username step, which makes
     the user, personal team, `personal` heap, identity and session in one transaction, also when registration is
     closed. A failure rolls it all back and leaves the row, so the person can try another username.
   - **Teams from groups.** The operator maps team slugs to provider groups with `UPHEAP_OIDC_GROUP_TEAMS`. The map is
     read from configuration and resolved to team IDs once at startup (a slug no team has, or a personal team, stops
     the server); it is in the settings, never stored on a team, so a team is group-managed while the map names it
     and unmapping it frees it again. Only when the map is not empty does the callback read the groups: from the
     payload of the ID token after its signature was checked, under the claim `UPHEAP_OIDC_GROUPS_CLAIM`. A list of
     strings, or one string, is known, and an empty list is known and empty; a missing claim, any other type and
     Entra ID's overage (the claim named in `_claim_names` and absent) are unknown. Unknown logs one warning and
     changes nothing; it is not the same as empty, which takes the person out of every mapped team.
     The sync runs in the transaction that opens the session. For each mapped team in ID order, so two sign-ins
     never wait on each other in a cycle, it locks the team, reads its memberships, and adds the person as a member
     when their groups name the team and they aren't in it, or deletes the membership, whatever the role, when they
     don't and they are. The last-admin rule doesn't apply: the provider has already said the person is out, and
     the operator makes an admin with a command. Each change has an audit entry (`group_member_added`,
     `group_member_removed`) with the person as actor and the groups in the diff. A `group_member_removed` is not a
     `member_removed`, so it doesn't void invite links made before it. Lock order where these meet: the mapped teams in
     ID order, then a SCIM row, then on Postgres the subject's advisory lock (below), then a person's personal team;
     a write that locks several of a person's teams (a display-name change) takes the mapped ones first. A sign-in of
     a known person, by the provider or by password,
     locks their personal team before it opens the session, and approving a CLI sign-in and exchanging its code lock
     it before they store or use the code, so each waits for a SCIM write that is ending their sign-ins and then
     sees the deactivation. A first sign-in has no user yet, so the callback keeps the groups on
     the `oidc_sign_ins` row (a JSON array; `NULL` is unknown) and the username step runs the sync after making the
     user. A SCIM write can't find that user until it commits, so the username step locks the SCIM rows with the
     person's subject before it makes the user: it and a SCIM write to one of them run one after the other, and the
     session is refused after a deactivation or ended by one that comes after it. The re-keying takes the same subject
     locks as the username step, in the same order; when the person is already deactivated it still commits, and ends
     their sign-ins as a push would. A record being created with the subject, or moved onto it, has no such row yet. On MySQL the lock's gap lock makes that write wait, and the SCIM
     user writes run at READ COMMITTED so that after the wait they see the account; on Postgres both sides also take
     an advisory lock on the subject. Password sign-in, invite registration and reset never sync.
   - **Provider only.** `UPHEAP_PASSWORD_SIGN_IN=false` (it needs the provider, or the server stops at startup)
     makes every password path in the service layer fail with `PasswordSignInOff`, so the API and the operator
     command both refuse: sign-in, registration, invite registration, asking for and using a reset link, and the
     operator's reset link. A local user is one with no `user_identities` row. Two things end their sign-ins, and
     both are needed. The server's start deletes their sessions and revokes their OAuth families and personal
     tokens in one transaction, so turning passwords back on doesn't revive them. The auth check of sessions, access
     tokens and personal tokens also refuses a local user's, one indexed lookup per request while passwords are off,
     because during a rolling deploy an instance that still has passwords on can make a sign-in after the new one
     started. Passwords are never deleted, so setting the variable back to `true` is the recovery.

   - **Signing out at the provider** (`UPHEAP_OIDC_PROVIDER_LOGOUT`, RP-initiated logout). Only when it is on, the
     callback keeps the raw ID token on the session row (`sessions.id_token`); a first sign-in keeps it on the
     `oidc_sign_ins` row until the username step copies it to the session, and the row's deletion removes it. With it
     off nothing is stored, and the setting is checked again at sign-out. Sign-out deletes the session row and clears
     the cookie first, then, if the row had a token, fetches discovery and, when it names `end_session_endpoint`,
     answers `200` with that URL carrying `id_token_hint`, `client_id` and `post_logout_redirect_uri`
     (`{BASE_URL}/login`); the web app goes there (every web sign-out does, the invite and consent pages too: they keep
     the page to return to in `sessionStorage` for ten minutes, and the sign-in page reloads that page, as the plain
     `204` does). Anything else, including a provider that is down, is the plain `204`: sign-out never fails because of
     the provider. There is no `state`, since nothing is kept to check it. The token is stored as it is, not hashed,
     because it has to be sent back; it can't sign anyone in to Upheap, which only accepts a code exchanged with the
     stored verifier and nonce. The CLI's sign-out only revokes its OAuth tokens.
   - **No password:** a provider user has no password row, so password sign-in can't succeed for them and Change
     password isn't offered (`change_password` is left out of `/me`'s allowed actions).
   - An invite page passes `redirect=/invite/<token>` through the round trip, so the person comes back to the invite
     page signed in and joins with its own Join step; the provider flow has no invite logic.
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
   - With mail set up, a user asks for a link themselves: `POST /password-resets` with an email address (and
     `GET /password-resets` tells the web app whether the server does). A mailed link is the same kind, made in the
     same transaction (`made_by: mail`), so each cancels the other. The request is counted per address in
     `sign_in_attempts` under a `reset:` key of its own, before the account is looked up, so asking never uses up the
     address's sign-in attempts and the reverse; the count is never cleared and runs out with its window. A known and
     an unknown address get the same `202`, and the mail is sent after commit in a spawned task, so the answer
     doesn't wait for the provider. A send that fails is logged once and not retried; in-flight sends aren't drained
     at shutdown, and the user asks again. Without mail the endpoint is `404`.
   - With mail set up and a local password, a user changes their **email address** from Account
     (`POST /me/email-changes`, the session cookie only, as a password change is: it is the person's decision in their
     browser). It takes the current password and the new address; the address changes only when a link mailed to the
     new one is applied. Tries are counted per session in the password change's count with its limit, and unlike a
     password change a success doesn't clear it, so it also limits the mails a session sends (five asks per 15
     minutes); a wrong password adds one to the account's sign-in count. An address another account has gets the same
     `202` and the same history entry as a free one, with no link and no mail, so the answer reveals nothing about
     accounts. A free one is a row in `email_change_links` (a token's SHA-256, the user, the new address, an expiry a
     day on), made in one transaction with the cancelling of the user's earlier working one. The table is apart from
     `password_reset_links` because a link carries the address it changes to and has its own lifetime; the state and
     the token are the reset link's (`domain::email_change`). Applying is `POST /email-changes/{token}`, no sign-in,
     and there is no read of a link: mail scanners and previews fetch every link in a mail, so the link opens a page
     with a button, and only the press, a `POST`, changes anything. Applying uses the link conditionally, sets the
     address and bumps the user's version, and cancels the user's working reset link, since one mailed to the old
     address must not keep working (a password change cancels it too, above), all in one transaction with its audit
     entry; a unique-index refusal, the address taken since the ask, is `409 email_taken` and rolls the use back, so
     the link keeps working. Sessions and tokens stay, and it signs no one in. After commit a notice goes to the old
     address, in a spawned task like the other mails. Sign-in counts are keyed by address, so the new address starts
     fresh. A password change or reset doesn't cancel a working email change link.
   - With mail set up, a new password account **confirms its address** before it can sign in. `users.unconfirmed_since`
     (millis) is set when a registration, plain or through an invite link, is made while the server has mail set up,
     and `NULL` means confirmed, so every account that existed before, every account made while mail was off, and
     every provider or SCIM account counts as confirmed with no backfill. Such a registration makes the account, its
     personal team and heap (and the invite's membership) with no session and a confirmation link in the same
     transaction, mails the link after commit like the others, and answers `202` with the address. The link is a row
     in `email_confirmation_links` (a token's SHA-256, the user, an expiry a day on), apart from the other two link
     tables as each purpose has its own, and one is working per user. Its state is the reset link's
     (`domain::password_reset::ResetLink`) and a link that doesn't work fails as an email change link does
     (`domain::email_change`). Sign-in checks the address only after the password verified, and only while mail is
     set up: an unconfirmed account gets a new link (cancelling the earlier one) and `403 email_not_confirmed`, so a
     wrong password reveals nothing and the attempt, still counted, caps the mails at the sign-in limit; with mail
     off an account left unconfirmed signs in, and turning mail back on refuses it again. There is no resend
     endpoint, since signing in again is how a person asks. Confirming is `POST /email-confirmations/{token}`, no
     sign-in and no read of a link (scanners again): it uses the link conditionally, clears `unconfirmed_since`,
     records `email_confirmed` (`made_by: link`) unless the address already was (a reset that confirms writes no entry of its own, its entry stands for it) and opens a session, in one transaction, with or without mail, then clears the address's sign-in count. Any
     mailed link confirms: setting a password with a reset link, the operator's included, and applying an email
     change clear `unconfirmed_since` too, so someone whose address a squatter used gets in through Forgot your
     password?, which mails any account with the address. Sessions an unconfirmed account already holds stay. The
     operator confirms by hand with `upheap-server user confirm <username or email>`, which needs no mail and records
     `email_confirmed` (`made_by: operator`) unless the address already was.
     A mistyped address is corrected with `POST /auth/confirmation-address` (login, password and the new address, no
     session): it counts and verifies like a sign-in, under the same limit (keyed by the address, so a changed address
     starts a fresh count), and only works for an account that is
     still unconfirmed (a confirmed one fails like a wrong password). In one transaction it sets the address (still
     unconfirmed, since nothing proved it), cancels the working reset link, so whoever reads the wrong mailbox can't
     take the account with one, makes a new confirmation link (cancelling the earlier one) and records
     `email_changed`; the link is mailed after commit. An address another account has answers the same `202` and
     changes, mails and records nothing.
   - A user's **account history** is the audit entries about them in their personal team, where their account entries
     go: a password change, a reset link made for them (actor: the user, for the reason above), a password set with
     one, an email change asked and one made, an address confirmed, the sign-in provider turning them off or back on (no account acts, so the actor is the user, as with a
     reset link made for them), and their own heap priority change (recorded here and not in the heap's team, since
     the value is private to them). `GET /me/history` reads exactly those nine actions for the caller, with no way to name another user or
     team, so only the user sees them and team admins never do. It is not the team log: the server words each entry
     as one sentence with no actor ("Password changed. 2 other sign-ins ended.", "You changed your priority for Work
     from 1 to 3.") from the entry's diff, and the sentence is made on reading, so a wording fix reaches old entries.
     A priority entry's diff carries the heap's name at the time of the change; an older entry without one reads the
     heap's current name by its ID, and "a heap" when that is gone. Cookie or bearer token may read it.
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
   [README.md](../README.md#configuration). No session secret is needed, because session tokens are random and
   stored hashed rather than signed. The operator's contact line goes out with the public password-reset options
   (`GET /password-resets`), because the sign-in page already reads them before anyone signs in.
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
   sent with `firm: false`, or a `YYYY-MM-DD` date with `firm: true`
   ([decisions.md](decisions.md#26-the-due-picker-offers-rough-choices-the-server-owns)). Every request carries the
   `Time-Zone` header with the machine's zone, so dates and labels read as they would in the browser. The `--due`
   help names the choices from `domain::due`, the rule the server lists them by, since help is read offline; the
   dates and the match still come from the server.
5. **Lists.** `item list` takes the list's query parameters as flags with the same names and values. `--view` loads
   a saved view and sends its settings, with any flag given replacing that setting. The optional text narrows the
   result on the client, as the web app's search does ([Search](#search)). `--tag` takes names, resolved against
   the heap's tags, its own and its team's: in a heap a name is one tag, so a name needs no kind. With
   `--create-tags`, the names the heap doesn't have are added as heap tags first, right before the item is written,
   and they stay if that write fails. `item add` reads the list once more to say where the item landed, a read
   `--quiet` and `--json` skip; if it fails, the add still stands and a warning says so. `--step` is sent as one step
   add per step after the create, so a failure leaves the item with the steps sent so far, and the CLI says which: a
   note on stderr, or with `--json` the item as it stands on stdout beside the problem body.
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
   says whether the token was revoked, has expired or isn't known.
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

The commands are in [README.md](../README.md#develop).
