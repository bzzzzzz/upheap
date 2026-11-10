# Work

What's in flight, what's next, and ideas. Last reviewed 2026-11-09.

## Tracks

In flight. Each has a doc with its own progress.

- [Getting in](getting-in.md): invite links, password change and reset. Stage 1 of 2, piece 2 next.
- [Work with agents](work-with-agents.md): notes, tasks and reminders with context and questions, worked on with
  AI agents through the CLI. Stage 1 of 2, piece 3 next.

## Next

Defined well enough to start, in rough order.

- **Need in the list.** Sort by need (Product List items §5), and show it in the row so a critical item stands out.
- **What holds an item back.** The contributors say why an item is here, not why it isn't higher (Product Priority
  §9). Say what's missing or low too: no due time, unset sizes, a low need.
- **Blocked items whose blocker has closed.** They stay blocked and hidden (Product List items §2, Priority §2).
  Bring them back to the user's attention when the blocker closes.
- **"Start next" in a shared heap.** The offer after Done shows only when nothing in the heap is in progress, which
  with teammates is almost never. Decide what it means when others have work in progress.
- **A view across heaps.** Each user orders their own heaps, and that order is how items from different heaps
  compare in one list (Product Heaps §4). Designed as heap weights in the Work with agents track, stage 2.
- **Phone width for Activity,** in the design tokens.
- **OIDC sign-in** (Product Accounts §3), with a username step on first sign-in (Arch Handles §7).
- **Backups.** Check Neon's restore window on the plan in use before real data goes in.
- **Confirmations.** Archiving a heap and demoting yourself act at once, against ux.md, Rules §1. Make the code
  follow the rule.
- **Old OAuth rows.** Used codes and expired or revoked tokens stay in their tables. Clean them up on each
  exchange, as sign-in attempts are.

## Ideas

Not commitments. [product.md](../product.md) holds the commitments.

- **Saved views** in the web app. The API is built; they wait for a place in the Filters panel.
- **Email:** invitations and reset by mail, address verification, changing an address.
- **Membership from identity-provider groups,** with OIDC.
- **Keyboard:** shortcut hints, a hint bar, `?` for help, and ⌘K across heaps
  ([ux.md](../ux.md#later-keyboard)).
- **Tablet.** Its own design pass. It keeps ux.md's layout, which the mockups disagree with.
- **Small web leftovers:** a member count on the team row, open counts in the heap switcher, an "Added" row in
  the phone item sheet, a back button on phone pages outside a heap, an optional link name when adding a heap.
- **Onboarding** beyond the empty heap's one line: how ranking works, what the sizes and loose due times mean.
- **Error pages:** a `404`, the local backend failing to open its storage, an offline banner, the `500` request ID.
- **Hiding Create account when registration is closed,** and a countdown on a throttled sign-in.
- **A searchable blocker picker,** bulk actions, and a link from "parameter in use" to the items holding a value.
- **Notifications and due reminders.**
- **Installable app:** a PWA for the local build, then store builds
  ([ux.md](../ux.md#later-an-installable-app)).
- **Username suggestions** when the chosen one is taken.
- **Renaming a personal team.** The web app hides Rename on it, and the server still offers it.
- **Moving an exported local database to a server.** An import would need a way to claim the account.
- **In the CLI:** a `--create-tags` flag, and profiles for several servers.
- **Playwright against Postgres.**
