# Progress

What's built, what's next and what's undecided, against [product.md](../product.md) and
[architecture.md](../architecture.md). Last reviewed 2026-11-09.

This file records state, not history: one line per area, and the pull requests for how an area got there. How
something works belongs in the design doc or the code each row points to.

Status: ✅ done · 🟡 partial · ⬜ not started. In the Spec column, Product and Arch are product.md and architecture.md.

## Status

| Area | Spec | Status | Notes |
| --- | --- | :---: | --- |
| Local accounts | Product Accounts §1–2, Arch Auth §4 | ✅ | Register, sign in by email or username, sign out. argon2id, hashed session tokens, CSRF header, five sign-in attempts per account per 15 minutes |
| Personal team and heap | Product Teams §3 | ✅ | Made on registration and named by the username. Private, so it takes no members |
| Readable IDs | Arch Handles | ✅ | Usernames, slugs and item keys in every URL and API path, `/h/…` short links with Copy link, slugs editable on the team and heap pages |
| Items | Product Items §1–4, 6–7 | ✅ | Loose and exact due times with fuzzy labels. Need is required on every item and never cleared. Reopen within 7 × 24 h |
| Ranking | Product Priority | ✅ | Formula `V2`: need, age scaled by need, deadline, value per effort, custom parameters, blocker inheritance. Admins edit every weight, the heap's and its parameters', on Settings › Ranking, with a preview of the top of the list |
| List view | Product List items | ✅ | Each state pinned, shown or hidden. Sort by priority, due, effort, impact or a parameter. One Filters panel for tags, need, effort, impact and parameter options. Client text search (`/`). The view lives in the URL |
| Closed items | Product Closed items | ✅ | Sections Today, This week and Earlier. Reopen while allowed. Cursor paging |
| Add item | Product Add item | ✅ | A dialog (`N`, or + on a phone) with every field, prefilled from the list's filters. Says where the item landed. Add another is remembered per device |
| Item pane | Product Edit item | ✅ | Over the list: a pane beside it, a panel on a tablet, a sheet on a phone. Every field, the state and the blocker, each change saved as it's made |
| Tags | Product Items §5, Manage tags | ✅ | Unique per heap ignoring case, never deleted. Created from the picker. Renamed and described in the heap's Settings |
| Blockers | Product Items §8, Priority §5 | ✅ | Cycle check in the write transaction. "Blocks …" contributor |
| Custom parameters | Product Custom parameters, Manage parameters | ✅ | Admins add, weigh, reorder and archive parameters and options (open items switch to the replacement) and link tags. Values count in ranking, contributors, filters and sort |
| Teams and members | Product Teams | ✅ | Create and rename teams, add members found by `GET /users`, promote, demote, remove, leave. A team always keeps an admin |
| Heaps | Product Teams §4, 9, Heaps | ✅ | Create from the Team page; rename, change the link name, archive and unarchive there or in the heap's Settings. An archived heap is read-only, with an Unarchive banner for admins |
| Audit log | Product Audit log | ✅ | Written in every write's transaction. Each entity's history where it's edited. Heap and team Activity pages |
| OIDC sign-in | Product Accounts §3 | ⬜ | |
| CLI | [cli.md](cli.md) | ✅ | Server: OAuth with PKCE for the `upheap-cli` client, rotating refresh tokens, bearer tokens on every endpoint, the consent page. `crates/api-types` shared by `api` and `crates/cli`. Commands: `auth login` and `logout` with the sign-in in the OS keychain, `heap list` and `use`, `item list` with the list flags, `--view`, `--closed`, `--json` and `--quiet`, `item add`, `show` (with `--history`), `edit` and the state verbs `start`, `block`, `done`, `wontdo` and `reopen`, and `tag list`, `add` and `edit`. Login names the sign-in after the machine, and Account's Command line card lists the CLI's sign-ins and ends them (`/me/sign-ins`). Released from a `v*` tag: static binaries for Apple silicon macOS and x86_64 and aarch64 Linux, and the `bzzzzzz/tap/upheap` cask |
| API conventions | Arch API, Access control | ✅ | `If-Match`, problem+json with stable codes, `404` for outsiders, `409` on archived heaps |
| Deployment | Arch Configuration, [operations.md](../operations.md) | ✅ | One binary on SQLite, MySQL or Postgres. Docker image, `/healthz` and `/readyz`. Cloud Run with Neon in us-west1, deployed from `main`, registration off |
| Local build | Arch Local build | ✅ | The backend in a browser worker on SQLite-wasm. Export, import and one tab at a time. Published to https://bzzzzzz.github.io/upheap/ after every merge |
| Web app at every width | [ux.md](../ux.md) | 🟡 | Shell, list, item, Add, Closed, Your data, sign-in, Teams, Team and heap Settings (General, Tags, Parameters, Ranking) fitted to phones and wide screens in the design tokens. Left: Activity at phone width. The tablet keeps ux.md's layout, unreviewed |
| Testing and CI | Arch Testing | ✅ | Rust suites on SQLite, MySQL and Postgres. The access-control table test. Playwright golden paths at three widths and against the local build. OpenAPI and generated types checked for drift |

