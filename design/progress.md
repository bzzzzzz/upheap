# Implementation progress

Tracks what is built against [requirements.md](requirements.md), [offering.md](offering.md) and
[technical-design.md](technical-design.md). Last reviewed 2026-11-08.

About 70% of the golden path is built: local accounts, teams with members and roles, heaps that can be created,
renamed and archived, the ranked list with add, edit, close and reopen, the item pane, tags, blockers, the
list view controls (state modes, sort, filters, search), saved views, custom parameters, the audit log and sign-in
rate limiting, readable handles in every URL, and need with ranking formula `V2`. OIDC and CLI sign-in are not started. All 263 Rust tests pass
(`cargo test --workspace`), the store and API suites also pass on MySQL 8.4 and Postgres 17, the 46 Playwright tests run at each of
three widths (a few skip at widths they don't apply to), and 43 against the local build (those that need accounts or
request routing skip there), and the web typecheck and lint are clean. CI
runs all of them on every pull request.

Status: ✅ done · 🟡 partial · ⬜ not started.

## Progress by area

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts: register, sign in, sign out | Off Auth §2–3 | ✅ | argon2id, hashed session tokens, CSRF header |
| Personal team and heap on registration | Off Teams §3 | ✅ | Named by the username; private to its owner, so it takes no members |
| Readable IDs: usernames, slugs, item keys, short links | [readable-ids.md](readable-ids.md) | ✅ | `/teams/{team}/heaps/{heap}/items/{item}` in the app and the API, `/h/…` short links with Copy link, slugs editable on team and heap pages; a migration fills them in on existing data |
| Items: fields, states, due times, unset values | Req Items §1–4, 6 | ✅ | Loose vs exact due, fuzzy labels, overdue; need (idea, nice to have, expected, critical) required on every new item, changeable but never cleared |
| Reopen within one week | Req Items §7 | ✅ | 7×24h, enforced on the server |
| Ranking: need, age, deadline, value per effort, custom parameters | Req Priority §1–10 | 🟡 | Formula `V2`: need as a factor and as the rate items age at, with blocker inheritance and custom parameters; the heap's own weights can't be changed yet |
| Main list view | Req Views: List items | ✅ | Per-state pinned/shown/hidden, blocked grouped or mixed in, `sort=`, tag/effort/impact/option filters and sort by a parameter, client text search (`/`), inline edits, `N`; the view lives in the URL |
| Closed items view | Req Views: Closed items | ✅ | Cursor paging, reopen |
| Add item | Req Views: Add item | ✅ | A dialog (`N`) with every field: rough due choices or an exact date, S M L XL buttons, tags created by typing a new name, and the parameters linked to the chosen tags; Add another for several in a row |
| Edit item view | Req Views: Edit item | ✅ | The item pane over the list (beside it on a wide screen, a panel on a tablet, a sheet on a phone): every field, tags, parameter values, state and blocker, each change saved as it's made; a closed item is reopened from its state |
| Tags and Manage tags | Req Items §5, Manage tags | ✅ | Unique per heap ignoring case; never deleted; removable from items in the list; filterable in the list |
| Blockers, inheritance, cycle check | Req Items §8, Priority §5 | ✅ | Picked in the list row of a blocked item; "Blocks …" contributor; runs on MySQL 8.4 in CI |
| Custom parameters and Manage parameters | Req Custom parameters, Manage parameters | ✅ | Admins add, rename and weigh parameters, add, relabel, reorder and archive options (open items switch to the replacement), link tags, archive unused parameters. Values on items, in ranking and contributors |
| Saved views | Req Heaps §4 | ✅ | Shared by the team, as links above the list; save the current view, save a change into the open view, rename, archive; parameter filters and sort included |
| Teams, members, roles | Req Heaps §2–3, Off Teams | ✅ | Create and rename teams, add members found with `GET /users`, promote, demote, remove, leave; `last_admin` checked under a team row lock. Teams and Team views, Account view for the display name |
| Heap create, rename, archive, unarchive | Off Teams §4, 9 | ✅ | Archiving is checked under the heap lock, so no write lands in a heap archived while it waited. A banner on archived heaps offers Unarchive to admins |
| Audit log: writing entries | Off Audit §1–3 | ✅ | Written in the same transaction as every write |
| Audit log: viewing entries | Off Audit §4 | ✅ | Heap and team logs, filtered by permission and paged; one entity's history in the item pane and on tags, parameters, saved views, heaps and members; Activity pages |
| OIDC sign-in | Off Auth §1 | ⬜ | |
| CLI sign-in: OAuth with PKCE | Off Auth §4 | ⬜ | |
| CLI: items, states, list, tags, heaps | [cli.md](cli.md) | ⬜ | Designed; needs CLI sign-in and the DTOs moved to `api-types` first |
| Sign-in rate limiting | TD Auth §4 | ✅ | Five attempts per email address per 15 minutes, counted with one upsert so concurrent attempts can't slip past; addresses compared exactly on MySQL too; `429` `too_many_attempts` with `Retry-After`. No per-IP limit |
| API conventions: `If-Match`, problem+json, scoping | TD API, Access control | ✅ | Outsiders get 404, archived heaps get 409 |
| Deployment: single binary, Docker, health checks | TD Configuration | ✅ | SQLite, MySQL or Postgres chosen at startup |
| Postgres support, Cloud Run with Neon | [cloud-run-postgres.md](cloud-run-postgres.md) | 🟡 | Postgres supported and in CI; the Neon and Cloud Run setup is next. Tracked in its own doc |
| Local build: static, single user, data in the browser | [local-build.md](local-build.md) | ✅ | The backend and the Router run in a browser worker on SQLite-wasm in OPFS, without accounts. The web app offers only what one user can do, keeps its data with export and import and one tab at a time, and CI runs the golden paths against the static build. The Pages workflow publishes it from `main` after every merge, to https://bzzzzzz.github.io/upheap/ |
| Web app UX for phones and wide screens | [app-ux.md](app-ux.md) | 🟡 | Step 1 done: design tokens, the app shell at each width (sidebar, rail, phone tabs), phone and tablet test projects. Step 2 done: the Add dialog with the due choices from the server (`GET /due-choices`) and S M L XL buttons; compact rows with the state circle and an undo toast; the item pane in place of the Edit item page; keyboard (arrows, Enter, `x`, Escape). Step 3 done: sheets sit above the on-screen keyboard; rows swipe right to mark done and left for In progress or Blocked; the item sheet drags down and the tablet panel right to close; Filters and View are a popover on a tablet or wide screen and a sheet on a phone, as are the heap switcher and More; saved views scroll sideways on a phone. First-set build ([ux-tracker.md](ux-tracker.md)) step 1 done: hover Start and ⋯ on rows, "Start next" in the toast, filter chips with "6 of 14", the changed view's dot with Save and Undo, "Show them" for hidden items; in the item, the likeliest next state as the main button, ↑ ↓, the key as the copy-link button, contributor bars and "Added by Sam 4 days ago" (`created_by` and `created_label` on items; the ranked list also carries `unfiltered_count` for "6 of 14" and `in_progress_count` for "Start next"). Step 2 (phone tabs, item bar and sheets) done: the phone's tabs are Open, Closed, +, Activity, More, with + opening Add; More and the heap switcher are sheets of 52 px rows, the switcher with "New heap in <team>"; the item sheet has its state buttons in a bar at the bottom and due, tags, parameters and description as rows that open sheets, the due sheet with Exact date and Clear due. Step 3 (Add) done: the heap chooser in the title, prefill from the list's filters, "Added “X” at #4 in Up next" with Open it (in the dialog with Add another, in the toast without), Add another remembered per device, due starting at None, and on a phone a sheet of chips with More fields. Step 4 (Closed) done: sections Today, This week and Earlier, an intro line on the week limit, "Done · today by you", and Reopen while allowed, 44 px on a phone (`closed_by` and `closed_group` on items, and a nullable `items.closed_by`). Step 5 (Your data, local build) done: the Account page's Your data card, a replace confirm (a modal, a sheet on a phone) that names the file and compares what the copy holds with what's here, with Download current first, and a storage note that mentions the last download when it's over two weeks old or never happened (the worker's `count` message; the last download in localStorage). The rest of the Account page, a Profile card with the display name and email, uses the tokens too. Step 6 (Sign in and create account) done: a card on a wide screen and the bare form on a phone, in the tokens, with a Sign in / Create account switch (tabs); a problem about an input shows under it, with focus on it, and any other above the button; a signed-out visit to a page goes to `/login?redirect=…`, says where the link leads ("Sign in to open the link to 3n2ay.") and opens it after signing in, ignoring a `redirect` that isn't a page of the app; an expired session returns the same way (step 8); two API changes: `email_taken` now names `field` `email`, and sign-in answers an empty email or password with `422` `field_empty` before counting the attempt (it used to be `401` `invalid_credentials` for an empty email); signing in or creating an account drops the query cache, so nothing of a previous user's shows. Step 7 (Team and Teams) done: Teams with + Create team opening a form (Name, optional Link name, the server's errors under their field) and rows saying "Admin · 4 members · 2 heaps"; the Team page in the settings layout, with the rarer team actions under a ⋯ by the name, heap rows with "14 open", Open and a ⋯ (Rename, Link name, Archive, History), a member search whose results are a listbox under it (arrows, Enter adds), member rows with an initials avatar, role badge, Make admin or Make member and Remove, a remove confirm under the list that names the heaps they lose, and on a phone 52 px heap rows that open the heap and member actions in a sheet; one API change: `open_count` on `Heap` and on each heap of `GET /me` (as `HeapOverview`), and `member_count` on `GET /me`'s teams. Next: every other page at phone width. Installing (PWA) comes later |
| Testing and CI | TD Testing | ✅ | CI runs the store and API suites on SQLite, MySQL and Postgres, and Playwright golden paths in Chromium; the access-control table is in place |

