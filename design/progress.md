# Implementation progress

Tracks what is built against [requirements.md](requirements.md), [offering.md](offering.md) and
[technical-design.md](technical-design.md). Last reviewed 2026-11-05.

About 40% of the golden path is built: local accounts, the personal heap, and the ranked list with add, edit,
close and reopen. Tags, blockers, custom parameters, teams, saved views, search, OIDC and CLI sign-in are not
started. All 35 Rust tests pass (`cargo test --workspace`), and the web typecheck and lint are clean. There is no
CI yet.

Status: ✅ done · 🟡 partial · ⬜ not started.

## Progress by area

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts: register, sign in, sign out | Off Auth §2–3 | ✅ | argon2id, hashed session tokens, CSRF header |
| Personal team and heap on registration | Off Teams §3 | ✅ | The only team and heap a user can reach today |
| Items: fields, states, due times, unset values | Req Items §1–4, 6 | ✅ | Loose vs exact due, fuzzy labels, overdue |
| Reopen within one week | Req Items §7 | ✅ | 7×24h, enforced on the server |
| Ranking: age, deadline, value per effort | Req Priority §1–4, 6–10 | 🟡 | Placeholder formula `V1`; no custom parameters or blocker boost |
| Main list view | Req Views: List items | 🟡 | Pinned and blocked groups, inline edits, `N` shortcut; no state config, sort, search or filters |
| Closed items view | Req Views: Closed items | ✅ | Cursor paging, reopen |
| Add item | Req Views: Add item | 🟡 | No tags or linked parameters |
| Edit item view | Req Views: Edit item | ⬜ | Description can't be edited after creation |
| Tags and Manage tags | Req Items §5, Manage tags | ⬜ | No tables or endpoints |
| Blockers, inheritance, cycle check | Req Items §8, Priority §5 | ⬜ | The blocked state exists without `blocked_by_item_id` |
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
| Testing and CI | TD Testing | 🟡 | Unit and API tests exist; no CI, MySQL run, Playwright or access-control table |

## Remaining work

- [ ] **Tags**
  - [ ] `tags` and `item_tags` tables, name unique per heap
  - [ ] `GET/POST /heaps/{id}/tags`, `PATCH /tags/{id}`; no delete
  - [ ] Tags in Add item; typing a new name creates the tag
  - [ ] Manage tags view
- [ ] **Blockers**
  - [ ] `blocked_by_item_id` column; a blocker from another heap is rejected with 422
  - [ ] Cycle detection inside the write transaction (`blocking_cycle`)
  - [ ] Blocker inheritance in `rank()` (memoized walk, optional boost)
  - [ ] Pick the blocking item in Edit item
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
  - [ ] CI pipeline: fmt, clippy, tests, web checks, OpenAPI drift check
  - [ ] Store and service suite on MySQL via testcontainers
  - [ ] Table-driven access-control test (non-member, member, admin, archived heap)
  - [ ] Playwright golden paths

## Suggested order

1. Tags. Add item, saved views and parameter links all build on them.
2. Blockers, with cycle detection and inherited priority.
3. Edit item view and list controls (sort, filters, search).
4. Teams and heaps management.
5. Custom parameters, which also completes the ranking formula.
6. CI with the MySQL run, alongside the steps above rather than at the end.
