# Work

What's in flight, what's next, and ideas. Last reviewed 2026-11-09.

## Tracks

In flight. Each track's doc holds its progress.

- [Getting in](getting-in.md): invite links, password change and reset.
- [Work with agents](work-with-agents.md): notes, tasks and reminders with context and questions, worked on with
  AI agents through the CLI.
- [Side tasks](side-tasks.md): backups, old OAuth rows, confirmations, Activity at phone width, and what holds an
  item back. Small pieces beside the two tracks.

## Next

Defined well enough to start, in rough order.

- **Need in the list.** Sort by need (Product List items §5), and show it in the row so a critical item stands out.
- **Blocked items whose blocker has closed.** They stay blocked and hidden (Product List items §2, Priority §2).
  Bring them back to the user's attention when the blocker closes.
- **"Start next" in a shared heap.** The offer after Done shows only when nothing in the heap is in progress, which
  with teammates is almost never. Decide what it means when others have work in progress.
- **A view across heaps.** Each user orders their own heaps, and that order is how items from different heaps
  compare in one list (Product Heaps §4). Designed as heap weights in the Work with agents track, stage 2.
- **OIDC sign-in** (Product Accounts §3), with a username step on first sign-in (Arch Handles §7).

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