## Remaining work

- [x] **Tags**
  - [x] `tags` and `item_tags` tables, name unique per heap ignoring case
  - [x] `GET/POST …/heaps/{heap}/tags`, `PATCH …/tags/{tag_id}`; no delete. Items take `tag_ids` on create and edit.
  - [x] Take a tag off an item from the list (audited as `item_updated`)
  - [x] Tags in Add item; typing a new name creates the tag
  - [x] Manage tags view
- [x] **Blockers**
  - [x] `blocked_by_item_id` column; a blocker from another heap is rejected with 422
  - [x] Cycle detection inside the write transaction (`blocking_cycle`)
  - [x] Blocker inheritance in `rank()` (memoized walk, 1.1 boost)
  - [x] Pick the blocking item, in the item pane
- [x] **Edit item view**
  - [x] Every field including description, tags, state and blocker, each saved as it's made with its own `If-Match`
  - [x] Opened from a list row and from Closed (the title), over the list; Back closes it; a closed item is reopened from its state
  - [x] Custom parameter values
- [x] **List view controls**
  - [x] Per-state pinned / shown / hidden (`open=`, `in_progress=`, `blocked=`); blocked items mixed in or grouped
    (`blocked_layout=`); `hidden_count` says how many the view leaves out, `unfiltered_count` how many it would
    list without its filters, and `in_progress_count` how many of the heap's items are in progress
  - [x] Sort by a single parameter (`sort=priority|due|effort|impact`), ties in priority order
  - [x] Server-side filters by tag, effort and impact (comma-separated, any of); ranking still runs over every item
  - [x] Client-side text search over title and description
  - [x] Sort by a custom parameter (`sort=parameter&sort_parameter=`) and filter by options (`param=`)
