# Progress

What's built, what's next and what's undecided, against [requirements.md](requirements.md),
[offering.md](offering.md) and [architecture.md](architecture.md). Last reviewed 2026-11-08.

This file records state, not history: one line per area, and the pull requests for how an area got there. How
something works belongs in the design doc or the code each row points to.

Status: ✅ done · 🟡 partial · ⬜ not started. In the Spec column, Req, Off and Arch are requirements.md, offering.md
and architecture.md.

## Status

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts | Off Auth §2–3, Arch Auth §4 | ✅ | Register, sign in by email or username, sign out. argon2id, hashed session tokens, CSRF header, five sign-in attempts per account per 15 minutes |
| Personal team and heap | Off Teams §3 | ✅ | Made on registration and named by the username. Private, so it takes no members |
| Readable IDs | Arch Handles | ✅ | Usernames, slugs and item keys in every URL and API path, `/h/…` short links with Copy link, slugs editable on the team and heap pages |
| Items | Req Items §1–4, 6–7 | ✅ | Loose and exact due times with fuzzy labels. Need is required on every item and never cleared. Reopen within 7 × 24 h |
| Ranking | Req Priority | 🟡 | Formula `V2`: need, age scaled by need, deadline, value per effort, custom parameters, blocker inheritance. The heap's own weights can't be edited yet |
| List view | Req Views: List items | ✅ | Each state pinned, shown or hidden. Sort by priority, due, effort, impact or a parameter. One Filters panel for tags, need, effort, impact and parameter options. Client text search (`/`). The view lives in the URL |
| Closed items | Req Views: Closed items | ✅ | Sections Today, This week and Earlier. Reopen while allowed. Cursor paging |
| Add item | Req Views: Add item | ✅ | A dialog (`N`, or + on a phone) with every field, prefilled from the list's filters. Says where the item landed. Add another is remembered per device |
| Item pane | Req Views: Edit item | ✅ | Over the list: a pane beside it, a panel on a tablet, a sheet on a phone. Every field, the state and the blocker, each change saved as it's made |
| Tags | Req Items §5, Manage tags | ✅ | Unique per heap ignoring case, never deleted. Created from the picker. Manage tags page |
| Blockers | Req Items §8, Priority §5 | ✅ | Cycle check in the write transaction. "Blocks …" contributor |
| Custom parameters | Req Custom parameters, Manage parameters | ✅ | Admins add, weigh, reorder and archive parameters and options (open items switch to the replacement) and link tags. Values count in ranking, contributors, filters and sort |
| Saved views | Req Heaps §4 | 🟡 | API done, shared by the team. Hidden in the web app until they have a place in the Filters panel |
| Teams and members | Req Heaps §2–3, Off Teams | ✅ | Create and rename teams, add members found by `GET /users`, promote, demote, remove, leave. A team always keeps an admin |
| Heaps | Off Teams §4, 9 | ✅ | Create, rename, archive and unarchive from the Team page. An archived heap is read-only, with an Unarchive banner for admins |
| Audit log | Off Audit | ✅ | Written in every write's transaction. Each entity's history where it's edited. Heap and team Activity pages |
| OIDC sign-in | Off Auth §1 | ⬜ | |
| CLI | [cli.md](cli.md) | 🟡 | Server half done: OAuth with PKCE for the `upheap-cli` client, rotating refresh tokens, bearer tokens on every endpoint, the consent page. Not started: `crates/api-types`, `crates/cli`, `/me/tokens` |
| API conventions | Arch API, Access control | ✅ | `If-Match`, problem+json with stable codes, `404` for outsiders, `409` on archived heaps |
| Deployment | Arch Configuration, [operations.md](operations.md) | ✅ | One binary on SQLite, MySQL or Postgres. Docker image, `/healthz` and `/readyz`. Cloud Run with Neon in us-west1, deployed from `main`, registration off |
| Local build | Arch Local build | ✅ | The backend in a browser worker on SQLite-wasm. Export, import and one tab at a time. Published to https://bzzzzzz.github.io/upheap/ after every merge |
| Web app at every width | [app-ux.md](app-ux.md) | 🟡 | Shell, list, item, Add, Closed, Your data, sign-in, Teams and Team fitted to phones and wide screens in the design tokens. Left: Activity, Tags and Parameters at phone width. The tablet keeps app-ux.md's layout, unreviewed |
| Testing and CI | Arch Testing | ✅ | Rust suites on SQLite, MySQL and Postgres. The access-control table test. Playwright golden paths at three widths and against the local build. OpenAPI and generated types checked for drift |

## Next

In rough order.

1. **Heap weights.** Admins edit the heap's need, age, deadline and value weights: an API field and a place on the
   heap's pages (Req Custom parameters §2).
2. **The CLI** ([cli.md](cli.md)): `crates/api-types` with the DTOs moved out of `api`, no behaviour change; then
   `crates/cli` with `auth login` and `logout`, `heap list` and `use`, and `item list`; then `item add`, `show`,
   `edit` and the state verbs; `tag list`, `add` and `edit`; last `/me/tokens` on the server and its Account card.
3. **Need in the list.** Sort by need, and show it in the row so a critical item stands out without opening it.
4. **Phone width** for Activity, Tags and Parameters, in the design tokens.
5. **Saved views** back in the web app, inside the Filters panel.
6. **OIDC sign-in**, with a username step on first sign-in (Arch Handles §7).
7. **Invitations** by email or link (Off Teams §6).
8. **Backups.** Check Neon's restore window on the plan in use before real data goes in.
9. **Docs.** Finish the restructuring: `ux.md` from ux-overview.md and app-ux.md with screens.md's core flows;
   `product.md` from requirements.md and offering.md. The rules are in [CLAUDE.md](../CLAUDE.md).

## Later

- **Keyboard.** Shortcut hints on controls, a hint bar, `?` for help, and ⌘K across heaps, which needs a server
  search endpoint ([ux-overview.md](ux-overview.md), Later: keyboard).
- **Tablet.** Its own design pass. Today it keeps app-ux.md's layout.
- **First-set leftovers.** The member count beside Team in More and open counts in the heap switcher, both already
  in `GET /me`; an "Added" row among the phone item sheet's fields; a badge on More fields when a new tag asks for a
  parameter; a back button on phone pages outside a heap; a confirm before demoting yourself; an optional link name
  when adding a heap; "team · heap" over a closed item's title.
- **Installable app.** A PWA for the local build, then store builds with Tauri ([app-ux.md](app-ux.md), Later).
- **Playwright against Postgres.** Optional; the config starts the server on in-memory SQLite.

## Open questions

1. **Tablet layout.** app-ux.md and the mockups disagree on the tablet. The phone tabs and the item's state
   buttons follow the mockups; the tablet hasn't been reviewed.
2. **Username suggestions** when the chosen one is taken: worth the server work?
3. **Old OAuth rows.** Used codes and expired or revoked tokens stay in their tables. They are authentication
   plumbing, which may be deleted ([engineering principles](engineering-principles.md#data)), so a small cleanup on
   each exchange, as sign-in attempts have, can come with `/me/tokens`.
4. **An exported local database on a server.** Is moving one there worth building? The local user has no
   password, so an import would need a way to claim the account.
