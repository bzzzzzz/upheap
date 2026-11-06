# Implementation progress

Tracks what is built against [requirements.md](requirements.md), [offering.md](offering.md) and
[technical-design.md](technical-design.md). Last reviewed 2026-11-05.

About 50% of the golden path is built: local accounts, the personal heap, the ranked list with add, edit, close
and reopen, tags, and blockers. Custom parameters, teams, saved views, search, OIDC and CLI sign-in are not
started. All 72 Rust tests pass (`cargo test --workspace`), and the web typecheck and lint are clean. CI runs on every
pull request.

Status: ✅ done · 🟡 partial · ⬜ not started.

## Progress by area

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts: register, sign in, sign out | Off Auth §2–3 | ✅ | argon2id, hashed session tokens, CSRF header |
| Personal team and heap on registration | Off Teams §3 | ✅ | The only team and heap a user can reach today |
| Items: fields, states, due times, unset values | Req Items §1–4, 6 | ✅ | Loose vs exact due, fuzzy labels, overdue |
| Reopen within one week | Req Items §7 | ✅ | 7×24h, enforced on the server |
| Ranking: age, deadline, value per effort | Req Priority §1–4, 6–10 | 🟡 | Placeholder formula `V1` with blocker inheritance; no custom parameters |
| Main list view | Req Views: List items | 🟡 | Pinned and blocked groups, inline edits, `N` shortcut; no state config, sort, search or filters |
| Closed items view | Req Views: Closed items | ✅ | Cursor paging, reopen |
| Add item | Req Views: Add item | 🟡 | Tags, created by typing a new name; no linked parameters |
| Edit item view | Req Views: Edit item | ⬜ | Description can't be edited and tags can't be added in the UI after creation; the API allows both |
| Tags and Manage tags | Req Items §5, Manage tags | ✅ | Unique per heap ignoring case; never deleted; removable from items in the list; no tag filter yet (List view controls) |
| Blockers, inheritance, cycle check | Req Items §8, Priority §5 | ✅ | Picked in the list row of a blocked item; "Blocks …" contributor; checked by hand on MySQL 8.4 |
| Custom parameters and Manage parameters | Req Custom parameters | ⬜ | Only the three fixed heap weights exist |
| Saved views | Req Heaps §4 | ⬜ | |
| Teams, members, roles | Req Heaps §2–3, Off Teams | ⬜ | `Permission` enum exists, but no use case checks the admin-only permissions |
| Heap create, archive, unarchive | Off Teams §4, 9 | ⬜ | The `heap_archived` write guard exists |
| Audit log: writing entries | Off Audit §1–3 | ✅ | Written in the same transaction as every write |
| Audit log: viewing entries | Off Audit §4 | ⬜ | No read endpoint or UI |
| OIDC sign-in | Off Auth §1 | ⬜ | |
| CLI sign-in: OAuth with PKCE | Off Auth §4 | ⬜ | |
| Sign-in rate limiting | TD Auth §4 | ⬜ | |
| API conventions: `If-Match`, problem+json, scoping | TD API, Access control | ✅ | Outsiders get 404, archived heaps get 409 |
| Deployment: single binary, Docker, health checks | TD Configuration | ✅ | SQLite or MySQL chosen at startup |
| Testing and CI | TD Testing | 🟡 | CI and the access-control table are in place; no MySQL run or Playwright |

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
  - [x] Pick the blocking item (in the list row for now; moves to Edit item when that view exists)
- [ ] **Edit item view**: every field including description, tags, parameters and state
- [ ] **List view controls**
  - [ ] Per-state pinned / shown / hidden; blocked items mixed in or grouped
  - [ ] Sort by a single parameter (`sort=`)
  - [ ] Server-side filters by state, tag and parameter value
  - [ ] Client-side text search over title and description
- [ ] **Teams and heaps**
  - [ ] Team endpoints: create, members, roles, leave (`last_admin` rule)
  - [ ] `GET /users?query=` for admins
  - [ ] Heap create, archive, unarchive; heap switcher in the UI
  - [ ] `PATCH /me` for display name
- [ ] **Custom parameters**
  - [ ] Parameter, option and tag-link tables
  - [ ] Admin endpoints; archiving with replacement (`replacement_required`, `parameter_in_use`)
  - [ ] Parameters in ranking and contributors
  - [ ] Suggest linked parameters when a tag is added; Manage parameters view
- [ ] **Saved views**: `saved_views` table, endpoints, UI
- [ ] **Audit log reading**: `GET /heaps/{id}/audit`, `GET /teams/{id}/audit`, filtered by permission
- [ ] **Auth**
  - [ ] Generic OIDC providers, matched by `(provider, subject)`
  - [ ] OAuth authorization code + PKCE, refresh-token rotation, `/me/tokens`
  - [ ] Sign-in rate limiting
- [ ] **Testing and CI**
  - [x] CI pipeline: fmt, clippy, tests, web checks, OpenAPI drift check (`.github/workflows/ci.yml`)
  - [ ] Store and service suite on MySQL via testcontainers
  - [x] Table-driven access-control test (non-member, member, admin, archived heap) in
    `crates/api/tests/access_control.rs`. Members and archiving are set up directly in the database until
    their endpoints exist.
  - [ ] Playwright golden paths

## Suggested order

1. The MySQL run in CI. A manual run found that the tags migration failed on MySQL, so this shouldn't wait.
2. Edit item view and list controls (sort, filters including tags, search).
3. Teams and heaps management.
4. Custom parameters, which also completes the ranking formula and links parameters to tags.