## Next

In rough order.

1. **Retire cli.md**: the CLI is built, so its mechanisms move into architecture.md and its choices into decisions.md (docs rule 3).
2. **Need in the list.** Sort by need, and show it in the row so a critical item stands out without opening it.
3. **Phone width** for Activity, in the design tokens.
4. **OIDC sign-in**, with a username step on first sign-in (Arch Handles §7).
5. **Backups.** Check Neon's restore window on the plan in use before real data goes in.

## Later

Ideas, not commitments. product.md holds the commitments.

- **Saved views.** Named views of a heap, defined by its filters and shared by the team, so "ready to pick up" or
  "blocked work" is one click without becoming a separate backlog. The API is built; the web app hides them until
  they have a place in the Filters panel ([decisions.md](../decisions.md), decision 27).
- **Invitations** by email or link, and membership from identity-provider groups. Today an admin adds an existing
  user.
- **Keyboard.** Shortcut hints on controls, a hint bar, `?` for help, and ⌘K across heaps, which needs a server
  search endpoint ([ux.md](../ux.md#later-keyboard)).
- **Tablet.** Its own design pass. Today it keeps ux.md's layout.
- **First-set leftovers.** The member count beside Team in More and open counts in the heap switcher, both already
  in `GET /me`; an "Added" row among the phone item sheet's fields; a badge on More fields when a new tag asks for a
  parameter; a back button on phone pages outside a heap; a confirm before demoting yourself; an optional link name
  when adding a heap; "team · heap" over a closed item's title.
- **Not yet designed.** Onboarding beyond the empty heap's one line (how ranking works, what effort, impact and
  loose due times mean, that nothing is deleted); a page for a `404` and one for when the local backend can't open
  its storage; an offline banner, where today each page shows its own error; the `500` request ID shown for a bug
  report; a searchable blocker picker for long heaps; a link from "parameter in use" to the items holding a value; a
  nudge on a blocked item whose blocker has closed; bulk actions, which `GridList` was chosen with in mind;
  notifications and due reminders; a countdown on a throttled sign-in; hiding Create account when registration is
  closed, which needs the server to say so before sign-in.
- **Password and account.** No password change, reset, email change or "sign out everywhere". The server sends no
  email, so a reset needs an admin path or a design.
- **Installable app.** A PWA for the local build, then store builds with Tauri ([ux.md](../ux.md#later-an-installable-app)).
- **Playwright against Postgres.** Optional; the config starts the server on in-memory SQLite.

## Open questions

1. **Tablet layout.** ux.md and the mockups disagree on the tablet. The phone tabs and the item's state buttons
   follow the mockups; the tablet hasn't been reviewed.
2. **Username suggestions** when the chosen one is taken: worth the server work?
3. **Confirmations.** Archiving a heap and demoting yourself act at once, while archiving a view, option or
   parameter, removing a member and leaving confirm first. The rule (ux.md, Rules §1) says Undo for the reversible
   and a confirm for what affects others or can't be undone; the code doesn't follow it everywhere yet.
4. **Old OAuth rows.** Used codes and expired or revoked tokens stay in their tables. They are authentication
   plumbing, which may be deleted ([engineering principles](../engineering-principles.md#nothing-is-deleted)), so a small cleanup on
   each exchange, as sign-in attempts have, is still to do.
5. **An exported local database on a server.** Is moving one there worth building? The local user has no
   password, so an import would need a way to claim the account.