- [x] **Teams and heaps**
  - [x] Team endpoints: create, rename, members, roles, leave (`last_admin` rule)
  - [x] `GET /users?query=` for admins
  - [x] Heap create, rename, archive, unarchive; heap switcher in the UI
  - [x] `PATCH /me` for display name
  - [ ] Invitations by email or link, and IdP group membership (offering: a later iteration)
- [x] **Custom parameters**
  - [x] Parameter, option, tag-link and item value tables
  - [x] Admin endpoints; archiving with replacement (`replacement_required`, `parameter_in_use`)
  - [x] Parameters in ranking and contributors
  - [x] Suggest linked parameters when a tag is added; Manage parameters view
  - [ ] Edit the heap's own weights (need, age, deadline, value)
- [ ] **Need**
  - [x] `items.need`, required on create, never cleared; existing items migrated to `expected`
  - [x] Need in ranking (`V2`): its own factor and the rate items age at; "Need: Critical" contributor
  - [x] Need picker in the Add dialog (required) and the item pane; audited
  - [ ] Filter and sort the list by need, like effort and impact
  - [ ] Show the need in the list row, so a critical item stands out without opening it
  - Making `need` required on `POST …/items` broke API §3 (breaking changes are additive or versioned) on purpose:
    defaulting it would guess a value the user didn't pick, and there are no outside clients yet. Release notes
    call it out.
