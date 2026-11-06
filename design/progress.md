# Implementation progress

Tracks what is built against [requirements.md](requirements.md), [offering.md](offering.md) and
[technical-design.md](technical-design.md). Last reviewed 2026-11-06.

About 70% of the golden path is built: local accounts, teams with members and roles, heaps that can be created,
renamed and archived, the ranked list with add, edit, close and reopen, the Edit item view, tags, blockers, the
list view controls (state modes, sort, filters, search) and saved views. Custom parameters, OIDC and CLI sign-in are
not started. All 141 Rust tests pass (`cargo test --workspace`), the store and API suites also pass on MySQL 8.4, and the
web typecheck and lint are clean. CI runs on every pull request.

Status: ✅ done · 🟡 partial · ⬜ not started.

## Progress by area

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts: register, sign in, sign out | Off Auth §2–3 | ✅ | argon2id, hashed session tokens, CSRF header |
| Personal team and heap on registration | Off Teams §3 | ✅ | |
| Items: fields, states, due times, unset values | Req Items §1–4, 6 | ✅ | Loose vs exact due, fuzzy labels, overdue |
| Reopen within one week | Req Items §7 | ✅ | 7×24h, enforced on the server |
| Ranking: age, deadline, value per effort | Req Priority §1–4, 6–10 | 🟡 | Placeholder formula `V1` with blocker inheritance; no custom parameters |
| Main list view | Req Views: List items | ✅ | Per-state pinned/shown/hidden, blocked grouped or mixed in, `sort=`, tag/effort/impact filters, client text search (`/`), inline edits, `N`; the view lives in the URL. Custom parameter sort and filters come with custom parameters |
| Closed items view | Req Views: Closed items | ✅ | Cursor paging, reopen |
| Add item | Req Views: Add item | 🟡 | Tags, created by typing a new name; no linked parameters |
| Edit item view | Req Views: Edit item | 🟡 | Every field, tags, state and blocker in one save; a closed item is reopened and edited in the same save. No parameters yet |
| Tags and Manage tags | Req Items §5, Manage tags | ✅ | Unique per heap ignoring case; never deleted; removable from items in the list; filterable in the list |
| Blockers, inheritance, cycle check | Req Items §8, Priority §5 | ✅ | Picked in the list row of a blocked item; "Blocks …" contributor; runs on MySQL 8.4 in CI |
| Custom parameters and Manage parameters | Req Custom parameters | ⬜ | Only the three fixed heap weights exist |
| Saved views | Req Heaps §4 | ✅ | Shared by the team, as links above the list; save the current view, save a change into the open view, rename, archive. Parameter filters come with custom parameters |
| Teams, members, roles | Req Heaps §2–3, Off Teams | ✅ | Create and rename teams, add members found with `GET /users`, promote, demote, remove, leave; `last_admin` checked under a team row lock. Teams and Team views, Account view for the display name |
| Heap create, rename, archive, unarchive | Off Teams §4, 9 | ✅ | Archiving is checked under the heap lock, so no write lands in a heap archived while it waited. A banner on archived heaps offers Unarchive to admins |
| Audit log: writing entries | Off Audit §1–3 | ✅ | Written in the same transaction as every write |
| Audit log: viewing entries | Off Audit §4 | ⬜ | No read endpoint or UI |
| OIDC sign-in | Off Auth §1 | ⬜ | |
| CLI sign-in: OAuth with PKCE | Off Auth §4 | ⬜ | |
| Sign-in rate limiting | TD Auth §4 | ⬜ | |
| API conventions: `If-Match`, problem+json, scoping | TD API, Access control | ✅ | Outsiders get 404, archived heaps get 409 |
| Deployment: single binary, Docker, health checks | TD Configuration | ✅ | SQLite or MySQL chosen at startup |
| Testing and CI | TD Testing | 🟡 | CI runs the store and API suites on SQLite and MySQL; the access-control table is in place; no Playwright |

## Remaining work

- [x] **Tags**
  - [x] `tags` and `item_tags` tables, name unique per heap ignoring case
  - [x] `GET/POST /heaps/{id}/tags`, `PATCH /tags/{id}`; no delete. Items take `tag_ids` on create and edit.
  - [x] Take a tag off an item from the list (audited as `item_updated`)
  - [x] Tags in Add item; typing a new name creates the tag
  - [x] Manage tags view
- [x] **Blockers**
  - [x] `blocked_by_item_id` column; a blocker from another heap is rejected with 422
  - [x] Cycle detection inside the write transaction (`blocking_cycle`)
  - [x] Blocker inheritance in `rank()` (memoized walk, 1.1 boost)
  - [x] Pick the blocking item, in the list row and in Edit item
- [ ] **Edit item view**
  - [x] Every field including description, tags, state and blocker, saved as one patch (one version, one audit entry)
  - [x] Opened from the list (Edit) and from Closed (the title); a closed item can be reopened and edited in one save
  - [ ] Custom parameters, once they exist
- [x] **List view controls**
  - [x] Per-state pinned / shown / hidden (`open=`, `in_progress=`, `blocked=`); blocked items mixed in or grouped
    (`blocked_layout=`); `hidden_count` says how many the view leaves out
  - [x] Sort by a single parameter (`sort=priority|due|effort|impact`), ties in priority order
  - [x] Server-side filters by tag, effort and impact (comma-separated, any of); ranking still runs over every item
  - [x] Client-side text search over title and description
  - [ ] Sort and filter by custom parameters, once they exist
- [x] **Teams and heaps**
  - [x] Team endpoints: create, rename, members, roles, leave (`last_admin` rule)
  - [x] `GET /users?query=` for admins
  - [x] Heap create, rename, archive, unarchive; heap switcher in the UI
  - [x] `PATCH /me` for display name
  - [ ] Invitations by email or link, and IdP group membership (offering: a later iteration)
- [ ] **Custom parameters**
  - [ ] Parameter, option and tag-link tables
  - [ ] Admin endpoints; archiving with replacement (`replacement_required`, `parameter_in_use`)
  - [ ] Parameters in ranking and contributors
  - [ ] Suggest linked parameters when a tag is added; Manage parameters view
- [x] **Saved views**
  - [x] `saved_views` table with the list view as JSON; archived, never deleted
  - [x] `GET/POST /heaps/{id}/views`, `PATCH /views/{id}`, `POST /views/{id}/archive`; any member, like tags
  - [x] Views bar on the heap page: open a view, save the current list as a view or into the open one, rename, archive
  - [ ] Custom parameter filters, once they exist
- [ ] **Audit log reading**: `GET /heaps/{id}/audit`, `GET /teams/{id}/audit`, filtered by permission
- [ ] **Auth**
  - [ ] Generic OIDC providers, matched by `(provider, subject)`
  - [ ] OAuth authorization code + PKCE, refresh-token rotation, `/me/tokens`
  - [ ] Sign-in rate limiting
- [ ] **Testing and CI**
  - [x] CI pipeline: fmt, clippy, tests, web checks, OpenAPI drift check (`.github/workflows/ci.yml`)
  - [x] Store and API suites on MySQL in CI (`TEST_DATABASE_URL`, a database per test, `store::testing`)
  - [x] Table-driven access-control test (non-member, member, admin, archived heap) in
    `crates/api/tests/access_control.rs`, covering heap- and team-scoped endpoints
  - [ ] Playwright golden paths

## Suggested order

1. Audit log reading, now that team, heap and view changes are audited too.
2. Custom parameters, which also completes the ranking formula and links parameters to tags (postponed for now).
3. OIDC sign-in, then the CLI's OAuth flow.