- [x] **Saved views**
  - [x] `saved_views` table with the list view as JSON; archived, never deleted
  - [x] `GET/POST …/heaps/{heap}/views`, `PATCH …/views/{view_id}`, `POST …/views/{view_id}/archive`; any member, like tags
  - [x] Views bar on the heap page: open a view, save the current list as a view or into the open one, rename, archive
  - [x] Custom parameter filters and sort
- [x] **Audit log reading**
  - [x] `GET /teams/{team}/heaps/{heap}/audit`, `GET /teams/{team}/audit`, filtered by permission, with `entity_id=` and cursor paging
  - [x] Changes described by the server, with current names for IDs
  - [x] History in the item pane and on tags, parameters, saved views, heaps and members; heap and team Activity pages
- [ ] **Auth**
  - [ ] Generic OIDC providers, matched by `(provider, subject)`
  - [ ] OAuth authorization code + PKCE, refresh-token rotation, `/me/tokens`
  - [x] Sign-in rate limiting
- [x] **Local build** ([local-build.md](local-build.md))
  - [x] Spike: `domain`, `store`, `service` and `api` on `wasm32-unknown-unknown`, SeaORM proxy over SQLite-wasm
  - [x] `accounts` feature in `service` and `api`, the local user, `trace-and-timeout` in `api`; CI builds and lints
    the wasm, runs it in Node and checks its size
  - [x] `crates/local`, the worker and the web app's worker `fetch`
  - [x] Single-user mode: `allowed_actions` on `GET /me` and the team without the actions it lacks, and the UI
    that hides them
  - [x] Persistent storage, export and import, one tab at a time
  - [x] GitHub Pages workflow; Playwright golden paths against the static build
- [x] **Testing and CI**
  - [x] CI pipeline: fmt, clippy, tests, web checks, OpenAPI drift check (`.github/workflows/ci.yml`)
  - [x] Store and API suites on MySQL and Postgres in CI (`TEST_DATABASE_URL`, a database per test, `store::testing`)
  - [x] Table-driven access-control test (non-member, member, admin, archived heap) in
    `crates/api/tests/access_control.rs`, covering heap- and team-scoped endpoints
  - [x] Playwright golden paths in `web/e2e` (sign-up and sign-in, add and rank, close and reopen, block and
    unblock, parameter management), against the real binary on in-memory SQLite

## Suggested order

1. Editing the heap's own weights, OIDC sign-in, the CLI's OAuth flow, and invitations.
2. The web app at phone and wide-screen widths ([app-ux.md](app-ux.md)), starting with the app shell.
