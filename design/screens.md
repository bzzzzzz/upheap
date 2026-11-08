# Screens

Draft. An inventory of every screen the web app can show, and every dialog, sheet, popover, menu and inline panel
on those screens, now and in what the design docs plan. It's the starting list for UX mockups. Layout rules and
interactions are in [app-ux.md](app-ux.md); this file only says what exists, where, for whom, how often, and how
much each install needs it.

Each entry is grounded in a doc or in the code. Status:

- **built:** in `web/src` today.
- **designed:** described in a design doc, not built.
- **implied:** follows from a use case, an API error or a rule, but no doc describes the UI. Treat these as
  proposals.

Frequency, for an average user (a member of one shared team plus their personal team):

| Rating | Meaning |
| :---: | --- |
| 5 | Constantly: several times per session |
| 4 | Daily |
| 3 | Weekly |
| 2 | Monthly, or occasional |
| 1 | Rare: once or a few times ever (setup, recovery) |

Must-have, rated separately for the local build (**Must (local)**) and the server build (**Must (server)**): can
the user do their job if this is absent?

| Rating | Meaning |
| :---: | --- |
| 5 | The core loop breaks without it; there's no workaround |
| 4 | The job gets done, but painfully, or a core flow has a dead end (such as no way to recover from an error). Needed for a first release |
| 3 | A clear improvement to a core flow, with a workable fallback |
| 2 | Nice to have; helps occasionally |
| 1 | Polish, or for edge cases only |
| n/a | Doesn't exist in that install |

Where one role has several presentations (the wide pane and the phone sheet, the sidebar and the phone tabs), the
rating is for the role. Every 4 and 5 says why in one line.

IDs: `G-` global and shell, `S-` screen (has a route), `M-` overlay (dialog, sheet, popover, menu, toast), `P-`
inline panel or section on a screen, `E-` error, empty or system state.

Widths are app-ux.md's: **phone** under 640 px, **tablet** 640–1023 px, **wide** 1024 px and up.

Routes are today's (UUIDs). [readable-ids.md](readable-ids.md) replaces them with handles; the API on this branch
already uses `/teams/{team}/heaps/{heap}/items/{item}`, and the web app's API calls are being moved over, but its
routes still take UUIDs. Each screen gives both where they differ.

## Core flows

The jobs each install must support, step by step, with the entries each step touches. Each flow is marked
**loop** (done many times a day), **setup** (once, or rarely) or **recovery** (after a mistake or a failure);
steps that differ from their flow say so.

### Single user (local build)

**L1. First open** (setup)

1. Open the page. The browser's database opens, with the user, the team and the heap "Personal" made on the first
   start. If storage can't open: S-backend-failed. If Upheap is open in another tab: S-tab-lock.
2. Land on S-heap, empty (E-heap-empty), with G-storage-note saying the data lives only in this browser.
3. Add the first items with M-add-item: a title, and effort, impact, P-due and P-tag-picker if known. Add another
   keeps the dialog open.
4. See them ranked on S-heap, each P-row with its contributors.

**L2. Daily loop** (loop)

1. See what's next: S-heap. In-progress items are pinned on top, then Up next.
2. Start one: Start in M-item, or swipe its P-row left (phone). G-toast offers Undo.
3. Finish it: P-row's state circle or a swipe right, or Done in M-item. G-toast offers Undo.
4. Add what came up: M-add-item, from the Add field, `N`, or + on a phone.
5. Re-size or re-date: P-row → M-item, then effort, impact or P-due. The item moves at once.
6. Block: Blocked in M-item, and P-blocker for what it waits for. It leaves the list, since blocked items are
   hidden by default.
7. Unblock: P-hidden-count's Show them puts blocked items in the list; open one in M-item and pick Unblock or
   Start.
8. Find an item: P-search, then M-item.

**L3. Weekly review** (weekly)

1. Open S-closed from the shell (G-shell-wide, or G-shell-phone's Closed tab).
2. Reopen anything closed by mistake with P-reopen, within its week. This is also the fix for a missed Undo
   (recovery).
3. Back on S-heap, close what won't happen as Won't do, and re-size or re-date the rest in M-item. P-refresh
   re-ranks at the current time.

**L4. Keeping the data safe** (setup, then recovery)

1. G-storage-note links to S-account (M-more on a phone).
2. P-local-export downloads a copy. Repeat now and then.
3. To restore, or to move to another browser: P-local-import, then M-local-import-confirm. The page reloads with
   the copy's heaps.

Not core in the local build: more heaps (S-team, P-heap-new, M-heap-switcher), tags pages, parameters, saved views,
filters and sort. One heap and its list do the job.

### Multi user (server build)

**S1. Sign up and sign in** (setup)

1. S-register: name, username, email, password. Lands on the personal heap, empty (E-heap-empty).
2. Next time: S-sign-in. When the 30-day session ends, G-session-expired sends the user back there (recovery).

**S2. Getting into a shared heap** (setup)

1. Someone creates a team on S-teams and becomes its admin; S-team opens.
2. The admin adds a heap with P-heap-new.
3. The admin adds each teammate with P-member-add. Teammates must have registered first (S1); invitations are
   later (S-invite).
4. The admin makes a second admin in P-member-row, so the team doesn't hang on one person.
5. A teammate finds the team's heap in G-shell-wide's sidebar or M-heap-switcher.

**S3. Daily loop in a shared heap** (loop)

1. Pick the heap: the sidebar, or M-heap-switcher on a phone. Most users move between their personal heap and the
   team's.
2. Then L2, unchanged. Teammates' changes arrive as the list refetches, and rows animate to their new places.

**S4. Coordinating with others**

1. See what others changed: the refreshed list (loop); P-item-history for one item, S-heap-activity for the heap.
2. Conflicts: saving over a teammate's change shows E-conflict, and M-item shows their version; redo the change if
   it still applies (recovery).
3. Point someone at an item: G-copy-link (designed), or the address bar today. The link opens M-item, after
   S-sign-in if signed out, or E-item-missing if the reader can't see that heap.
4. Share a slice of the heap: P-saved-views. Not core: filters and search rebuild it.

**S5. Admin basics** (setup, occasional)

1. Members on S-team: P-member-add, and Make admin, Make member or Remove (M-member-remove) in P-member-row.
2. Heaps on S-team: P-heap-new, and rename or archive in P-heap-row; G-archived-banner to unarchive.
3. Ranking: S-parameters for custom parameters (optional), P-heap-weights once it exists.
4. Leaving: P-leave, or P-last-admin saying why it's missing.

Only adding members, making an admin, removing someone and adding a heap are needed for a first release; the rest
is occasional.

## Summary

Sorted by the higher of the two must-have ratings, then by frequency, then by name. Each entry's details justify its
4s and 5s.

| ID | Name | Parent | Freq | Must (local) | Must (server) | Status |
| --- | --- | --- | :---: | :---: | :---: | --- |
| M-add-item | Add an item | S-heap | 5 | 5 | 5 | built |
| S-heap | Heap list (Open) | — | 5 | 5 | 5 | built |
| M-item | Item pane, panel or sheet | S-heap | 5 | 5 | 5 | built |
| P-row | List row: state circle, swipe actions | S-heap | 5 | 5 | 5 | built |
| P-due | Due picker | M-item, M-add-item | 4 | 5 | 5 | built |
| S-sign-in | Sign in | — | 2 | n/a | 5 | built |
| S-team | Team | — | 2 | 2 | 5 | built |
| P-member-add | Add a member | S-team | 1 | n/a | 5 | built |
| S-register | Create account | — | 1 | n/a | 5 | built |
| P-heap-new | New heap | S-team | 1 | 2 | 5 | built |
| S-teams | Teams | — | 1 | n/a | 5 | built |
| G-shell-phone | Phone top bar and tabs | — | 5 | 4 | 4 | built |
| G-storage-note | Storage note (local build) | shell | 5 | 4 | n/a | built |
| G-shell-rail | Tablet rail | — | 5 | 4 | 4 | built |
| G-shell-wide | Wide sidebar | — | 5 | 4 | 4 | built |
| M-heap-switcher | Heap switcher | shell | 4 | 2 | 4 | built |
| P-search | Search box | S-heap | 4 | 4 | 4 | built |
| S-closed | Closed items | — | 3 | 4 | 4 | built |
| P-hidden-count | Hidden items note | S-heap | 3 | 4 | 4 | built |
| M-more | More menu (phone) | G-shell-phone | 3 | 4 | 4 | built |
| P-local-export | Download a copy (local build) | S-account | 2 | 4 | n/a | built |
| P-reopen | Reopen | S-closed, M-item | 2 | 4 | 4 | built |
| G-session-expired | Session expired | shell | 2 | n/a | 4 | built |
| S-account | Account | — | 1 | 4 | 1 | built |
| E-conflict | Changed by someone else | M-item, S-tags, S-parameters, S-team | 1 | 1 | 4 | built |
| P-member-row | Member row (role, remove) | S-team | 1 | n/a | 4 | built |
| P-local-import | Open a copy (local build) | S-account | 1 | 4 | n/a | built |
| S-tab-lock | Open in another tab | — | 1 | 4 | n/a | built |
| M-local-import-confirm | Replace everything here? | P-local-import | 1 | 4 | n/a | built |
| G-toast | Undo toast | S-heap | 5 | 3 | 3 | built |
| E-loading | Loading states | every screen | 4 | 2 | 3 | built |
| P-tag-picker | Tag picker | M-item, M-add-item, P-param-card | 4 | 2 | 3 | built |
| M-filters | Filters | S-heap | 3 | 2 | 3 | built |
| P-blocker | Waiting for (blocker picker) | M-item | 3 | 3 | 3 | built |
| G-copy-link | Copy link | M-item, S-heap, S-team | 2 | 1 | 3 | designed |
| G-offline | Can't reach the server | shell | 1 | n/a | 3 | implied |
| E-heap-empty | Empty heap | S-heap | 1 | 3 | 3 | built |
| E-item-missing | Item not found | M-item | 1 | 2 | 3 | built |
| S-backend-failed | Local backend didn't start | — | 1 | 3 | n/a | implied |
| E-load-error | Page failed to load | every screen | 1 | 3 | 3 | built |
| P-saved-views | Saved views bar | S-heap | 4 | 2 | 2 | built |
| P-item-history | Item history | M-item | 3 | 1 | 2 | built |
| P-param-picker | Parameter picker | M-item, M-add-item | 3 | 2 | 2 | built |
| P-sort | Sort select | S-heap | 3 | 2 | 2 | built |
| P-view-changed | Changed-view actions | P-saved-views | 2 | 2 | 2 | built |
| M-add-discard | Discard this item? | M-add-item | 2 | 2 | 2 | built |
| S-heap-activity | Heap activity | — | 2 | 1 | 2 | built |
| P-view-name | Name a view (new or rename) | P-saved-views | 2 | 2 | 2 | built |
| E-no-match | No items match | S-heap | 2 | 2 | 2 | built |
| S-parameters | Parameters | — | 2 | 2 | 2 | built |
| P-refresh | Refresh order | S-heap | 2 | 2 | 2 | built |
| P-tag-row | Tag row (inline edit) | S-tags | 2 | 1 | 2 | built |
| S-tags | Tags | — | 2 | 1 | 2 | built |
| M-view-settings | View settings | S-heap | 2 | 2 | 2 | built |
| S-invite | Accept an invitation | — | 1 | n/a | 2 | implied (later) |
| G-archived-banner | Archived heap notice | shell | 1 | 2 | 2 | built |
| S-oidc-username | Choose a username | — | 1 | n/a | 2 | designed |
| M-self-demote | Give up admin rights? | P-member-row | 1 | n/a | 2 | implied |
| P-heap-row | Heap row (rename, archive, open) | S-team | 1 | 2 | 2 | built |
| P-leave | Leave team | S-team | 1 | n/a | 2 | built |
| P-param-new | New parameter | S-parameters | 1 | 2 | 2 | built |
| S-no-heaps | No heaps yet | — | 1 | 2 | 2 | built |
| E-forbidden | Not allowed | every screen | 1 | n/a | 2 | implied |
| P-last-admin | Only-admin note | S-team | 1 | n/a | 2 | built |
| S-not-found | Page not found | — | 1 | 2 | 2 | implied |
| P-param-card | Parameter card | S-parameters | 1 | 2 | 2 | built |
| P-personal-team | Personal team variant | S-team | 1 | n/a | 2 | designed |
| P-heap-weights | Ranking weights (age, deadline, value) | S-heap-settings or S-parameters | 1 | 2 | 2 | designed |
| E-registration-closed | Registration closed | S-register | 1 | n/a | 2 | built (message only) |
| M-member-remove | Remove member? | P-member-row | 1 | n/a | 2 | built |
| S-oauth-authorize | Sign in the CLI | — | 1 | n/a | 2 | designed |
| P-tokens | Signed-in clients (CLI tokens) | S-account | 1 | n/a | 2 | designed |
| E-throttled | Too many sign-in attempts | S-sign-in | 1 | n/a | 2 | built (message only) |
| E-bad-view | View can't be shown | S-heap | 1 | 2 | 2 | built |
| M-shortcuts | Keyboard shortcuts help | shell | 2 | 1 | 1 | implied |
| P-option-archive | Archive option, with replacement | P-param-card | 1 | 1 | 1 | built |
| P-param-archive | Archive parameter | P-param-card | 1 | 1 | 1 | built |
| M-heap-archive | Archive this heap? | P-heap-row | 1 | 1 | 1 | implied |
| M-view-archive | Archive this view? | P-saved-views | 1 | 1 | 1 | built |
| P-param-archived | Archived parameters | S-parameters | 1 | 1 | 1 | built |
| S-oauth-done | Back to the terminal | — | 1 | n/a | 1 | implied |
| P-display-name | Display name | S-account | 1 | 1 | 1 | built |
| S-heap-settings | Heap settings | — | 1 | 1 | 1 | designed |
| M-pwa-install | Install the app | shell (local build) | 1 | 1 | n/a | designed (later) |
| P-param-tags | Linked tags | P-param-card | 1 | 1 | 1 | built |
| P-tag-new | New tag | S-tags | 1 | 1 | 1 | built |
| M-pwa-update | New version available | shell (local build) | 1 | 1 | n/a | designed (later) |
| P-param-history | Parameter history | P-param-card | 1 | 1 | 1 | built |
| P-tag-history | Tag history | P-tag-row | 1 | 1 | 1 | built |
| S-team-activity | Team activity | — | 1 | 1 | 1 | built |
| P-team-name | Team name and slug | S-team | 1 | n/a | 1 | built (slug designed) |
| P-view-history | View history | P-saved-views | 1 | 1 | 1 | built |

## First design and build set

What to design and build first: every entry rated 4 or 5 in either install, plus the entries rated 4 or 5 for
frequency that the workflow does use, marked *(frequency)* with the reason. Grouped by screen. Tablet is out of
scope for now; "browser" is 1024 px and up, "phone" under 640 px.

### Shell

G-shell-wide, G-shell-phone, M-more, M-heap-switcher, G-storage-note (local), G-session-expired (server), and
E-loading *(frequency: every screen shows it, and a blank page reads as broken)*.

- **Browser:** the sidebar with every heap (grouped by team when there are several), the open heap's Open and
  Closed, then Team (server), Account, Sign out (server) and the storage note (local). The sidebar is the heap
  switcher. Loading keeps the previous view, dimmed.
- **Phone:** the top bar with the heap name, which opens M-heap-switcher as a sheet; the tabs Open, Closed and More;
  the More sheet with Team (server), Account and Sign out (server); the storage note under the content (local).
- **Both:** a `401` goes to S-sign-in (server).

### S-heap: Heap list

S-heap, P-row, P-search, P-hidden-count, G-toast *(frequency: shown after every state change from the list; Undo
is the cheap fix for a mis-tap)*, and P-saved-views *(frequency: used daily, and its chips sit at the top of the
list at both widths, so leaving them out of the first design would mean laying the list out twice)*.

- **Browser:** heap name; the saved view chips; the Add field; search; the Pinned and Up next groups; rows with the
  state circle, position, title, due chip, tags, contributors, and effort and impact at the right edge; the
  selected row while M-item is open; the hidden-items note with its way to show blocked items; the toast at the
  bottom centre with Undo. No shortcut hints (see Deferred).
- **Phone:** one- or two-line rows; swipe right for done, left for In progress and Blocked; the chips scrolling
  sideways; search in the toolbar; + within thumb reach above the tabs; the toast above the tabs.

### M-item: Item

M-item, P-due, P-tag-picker *(frequency: used on most adds and edits, and it shares the pane and the Add dialog
with P-due)*, E-conflict (server).

- **Browser:** a pane beside the list: "#3 · Up next" and ✕; title; state (Open, In progress, Blocked, Done, and
  Won't do apart); "Why it's #3"; effort and impact as S M L XL; the due choices and Exact date; tags; description;
  the error line, where E-conflict says the pane now shows a teammate's version.
- **Phone:** a bottom sheet with a grab handle, above the keyboard: the same fields, and Start, Blocked and Done in
  reach of the thumb (app-ux.md The item §2).

### M-add-item: Add an item

M-add-item, with P-due and P-tag-picker as in M-item.

- **Browser:** a centred dialog near the top, 600 px wide: "Add to \<heap\> ▾" and ✕; the "Added at #4" line with
  Open it; title (focused), description; then labels beside their fields: need, effort, impact, due (starting at
  None), tags, and the parameters the tags ask for, each saying which tag asked; Add another; Cancel and Add. The
  tablet uses it too.
- **Phone:** a bottom sheet above the keyboard: "Add to \<heap\> ▾" with the "added at #4" note beside it; the
  title first; chips that scroll sideways for need, effort and impact (each opening a sheet of S M L XL), tags and
  due; More fields for the description and parameters; Add another, More fields and Add in the last row, reachable
  without closing the keyboard.

### S-closed: Closed items

S-closed, P-reopen.

- **Browser:** closed items, newest first, under Today, This week and Earlier, each with "Done · today by you" or
  "Won't do · 3 Nov by Sam" and Reopen inside its week; Load more; a row opens M-item.
- **Phone:** the same list at full width from the Closed tab, with Reopen as a 44 px target.

### S-account: Your data (local build)

S-account (local), P-local-export, P-local-import, M-local-import-confirm, S-tab-lock.

- **Browser:** Account with "Your data": the one-line explanation, Download a copy, Open a copy…, and the confirm
  naming the file ("Replace everything here with {file}?"). S-tab-lock as a full page instead of the app.
- **Phone:** the same in one column, reached from More, with the confirm as a sheet. S-tab-lock as on the browser.

### S-sign-in and S-register (server build)

S-sign-in, S-register.

- **Browser:** one centred column: Email, Password, Sign in, and the switch to Create account with Name, Username,
  Email, Password. Errors inline under the field they belong to.
- **Phone:** the same column at full width with 16 px inputs, the right keyboard for each field, and the button
  visible above the keyboard.

### S-teams and S-team (server build)

S-teams, S-team, P-heap-new, P-member-add, P-member-row.

- **Browser:** Teams: the user's teams with role, and Create team. Team, in the settings layout of ux-overview.md
  (title, one line, + New, rows): Heaps with Open and New heap; Members with role, Make admin, Make member and
  Remove (with today's confirm), and Add member's search with its results.
- **Phone:** both pages in one column; member rows with name and role on one line and their actions under ⋯; Add
  member's search at full width with results under it.

### Deferred

Not designed in this round. Built entries keep working as they are; they get their new design after the set
above.

- **Tablet:** G-shell-rail, and the tablet panel presentation of M-item and the other overlays.
- **Keyboard:** shortcut hints on controls, the hint bar, M-shortcuts and ⌘K. The keys already built (`N`, `/`,
  ↑ ↓, Enter, `x`, `⌘↵`) keep working, but no first-set design shows or depends on them.
- **List extras:** M-filters, P-sort, M-view-settings beyond showing blocked items, P-view-changed, P-view-name,
  M-view-archive, P-view-history, P-refresh, E-no-match, E-bad-view, E-heap-empty (today's line will do),
  M-shortcuts.
- **Item extras:** P-blocker (today's select will do), P-param-picker, P-item-history, E-item-missing,
  G-copy-link, M-add-discard.
- **Activity:** S-heap-activity, S-team-activity.
- **Tags and parameters:** S-tags, P-tag-new, P-tag-row, P-tag-history, S-parameters, P-param-new, P-param-card,
  P-option-archive, P-param-tags, P-param-archive, P-param-history, P-param-archived.
- **Heap settings:** S-heap-settings, P-heap-weights, P-heap-row, M-heap-archive, G-archived-banner, S-no-heaps.
- **Team extras:** M-member-remove beyond today's inline confirm, M-self-demote, P-leave, P-last-admin,
  P-team-name, P-personal-team, S-invite.
- **Account and sign-in extras:** S-account in the server build, P-display-name, P-tokens, S-oauth-authorize,
  S-oauth-done, S-oidc-username, E-throttled, E-registration-closed.
- **Errors and system states:** E-load-error, E-forbidden, S-not-found, G-offline, S-backend-failed, the `500`
  request ID.
- **Later anyway:** M-pwa-install, M-pwa-update.

## Global and app shell

The shell wraps every signed-in screen (`AppLayout.tsx`). Each width has its own navigation; the content and
overlays are the same components.

### G-shell-wide: Wide sidebar

- **Purpose:** navigation on a wide screen. Everyone signed in.
- **Shows:** the Upheap mark; every heap grouped by team (team names only when there are several teams; archived
  heaps say "(archived)"); the open heap's views: Open, Closed, Activity, Tags, Parameters, Team (the heap's team;
  left out in the local build, where the bottom link already goes there); at the bottom Teams (or the one team's name in the local build), Account, Sign out
  (only with `sign_out` in `GET /me` `allowed_actions`), and the local build's storage note.
- **Actions:** open a heap, open a heap view, open Teams, Account, sign out.
- **Layout:** the sidebar and content are one centred block (sidebar 15 rem plus a 48 rem column); the sidebar's
  colour runs to the left edge. An open item pane widens the block.
- **Freq 5:** always on screen. **Source:** app-ux.md Layout; `AppLayout.tsx` `Sidebar`. Built.
- **Must:** local 4, server 4. The only wide-screen route to Closed, Account, Team and other heaps. One role with
  G-shell-phone and G-shell-rail, rated per width.

### G-shell-rail: Tablet rail

- **Purpose:** the sidebar folded into an 80 px rail.
- **Shows:** a square button with the heap's initial (opens M-heap-switcher as a popover), then Open, Closed,
  Activity, Tags, Parameters, Team as icons over short labels, then Teams, Account, Sign out at the bottom. 44 px
  targets.
- **Freq 5.** **Source:** app-ux.md Tablet §1; `Sidebar` at `sm`. Built.
- **Must:** local 4, server 4. The same role as G-shell-wide at tablet width. Tablet is out of scope for now, so it's
  deferred anyway.

### G-shell-phone: Phone top bar and tabs

- **Purpose:** phone navigation.
- **Shows:** the heap name with a chevron at the top (opens M-heap-switcher as a sheet; also serves as the page
  title); bottom tabs Open, Closed, + (opens M-add-item), Activity, More. Respects safe-area insets.
- **Freq 5.** **Source:** app-ux.md Layout, Phone details; `PhoneTopBar`, `PhoneTabs`. Built.
- **Must:** local 4, server 4. The phone's only route to Closed and More. One role with G-shell-wide.
- Note: on pages outside a heap (Teams, Account) the tabs aren't shown and the top bar says "Upheap".

#### M-more: More menu (phone)

- **Purpose:** the heap views and account pages that don't fit in the tabs.
- **Trigger:** More tab. **Contents:** Team (not in the local build) with the team's name, Teams with their count
  or the team's name, Tags, Parameters, Account with the username, Sign out (server build only), as 52 px rows.
- **Phone:** a card over a dimmed page, sitting above the tabs. It opens on release, like any button.
- **Freq 3:** Tags and Parameters are occasional. **Source:** app-ux.md Layout, Phone details; `PhoneTabs`. Built.
- **Must:** local 4, server 4. On a phone, the only route to Account (export, local build), Team and Sign out (server
  build). Part of the phone shell's role.

### M-heap-switcher: Heap switcher

- **Purpose:** move between heaps (Req Heaps §3).
- **Trigger:** heap name (phone), heap initial (tablet). On a wide screen the sidebar list is the switcher.
- **Contents:** heaps grouped by team as 52 px rows, a check on the current one, "(archived)" on archived heaps.
  Open counts are designed, not built: they need the server.
- **Actions:** pick a heap; "New heap in \<team\>" for the current heap's team, when the team allows it, asks for a
  name and opens the new heap.
- **Presentation:** popover beside the rail on a tablet; bottom sheet on a phone.
- **Freq 4:** most users have a personal and a team heap. **Source:** app-ux.md Layout; `HeapSwitcher`. Built, apart from open counts.
- **Must:** local 2, server 4. Server: most users move between a personal and a team heap every day, and on a phone
  the only other route is through Team. On a wide screen the sidebar plays this role. Local: one heap is usual.

### G-toast: Undo toast

- **Purpose:** confirm a state change made from the list and offer to take it back (app-ux.md The list §3).
- **Trigger:** state circle, `x`, or a swipe. **Contents:** "“Title” is done." with Undo and ✕; gone after 6 s.
  Undo restores the previous state and blocker. An error variant ("That item is closed…") uses the alert colour and
  has no Undo.
- **Presentation:** bottom centre; above the tabs on a phone.
- **Freq 5.** **Source:** `UndoToast.tsx`, `HeapPage.tsx` `useChangeState`. Built.
- **Must:** local 3, server 3. Undo is a big help after a mis-tap or a stray swipe, but S-closed and P-reopen are a
  fallback.
- Only list changes get a toast. Changes in M-item save silently; there's no undo there.

### G-archived-banner: Archived heap notice

- **Purpose:** say a heap is read-only. **Who:** everyone in the heap; Unarchive for admins (`unarchive` in the
  heap's `allowed_actions`).
- **Shows:** "This heap is archived and read-only." with Unarchive, above every heap page.
- **Freq 1.** **Source:** Off Teams §9, TD Archived heaps; `ArchivedNotice`. Built.
- **Must:** local 2, server 2. Explains disabled controls; Unarchive is also on P-heap-row.

### G-storage-note: Storage note (local build)

- **Purpose:** say the data lives only in this browser, and whether the browser may clear it.
- **Shows:** normally "Stored only in this browser · Download a copy". It becomes sentences when there's more to
  say: ", which may clear it when space runs low" when the browser hasn't agreed to keep the data, and "Last copy
  downloaded 3 weeks ago." or "No copy downloaded yet." when the last download is over two weeks old or never
  happened (the date is kept in this browser's localStorage). The link goes to S-account.
- **Where:** sidebar footer on a wide screen; under the content on a tablet and phone.
- **Freq 5** (always visible, passive). **Source:** local-build.md Keeping the data §1; `StorageNote`. Built.
- **Must:** local 4, server n/a. The only thing that tells a user their heaps can vanish with the browser's storage,
  and where to keep a copy.

### G-copy-link: Copy link

- **Purpose:** share a short link (`/h/my-team/backlog/k3x9a`).
- **Where:** "Items, heaps and teams offer Copy link with the short form." Placement isn't decided: an item pane
  header button, a heap header menu and the team page are the obvious homes. Copying wants a confirmation toast
  ("Link copied").
- **Freq 2.** **Source:** readable-ids.md Short links. Designed.
- **Must:** local 1, server 3. Server: links are how teammates point at an item; the address bar is a workable
  fallback on a wide screen, less so on a phone.
- `/h/…` short links have no screen: the router replaces them with the full URL as it opens.

### G-session-expired: Session expired

- **Behaviour:** any `401`, from a query or a write (`main.tsx`, the query and mutation caches), sends the user to
  S-sign-in with the page they were on as `redirect`, as the route guard does for a signed-out visit, so after
  signing in they're back there. It replaces the page in history, so Back doesn't return to it signed out. During a
  pending navigation the page is the one being opened, so the guard and the handler agree. Nothing happens on
  S-sign-in itself, where a wrong password is a `401` too. Signing in drops the query cache, so whoever signs in
  sees nothing cached for the last user. The notice reads "Sign in to open that page." or names the item; there's
  no "your session ended" line.
- **Freq 2:** sessions last 30 days (`SESSION_TTL_DAYS`). Built.
- **Must:** local n/a, server 4. Without it a `401` leaves every page failing, with no way back to sign-in.

### G-offline: Can't reach the server

- **Purpose:** say requests are failing because the server is down (`503` `unavailable`) or the network is gone.
- Today queries retry twice, then each page shows its own error text (E-load-error). There's no shell-level banner.
- **Freq 1.** Implied by engineering-principles.md Errors §4 (dependency category) and TD Configuration §5.
- **Must:** local n/a, server 3. Per-page errors already say something failed; a banner says why.

### M-shortcuts: Keyboard shortcuts help

- **Purpose:** list the keys. The app is "shortcut-driven" (engineering-principles.md TypeScript §6) but nothing
  lists them.
- **Keys today:** `N` add an item, `/` search, ↑ ↓ move between rows (↓ starts on the first row), Enter open,
  `x` mark done, Escape close the item or clear the search, `⌘↵`/`Ctrl↵` add from the Add dialog, Enter in the
  title adds too.
- **Presentation:** a centred dialog on a wide screen (opened with `?`); no phone version needed.
- **Freq 2.** Implied.
- **Must:** local 1, server 1.

### Not in the docs

There is no command palette, global search across heaps, or notifications (in-app or email) in any doc. Teammates'
changes arrive by refetch on focus and on an interval, and items animate to their new positions (TD Session-stable
order §4–5). See Gaps.

## Auth and onboarding

### S-sign-in: Sign in

- **Route:** `/login`. **Who:** signed-out visitors (server build only; the local build never shows it).
- **Purpose:** sign in with a local account (Off Auth §2).
- **Shows:** the Upheap mark, a Sign in / Create account switch (tabs; S-register is the other tab), Email,
  Password and Sign in, in a 380 px card on a wide screen (WideSignIn). A problem whose `field` names one of the
  inputs shows under it, which gets focus; any other (`invalid_credentials`, E-throttled, E-registration-closed) is
  one line above the button. Editing an input clears its error. The board has no tagline, so the page has none.
- **From a link:** a signed-out visit to any page of the app goes to `/login?redirect=<path>` (a plain visit to `/`
  to `/login`). A notice above the switch says where it leads: "Sign in to open the link to 3n2ay." for an item
  whose segment has an item key's shape (five lowercase Crockford base32 characters, one a digit), so a crafted
  link can't put its own words there, and "Sign in to open that page." otherwise.
- **Exits:** to `redirect` when, as the URL parser normalises it, it's a same-origin path that the router matches
  to one of the signed-in pages; otherwise (`//evil.example`, `/.//evil.example`, `javascript:…`, `/LOGIN`, a path
  no page answers) to `/`, which redirects to the first unarchived heap.
- **Empty inputs:** an empty email or password gets `field_empty` with that field before the server counts the
  attempt, so a form sent before a password manager fills it doesn't use one up.
- **Designed additions:** a button per configured OIDC provider ("Sign in with Okta"), Off Auth §1, TD Auth §3.
  The phone board's "Email or username" waits on signing in by username, which the server doesn't do.
- **Phone:** the bare form at full width (PhoneSignIn): 48 px inputs with 16 px text, a 52 px button, and the
  keyboard each input wants (`type=email`, no autocapitalise or autocorrect for email and username).
- **Freq 2.** **Source:** `AuthPage.tsx`. Built.
- **Must:** local n/a, server 5. No way into the server build without it.

#### E-throttled: Too many sign-in attempts

- `429` `too_many_attempts` after five attempts in 15 minutes per address, with `Retry-After`. The server's message
  says how long to wait; the page shows it in the line above Sign in. A countdown, or disabling Sign in until then,
  isn't designed.
- **Freq 1.** **Source:** TD Auth §4. Built (message only).
- **Must:** local n/a, server 2.

### S-register: Create account

- **Route:** `/login`, register mode. **Who:** anyone, unless `ALLOW_REGISTRATION=false`.
- **Shows:** Name, Username, Email, Password with the hint "At least 10 characters", Create account, and under it
  "You'll start in your own heap. Teams come next." The Username field (readable-ids.md Sign-up): 2–39
  characters, lowercase letters, digits and single dashes, starting with a letter; permanent; becomes the personal
  team's slug.
- **Exits:** to `redirect`, as S-sign-in, or else to the new personal heap "Personal"
  (`/teams/{username}/heaps/personal`), empty (E-heap-empty).
- **Errors,** each under its input: `invalid_handle` (with a hint such as `my-team`) and `handle_taken` (field
  `username`), `email_taken` and `email_invalid` (`email`), `password_too_short` and `password_too_long`
  (`password`), `field_empty` (`display_name`). `registration_closed` shows above the button.
- **Freq 1.** **Source:** Off Auth §3, Teams §3; `AuthPage.tsx`. Built.
- **Must:** local n/a, server 5. The only way to get an account: there are no admin-made users, and OIDC isn't built.

#### E-registration-closed

- `registration_closed` when the server turns registration off. Today the form still offers "Create account" and
  shows the error after submit. Better: hide the switch. That needs the server to say so before sign-in, which no
  endpoint does.
- **Freq 1.** Built (message only); hiding it is implied.
- **Must:** local n/a, server 2.

### S-oidc-username: Choose a username

- **Purpose:** "When [OIDC] is [built], a first sign-in asks for a username before creating the user."
- **Shows:** the name and email from the provider, a username field with the format rules, Continue.
- **Freq 1.** **Source:** readable-ids.md Sign-up. Designed.
- **Must:** local n/a, server 2. Becomes 5 for an OIDC deployment once OIDC is built.

### S-oauth-authorize: Sign in the CLI

- **Route:** `/oauth/authorize` (server). **Purpose:** the CLI opens the browser, the user signs in with the normal
  web flow, and the server hands a token back to the CLI's loopback address (Off Auth §4, TD Auth §2).
- **Shows:** S-sign-in if signed out, with context ("Sign in to use the Upheap CLI"). Whether a consent step ("Allow
  `upheap` on this computer?") follows isn't decided; only registered first-party clients are allowed.
- **Freq 1.** Designed (flow), screen implied.
- **Must:** local n/a, server 2. Needed by CLI users only; the web app's job doesn't depend on it.

### S-oauth-done: Back to the terminal

- **Purpose:** after the redirect, tell the user they can close the tab. The CLI's loopback listener could serve
  it, or the app could. Error variant for a refused or expired request.
- **Freq 1.** Implied.
- **Must:** local n/a, server 1.

### S-invite: Accept an invitation

- Invitations by email or link and IdP group membership are "a later iteration" (Off Teams §6, progress.md). Would
  need: an invite landing page (team name, who invited), sign in or register, Join; and on S-team an Invite panel.
- **Freq 1.** Implied (later).
- **Must:** local n/a, server 2. P-member-add covers joining for now.

### S-no-heaps: No heaps yet

- **Route:** `/` when the user has no unarchived heap (every heap archived, or they left all their teams before
  personal teams were fixed). **Shows:** "You don't have any heaps yet. Start one in {team}." or "Create a team to
  start one."
- **Freq 1.** **Source:** `routes.tsx` `NoHeaps`. Built.
- **Must:** local 2, server 2. Only when every heap is archived; the shell still reaches Team.
- With personal teams (readable-ids.md), a user always has a team, so only the first message can show.

### First run

There's no onboarding flow. A new user, or a fresh local build, lands on their empty personal heap: "Nothing here
yet. Add the first thing on your plate." (E-heap-empty). See Gaps.

## Heaps and items

### S-heap: Heap list (Open)

- **Route:** `/heaps/$heapId?…view params` today; `/teams/{team}/heaps/{heap}` designed.
- **Purpose:** "what should I work on?" The app opens here (Req Views: List items §1). **Who:** every team member;
  read-only in an archived heap.
- **Shows, top to bottom:**
  1. Heap name (hidden on a phone, where the top bar has it) and Refresh order (P-refresh).
  2. P-saved-views.
  3. "Add an item" field with an `N` hint: the way into M-add-item. Disabled in an archived heap.
  4. Toolbar: P-search, P-sort, Filters (with a count, M-filters), View (M-view-settings).
  5. Groups: **Pinned** (titled "In progress" when every pinned item is in progress; compact rows without
     contributors), **Up next** (the ranked items; untitled when nothing is pinned), **Blocked** (when blocked is
     shown and grouped). Each group numbers its rows from 1.
  6. P-hidden-count.
- **Actions:** open an item, mark one done, change state by swipe, add, search, sort, filter, change view, open or
  save a saved view.
- **Exits:** M-item (over the list), other heap views via the shell.
- **Widths:** wide shows effort and impact sizes ("M · L") at each row's right edge and parameter chips on the first
  line; phone and tablet put parameter chips on their own line. While another view loads, the previous one stays,
  dimmed.
- **Freq 5.** **Source:** Req Views: List items; app-ux.md The list; TD Ranking, Search; `HeapPage.tsx`. Built.
- **Must:** local 5, server 5. The ranked list is the answer to “what next?”; nothing else shows the order.

#### P-row: List row

- **Contents:** state circle (empty for open, half-filled for in progress, dashed for blocked), position, title,
  due chip ("Due 12 Mar", "Overdue · 12 Mar", or a fuzzy label such as "next week" with no "Due"), tag chips (grey),
  parameter chips (violet, "Energy: Low", only when set), and under them up to two contributors ("Due in 3 days ·
  S impact, XL effort").
- **Actions:** tap the row to open M-item; tap the circle to mark done (`change_state` needed); keyboard ↑ ↓ Enter
  `x`. With a mouse, hovering a row shows Start (open rows only) and ⋯ with the other states, Done excepted, as the
  circle does it. With a finger: drag right past about 100 px marks done; drag left opens two buttons, In progress
  and Blocked, with Open in place of the current state. Each change shows G-toast.
- **Selected:** the open item's row is highlighted while M-item is open.
- **Freq 5.** **Source:** app-ux.md The list §1–5, Phone details; `ItemRow.tsx`. Built.
- **Must:** local 5, server 5. The rows are the list: every item is opened from one, and the state circle and swipes
  are the quick way to finish.

#### P-search: Search box

- "Search titles and descriptions /". Filters on the client as the user types, keeping the ranked order. Not in the
  URL. Escape clears.
- **Freq 4.** **Source:** TD Search §2; `ListControls.tsx`. Built.
- **Must:** local 4, server 4. The only practical way to find one item in a long heap, above all on a phone, where
  browser find is awkward.

#### P-sort: Sort select

- Priority (default), Due soonest, Smallest effort, Largest impact, and "By {parameter}" for each parameter in use.
  A saved sort by an archived parameter shows "By {name} (archived, priority order)".
- **Freq 3.** **Source:** Req List items §5, TD Search §3; `ListControls.tsx`. Built.
- **Must:** local 2, server 2.

#### M-filters: Filters

- **Trigger:** Filters button (shows "Filters (3)" when on). **Contents:** Tags (any) chips, Effort S M L XL, Impact
  S M L XL, each parameter's options "(any)", and filters a saved view still has on archived parameters, labelled
  "(archived, ignored)" so they can be removed. Clear filters.
- **Semantics:** values within one filter match any; filters combine with AND. Filtering keeps priority order.
- **Presentation:** popover under the button (tablet, wide); bottom sheet above the keyboard (phone).
- **Freq 3.** **Source:** Req List items §6, app-ux.md The list §7; `FiltersPanel`. Built.
- **Must:** local 2, server 3. Server: with no assignees, filtering by tag is how a team member narrows a shared heap
  to their part; search is the fallback.

#### M-view-settings: View settings

- **Trigger:** View button. **Contents:** Open, In progress, Blocked, each
  Pinned / Shown / Hidden (defaults: open shown, in progress pinned, blocked hidden); "Shown blocked": In their own
  group / Mixed in (only when blocked is shown); Reset to the default view.
- **Presentation:** as M-filters.
- **Freq 2.** **Source:** Req List items §2–3, TD Search §3; `ViewPanel`. Built.
- **Must:** local 2, server 2. The way to show blocked items is also P-hidden-count's job; pinning and mixing are
  rarely changed.

#### P-hidden-count: Hidden items note

- "3 blocked items are hidden. Show them" under the list, from `hidden_count` ("3 items are hidden" when states
  other than blocked are hidden too). Show them puts every hidden state back in the list, at its default (Blocked shown). Shows whenever a
  blocked item exists under the default view.
- **Freq 3.** Built.
- **Must:** local 4, server 4. Blocked items are hidden by default; without the note they vanish, and unblocking
  (L2.7) has no visible way in. M-view-settings is the long way round.

#### P-refresh: Refresh order

- "Refresh order" re-ranks at the current time (drops `as_of`). The list otherwise keeps its order for the session,
  apart from edited items, which move at once.
- **Freq 2.** **Source:** Req Priority §6, TD Session-stable order §6. Built.
- **Must:** local 2, server 2. Reloading the page does the same.

#### P-saved-views: Saved views bar

- **Shows:** chips, Default first, then the heap's views by name; the open one filled, with a dot when the
  list differs from it. Scrolls sideways on a phone.
- **Actions by state:**
  - Unchanged saved view open: Rename, Archive (M-view-archive), and a collapsed "History of {name}"
    (P-view-history).
  - Changed (P-view-changed): the dot's menu, with Save to {name}, Save as a new view… and Undo changes.
- **Who:** every member sees and edits views (`create` on the list; `edit`, `archive` on each).
- **Freq 4.** **Source:** Req Heaps §4, TD API saved views; `SavedViewsBar.tsx`. Built.
- **Must:** local 2, server 2. Filters and search rebuild any view by hand.

##### P-view-changed, P-view-name, M-view-archive, P-view-history

| ID | Purpose | Trigger | Contents and actions | Freq | Must (local) | Must (server) | Presentation |
| --- | --- | --- | --- | :---: | :---: | :---: | --- |
| P-view-changed | Keep or drop a change to the list | The open chip, once it shows the dot | Save to {name} (not for Default); Save as a new view…; Undo changes | 2 | 2 | 2 | Menu under the chip |
| P-view-name | Name a new view, or rename one | Save as a new view…, Rename | Name field (100 chars), Save view or Rename, Cancel; Escape cancels | 2 | 2 | 2 | Inline row |
| M-view-archive | Confirm archiving a shared view | Archive | "Archive {name} for everyone in the team?" Archive, Cancel. No unarchive | 1 | 1 | 1 | Inline confirm replacing the buttons |
| P-view-history | Who changed the view | "History of {name}" | P-item-history's layout | 1 | 1 | 1 | Collapsed disclosure |

#### E-heap-empty, E-no-match, E-bad-view

| ID | When | Shows | Freq | Must (local) | Must (server) | Status |
| --- | --- | --- | :---: | :---: | :---: | --- |
| E-heap-empty | No open items, nothing hidden, no filter or search | "Nothing here yet. Add the first thing on your plate." | 1 | 3 | 3 | built |
| E-no-match | A filter, search or view leaves nothing | "No items match." | 2 | 2 | 2 | built |
| E-bad-view | The server refuses the URL's view (`400`/`422`, such as a tag from a bookmark that's gone) | Saved views and toolbar stay; "This view can't be shown: {message}" and Reset to the default view | 1 | 2 | 2 | built |

A view URL naming an archived saved view silently drops `view` (TD API saved views). Built.

### M-item: Item pane, panel or sheet

- **Route:** `/heaps/$heapId/items/$itemId` today; `/teams/{team}/heaps/{heap}/items/{key}` designed. The list stays
  mounted underneath.
- **Purpose:** see and edit every field of an item, change its state, pick a blocker, see why it ranks where it
  does, and read its history (Req Views: Edit item). **Who:** every member; read-only when the heap is archived or
  the item is closed (only state, via reopen).
- **Header:** "#3 · Up next" (position and group, when the item is in the list on screen); ↑ ↓ to open the row above
  or below without closing the pane (wide and tablet); the key (`k3x9a`) as the copy-link button; ✕.
- **Body, top to bottom:**
  1. Title (inline, saves on blur; Enter saves, Escape reverts). Max 300 characters.
  2. State: the current state, then the likeliest next one as the main button (Start for open, Done for in
     progress, Unblock for blocked), the other states beside it, and Won't do under ⋯. For a closed item: "Done ·
     yesterday" and Reopen, or "It can't be reopened." when the week is over.
  3. When blocked: P-blocker.
  4. "Why it's #3": each contributor ("Overdue by 2 days") with a bar for its `share_percent`.
  5. Need: Idea, Nice to have, Expected, Critical, with a line saying what the picked one means; it can't be cleared.
     Effort and Impact: S M L XL buttons; pressing the picked one clears it.
  6. P-due.
  7. P-tag-picker.
  8. Parameters: P-param-picker for each parameter in use; archived parameters and archived options a closed item
     keeps show as read-only text ("Customer (archived) Enterprise (archived)").
  9. Description (up to 20,000 characters; plain text).
  10. Error line (E-conflict and the other item errors).
  11. P-item-history.
- **Saving:** each change saves as it's made, one at a time, each with the version the last one returned.
- **Closing:** ✕, Escape, Back, a tap on the dimmed list (phone), dragging the sheet down 120 px (phone), dragging
  the panel right (tablet). Returns to where it was opened from. Opening another row replaces it in history.
- **Presentation:** wide: a 26 rem pane beside the list, which widens the shell. Tablet: a 400 px panel over the
  right of the list, the list still live beside it. Phone: a bottom sheet up to 88% of the height, with a grab
  handle, above the on-screen keyboard.
- **Phone:** the state buttons in a bar at the bottom of the sheet (app-ux.md The item §2); due, tags, parameters
  and the description as rows that open sheets.
- **Designed, not built:** the phone's "Added" row among the fields; until then "Added by" shows beside History
  there too.
- **Freq 5.** **Source:** app-ux.md The item; Req Views: Edit item; `ItemPane.tsx`. Built, including "Added by Sam 4
  days ago" beside History.
- **Must:** local 5, server 5. The only place to re-size, re-date, block or unblock an item and see why it ranks where
  it does. Rated as one role: the wide pane, tablet panel and phone sheet are presentations of it.

#### Sub-panels of M-item

| ID | Purpose | Trigger | Contents and actions | Freq | Must (local) | Must (server) | Presentation |
| --- | --- | --- | --- | :---: | :---: | :---: | --- |
| P-blocker | Pick the item this one waits for (Req Items §8) | State is Blocked | "Waiting for" select: Nothing picked, then the heap's other non-closed items; a closed blocker still shows, as "(done)". Errors `blocking_cycle` ("That item already waits for this one…"), `blocker_closed` | 3 | 3 | 3 | Native select in the pane; a searchable list would suit long heaps (Gaps) |
| P-due | Pick a due time (app-ux.md Due) | Always shown | The server's rough choices as pills (In a few days, Next week, In a few weeks, Next month, In a few months), then Exact date, which opens a date field and "An exact date can become overdue." Pressing the picked pill clears it. In the Add dialog on a tablet or wide screen, a picked "None" comes first; the phone's Add chips have none. Beside the label, how the saved one reads now ("Overdue · 12 Mar") | 4 | 5 | 5 | Inline pills; the date field is native |
| P-tag-picker | Add and remove tags | Always shown | Chips with ×; a combobox "Type to add or create" with up to 6 matches and "Create tag “x”"; Enter picks, Backspace removes the last. Errors `tag_name_taken` | 4 | 2 | 3 | Inline, with a dropdown list under the field |
| P-param-picker | Set a parameter value | Each parameter in use | Up to 4 short options (12 chars) as buttons, like effort; more as a menu with "Not set". Violet label | 3 | 2 | 2 | Buttons, or a native select |
| P-item-history | What changed and who | Always at the bottom | Entries newest first: "You edited item" or "{name} …", a time label, and each change as "Field: ~~before~~ → after"; Load more; "No history you can see." | 3 | 1 | 2 | Inline section |
| E-conflict | Someone else saved first (`version_conflict`) | A save | "Someone else changed this item meanwhile. It now shows their version." The pane re-renders from the server | 1 | 1 | 4 | Inline alert |
| E-item-missing | Unknown item, or one in a heap the user can't see (both `404`) | Opening a link | "This item doesn't exist, or it's in a heap you can't see." in the pane | 1 | 2 | 3 | Pane body |

Why the 4s and 5s:

- **P-due** (local 5, server 5): Re-dating is in the daily loop, and nothing else sets a due time, one of the
  ranking's three inputs.
- **E-conflict** (local 1, server 4): Server: two people editing one item is normal in a shared heap; without it a
  save fails unexplained and the pane shows stale data. Local: one user, one tab.

Other item errors shown inline: `item_closed` ("This item is closed, so only its state can change."),
`reopen_window_expired`, `item_not_blocked`, `option_not_in_parameter` ("That option was archived meanwhile.").

### M-add-item: Add an item

- **Trigger:** "Add an item" above the list, or `N`; + in the phone's tabs. **Who:** members of an unarchived heap.
- **Purpose:** the fastest path in the app (Req Views: Add item).
- **Contents:** "Add to \<heap\> ▾", which chooses among the unarchived heaps (tags and parameter values clear);
  the "Added “X” at #4 in Up next" line with Open it, after an add with Add another; title (focused, "What needs
  doing?"), Description ("Optional"), Need (required; Add stays off until it's picked), Effort, Impact, P-due
  (starting at None), P-tag-picker, and the parameters linked to the chosen tags, which appear as tags are added,
  each with "Asked because of the tag x" (Req Custom parameters §3). It starts from the list's filters: their tags,
  and effort, impact or a parameter's option where they pick one value. Footer: Add another (checkbox, remembered
  on the device), Cancel, Add.
- **Actions:** Add (or Enter in the title, or `⌘↵`): without Add another the dialog closes and the toast says where
  the item landed, with Open it; with it, the dialog says so and goes back to its starting point, keeping anything
  changed while it was being added. The position is the one in the list on screen, fetched again after the add; an
  item the list doesn't show (another heap, or hidden by the view or the search) gets "Added “X” to \<heap\>". A
  failed add says "“X” wasn't added: …" in the dialog, or in a toast once the dialog is closed.
- **Presentation:** a centred modal near the top, 600 px wide, labels beside the fields, on tablet and wide. On a
  phone a bottom sheet above the keyboard: the heap and the "added at #4" note on one line, the title, then chips
  that scroll sideways for need, effort and impact (each a sheet of S M L XL), tags (+ Tag opens the picker in a
  sheet) and due (no None: a second tap clears); More fields shows the description and parameters.
- **Freq 5.** **Source:** app-ux.md Adding an item; `AddItem.tsx`. Built.
- **Must:** local 5, server 5. The only way to get work into a heap.

#### M-add-discard: Discard this item?

- **Trigger:** Escape, Cancel or a tap outside with a title typed. **Contents:** "Discard this item?" Keep editing
  (focused), Discard. Replaces the footer in place.
- **Freq 2.** Built.
- **Must:** local 2, server 2.

### S-closed: Closed items

- **Route:** `/heaps/$heapId/closed`; designed `/teams/{team}/heaps/{heap}/closed`.
- **Purpose:** done and won't-do items, most recently closed first (Req Views: Closed items).
- **Shows:** an intro line on the week limit, then sections Today, This week and Earlier (the item's
  `closed_group`). Rows have a state mark on wide screens, the title (opens M-item), "Done · today by you" or
  "Won't do · 3 Nov by Sam" (no "by" for an item whose closer isn't known), and Reopen while the item is
  inside its week (`reopen` in `allowed_actions`), 44 px on a phone. No tag chips. Load more (cursor paging). Empty:
  "Nothing closed yet."
- **Freq 3.** **Source:** `ClosedPage.tsx`. Built.
- **Must:** local 4, server 4. The only route back to a closed item once the toast is gone, and the weekly review's
  screen.
- Opening a closed item shows M-item over the Open list, not over Closed; closing it goes back to Closed. No search
  or filters here.

#### P-reopen: Reopen

- A button on the closed row, or an open state in M-item. No confirmation; reopening is itself reversible. Error
  `reopen_window_expired`.
- **Freq 2.** Built.
- **Must:** local 4, server 4. The only fix for a mistaken Done or Won't do after the toast. M-item's state control
  does the same for an opened item; rated as one role.

### S-heap-activity: Heap activity

- **Route:** `/heaps/$heapId/activity`. **Purpose:** everything that happened in the heap that the user may see,
  newest first (Off Audit §4). Members see item, tag and saved view changes and display names; admins also see
  heap and parameter changes.
- **Shows:** "What changed in {heap}, as far as you can see." and entries like P-item-history, each naming its
  entity ("You edited item **Rotate the certs**"). Load more.
- **Gaps:** entries don't link to the item; no filters by actor, entity or date.
- **Freq 2.** **Source:** TD Audit log §4–6; `ActivityPage.tsx`. Built.
- **Must:** local 1, server 2.

### S-heap-settings: Heap settings

- **Route:** `/teams/{team}/heaps/{heap}/settings` (readable-ids.md URLs). **Who:** admins (`rename`, `archive`,
  `unarchive`, `view_history` on the heap).
- **Purpose:** one place for a heap's configuration. Today rename and archive live in P-heap-row on S-team.
- **Would show:** name; slug (designed: "slug fields on team and heap settings"; a slug change breaks old links,
  which deserves a warning); P-heap-weights; Archive (M-heap-archive) or Unarchive; history.
- **Freq 1.** Designed (route and slug), contents implied. See Gaps for the overlap with S-parameters.
- **Must:** local 1, server 1. Rename and archive live on P-heap-row today.

#### P-heap-weights: Ranking weights

- Age, deadline and value-per-effort weights (`age_weight`, `deadline_weight`, `value_weight`, 0–100, default 10).
  Admins only (`ConfigureHeap`). "Edit the heap's own weights" is the next item in progress.md. No API endpoint yet.
- **Freq 1.** **Source:** Req Custom parameters §2, Off Roles (Configure a heap), progress.md. Designed.
- **Must:** local 2, server 2. Default weights rank well enough; tuning is occasional.

#### M-heap-archive: Archive this heap?

- Archiving makes a whole heap read-only for the team; today it happens on one click. A confirmation ("Archive
  {heap}? It becomes read-only for everyone in {team}. An admin can unarchive it.") fits the other archive actions.
- **Freq 1.** Implied.
- **Must:** local 1, server 1. ux-overview.md prefers Undo to a confirm here.

## Views, filters and tags

The list's view controls (P-search, P-sort, M-filters, M-view-settings, P-saved-views) are described under S-heap,
where they live.

### S-tags: Tags

- **Route:** `/heaps/$heapId/tags`. **Who:** every member can add, rename and describe tags (`create` on the list,
  `edit` on each); read-only in an archived heap.
- **Purpose:** manage the heap's tags (Req Views: Manage tags). Tags can't be deleted.
- **Shows:** "Tags group similar items in {heap}. They can be renamed but not deleted.", P-tag-new, then a row per
  tag. Empty: "No tags yet."
- **Gaps:** no item count per tag, no link to the list filtered by the tag.
- **Freq 2:** most tags are made from P-tag-picker. **Source:** `TagsPage.tsx`. Built.
- **Must:** local 1, server 2. Tags are made in P-tag-picker; this page renames and describes them.

| ID | Purpose | Contents and actions | Freq | Must (local) | Must (server) | Presentation |
| --- | --- | --- | :---: | :---: | :---: | --- |
| P-tag-new | Add a tag | Name (50 chars), Description (optional, 1,000 chars), Add tag. Error `tag_name_taken` | 1 | 1 | 1 | Inline form |
| P-tag-row | Edit a tag | Name and description inputs; Save and Cancel appear once changed. Errors `tag_name_taken`, `version_conflict` ("Someone else changed this tag…") | 2 | 1 | 2 | Inline row |
| P-tag-history | Tag history | Collapsed "History" | 1 | 1 | 1 | Disclosure |

### S-parameters: Parameters

- **Route:** `/heaps/$heapId/parameters`. **Who:** every member sees them (they set values on items); only admins get
  controls (`create` on the list; `edit`, `archive`, `view_history` on each).
- **Purpose:** custom parameters and their ranking weight (Req Views: Manage parameters).
- **Shows:** an explanation ("Parameters rank the items of {heap} next to age, deadline and value…"), P-param-new,
  a card per parameter in use, then P-param-archived. Empty: "No parameters yet."
- **Freq 2** for admins, 1 for members. **Source:** `ParametersPage.tsx`. Built.
- **Must:** local 2, server 2. Custom parameters are optional; effort, impact and due rank without them.

| ID | Purpose | Contents and actions | Freq | Must (local) | Must (server) | Presentation |
| --- | --- | --- | :---: | :---: | :---: | --- |
| P-param-new | Add a parameter | Name (50 chars), Weight (0–100, default 10), Options one per line, highest priority first (up to 50); Add parameter. Error `parameter_name_taken` | 1 | 2 | 2 | Inline card |
| P-param-card | Edit a parameter | Name (Save/Cancel), Weight (Save); options in priority order, each with a label input and ↑ ↓; Save options / Cancel once changed; New option + Add option. Errors `option_label_taken`, `options_mismatch`, `too_many_options`, `parameter_archived` | 1 | 2 | 2 | Card |
| P-option-archive | Archive an option with a replacement | "Open items with {option} switch to [select]" Archive, Cancel. Offered when the parameter has another option and no unsaved changes | 1 | 1 | 1 | Inline confirm in the option row |
| P-param-tags | Link tags, so adding one suggests the parameter | P-tag-picker; "Adding one of these tags to an item suggests this parameter." Members see "Suggested for items tagged …" | 1 | 1 | 1 | Inline |
| P-param-archive | Archive a parameter | "Archive {name}? It stops counting in the ranking; closed items keep their values." Archive, Cancel. Error `parameter_in_use` when an open item has a value | 1 | 1 | 1 | Inline confirm |
| P-param-history | Parameter history (admins) | Collapsed "History" | 1 | 1 | 1 | Disclosure |
| P-param-archived | Archived parameters | "{name}: option, option", history. No unarchive | 1 | 1 | 1 | List at the bottom |

`parameter_in_use` leaves the admin with no way to find which open items hold a value; see Gaps.

## Teams and members

### S-teams: Teams

- **Route:** `/teams`. **Who:** server build; redirects to the one team when the user can't create teams and has one
  (the local build).
- **Shows:** "A team shares its heaps. Admins add members, manage heaps and make others admins.", the user's teams
  with heap count and role, and New team name + Create team, which opens the new team.
- **Designed:** an optional slug on create; derived from the name ("My Team" → `my-team`, `-2` on clashes) when left
  out (readable-ids.md Format §2).
- **Freq 1.** **Source:** Off Teams §1–2; `TeamsPage.tsx`. Built.
- **Must:** local n/a, server 5. Create team is the only way to make a team others can join; personal teams take no
  members.

### S-team: Team

- **Route:** `/teams/$teamId`; designed `/teams/{team}`, with `/teams/{team}/settings` as "a team page".
- **Who:** every member; controls by `allowed_actions` (team: `rename`, `change_slug`, `add_member`, `create_heap`,
  `leave`; member: `promote`, `demote`, `remove`; heap: `rename`, `archive`, `unarchive`, `view_history`).
- **Shows:** the name (P-team-name), "You are an admin of this team." and an Activity link (S-team-activity),
  **Heaps** (P-heap-row each, P-heap-new), **Members** (P-member-row each, P-member-add), P-leave or P-last-admin.
  Empty heaps: "This team has no heaps yet."
- **Local build:** the place to create, rename, archive and unarchive heaps; members section shows the one user
  with nothing to do (local-build.md Single-user mode).
- **Phone:** not yet fitted (app-ux.md Plan step 4).
- **Freq 2.** **Source:** Off Teams, Roles; `TeamPage.tsx`. Built.
- **Must:** local 2, server 5. Server: the only place to add members and heaps, so without it nothing is shared.
  Local: it only holds New heap.

| ID | Purpose | Contents and actions | Freq | Must (local) | Must (server) | Status | Presentation |
| --- | --- | --- | :---: | :---: | :---: | --- | --- |
| P-team-name | Rename the team; change its slug | Name input, Save/Cancel. Designed: slug field, not on a personal team; `invalid_handle`, `handle_taken` | 1 | n/a | 1 | built (slug designed) | Inline |
| P-heap-row | Manage a heap | Name (inline rename), Archived badge, Open, Archive or Unarchive, History | 1 | 2 | 2 | built | Row |
| P-heap-new | Add a heap | New heap name, Add heap. Designed: optional slug | 1 | 2 | 5 | built | Inline form |
| P-member-row | Manage a member | Display name, email, Admin/Member; Make admin, Make member, Remove; History | 1 | n/a | 4 | built | Row |
| M-member-remove | Confirm removal | "Remove {name}?" Remove, Cancel | 1 | n/a | 2 | built | Inline confirm |
| M-self-demote | Confirm demoting yourself | Make member on your own row drops your admin rights at once, with no prompt | 1 | n/a | 2 | implied | Inline confirm |
| P-member-add | Add an existing user (Off Teams §6) | Search "name, or full email" (3+ characters; email shown only when typed in full); results with Add or "Already a member"; "No one matches." | 1 | n/a | 5 | built | Inline search with results list |
| P-leave | Leave the team | Leave team → "Leave {team}? You'll lose access to its heaps." Leave, Cancel. Goes to S-teams | 1 | n/a | 2 | built | Inline confirm |
| P-last-admin | Explain why leave is missing | "You are the only admin, so you can't leave until someone else is one." | 1 | n/a | 2 | built | Text |
| P-personal-team | A personal team's page | No member controls, no Leave, slug fixed, name editable (readable-ids.md Usernames §1–2; `personal_team` error). `personal` flag on `TeamDto` | 1 | n/a | 2 | designed | Variant of S-team |

Why the 4s and 5s:

- **P-heap-new** (local 2, server 5): Server: a new team has no heap and personal heaps can't be shared, so without it
  there's nothing to share.
- **P-member-row** (local n/a, server 4): The only way to make a second admin, or to take away a former teammate's
  access.
- **P-member-add** (local n/a, server 5): The only way anyone joins a team; invitations come later.

### S-team-activity: Team activity

- **Route:** `/teams/$teamId/activity`. **Purpose:** changes to the team, its members and its heaps as wholes
  (created, renamed, archived), not what happens inside heaps. Members see display name changes only; admins see
  everything.
- **Freq 1.** **Source:** TD Audit log §4; `ActivityPage.tsx`. Built.
- **Must:** local 1, server 1.

## Personal settings and account

### S-account: Account

- **Route:** `/account`. **Who:** everyone signed in, and the local user.
- **Shows:** a Profile card with P-display-name and Email (read-only). In the local build, the "Your data" card
  (P-local-export, P-local-import): "Everything is kept in this browser only. Clearing site data, or a browser that
  removes it, loses it. Download a copy now and then, and open it here or in another browser to carry on." Download
  a copy (primary) and Open a copy… (secondary), then "2 heaps · 41 items · last downloaded 26 Oct" (or "never
  downloaded").
- **Designed:** Username, shown read-only (readable-ids.md: usernames never change); P-tokens.
- **Freq 1.** **Source:** TD API Me; `AccountPage.tsx`. Built.
- **Must:** local 4, server 1. Local: it holds export and import, the local build's only data safety. Server: display
  name only.

| ID | Purpose | Contents and actions | Freq | Must (local) | Must (server) | Status | Presentation |
| --- | --- | --- | :---: | :---: | :---: | --- | --- |
| P-display-name | Change the name teammates see (audited in each team) | Input, Save/Cancel | 1 | 1 | 1 | built | Inline |
| P-tokens | See and revoke CLI and other client tokens (`GET /me/tokens`, `POST /me/tokens/{id}/revoke`) | A row per client: name, created, last used; Revoke (confirm) | 1 | n/a | 2 | designed | List |

Not designed anywhere: changing the password or email, password reset, signing out other sessions, deleting the
account (nothing is deleted, but a user may want to leave). See Gaps.

## Admin and server settings

There are none. The server is configured by environment variables (`DATABASE_URL`, `BASE_URL`,
`ALLOW_REGISTRATION`, `SESSION_TTL_DAYS`, OIDC providers; TD Configuration §1) and has no instance-admin role. Every
"admin" in the app is a team admin, and their screens are S-team, S-parameters and S-heap-settings. `/healthz` and
`/readyz` aren't pages.

## Local build

The local build runs the same app with no accounts (local-build.md). What differs:

| Area | Server build | Local build |
| --- | --- | --- |
| S-sign-in, S-register | Shown | Never shown |
| Sign out | In the shell and More | Hidden (`GET /me` `allowed_actions` is empty) |
| S-teams | List and Create team | Redirects to the one team ("My heaps", slug `me`) |
| Shell "Teams" link | "Teams" | The team's name |
| S-team | Members and team actions by role | Heaps only; only `create_heap` on the team |
| G-storage-note, "Your data" on S-account, S-tab-lock | Absent | Present |
| First heap | "Personal", in the user's personal team | "Personal" in team `me` (local-build.md still says "My heap"; the code says "Personal") |
| URLs | `/teams/…` | Under the Pages base path, `/upheap/teams/me/heaps/personal/items/k3x9a` |

### S-tab-lock: Open in another tab

- **Shows instead of the app:** "Upheap is open in another tab. Your heaps live in one place in this browser, so
  Upheap runs in one tab at a time. Close the other tab, and Upheap opens here." Opens by itself once the other tab
  closes.
- **Freq 1.** **Source:** local-build.md Keeping the data §3; `LocalDatabaseGate`. Built.
- **Must:** local 4, server n/a. A second tab can't open the database; without this page it fails with no reason
  given, and second tabs are common.

### P-local-export: Download a copy

- Download a copy, on the Your data card, saves `upheap-YYYY-MM-DD.sqlite3` and records the time in this
  browser for G-storage-note and the card's "last downloaded".
- **Freq 2.** **Source:** local-build.md Keeping the data §2; `LocalDataSection`. Built.
- **Must:** local 4, server n/a. The only copy outside the browser, which can clear its storage.

### P-local-import and M-local-import-confirm: Open a copy

- Open a copy… picks a file, then M-local-import-confirm: a modal alert dialog on a wide screen and a sheet on a
  phone. "Replace everything here with {file}?" and "The copy has 2 heaps and 38 items, from 1 Nov. What's in this
  browser now (41 items) is replaced and can't be brought back." The worker counts the file first, without storing
  it ("Reading the copy…" meanwhile). Cancel, Download current first (downloads and stays open, then says
  "Downloaded"), and Replace in the danger colour. On success the page reloads from the top. Escape or Cancel close
  it and focus goes back to Open a copy…. A file that isn't an Upheap database is refused in the confirm, with only
  Cancel; one from a newer version is refused on Replace, with the data left as it was.
- **Freq 1.** Built.
- **Must:** local 4, server n/a, for both. Without it a downloaded copy can't be restored, so export protects nothing.
  The confirm: an import replaces everything and can't be undone; this is its only guard.

### S-backend-failed: Local backend didn't start

- If the worker or SQLite-wasm fails (OPFS unavailable, a private window, a failed migration), every request fails
  like a network error and each page shows its own error. A full-page "Upheap can't open its storage in this
  browser" with the reason would be clearer.
- **Freq 1.** Implied by local-build.md Approach §4.
- **Must:** local 3, server n/a. Each page already shows an error; one page saying why (a private window, no OPFS)
  saves a confused first open.

### M-pwa-install and M-pwa-update (later)

- Install where the browser offers it, Share → Add to Home Screen instructions on iOS, and an update prompt since a
  new version waits for a reload (app-ux.md Later: an installable app). Out of scope for now.
- **Freq 1.** Designed (later).
- **Must:** local 1, server n/a, for both.

## Errors and system states

| ID | When | Shows today | Freq | Must (local) | Must (server) | Status |
| --- | --- | --- | :---: | :---: | :---: | --- |
| E-loading | Any query in flight | "Loading…" centred on the page or in the pane; "Loading history…" in histories | 4 | 2 | 3 | built |
| E-load-error | A page's main query fails (including `404` for a heap the user can't see) | The error message centred in red | 1 | 3 | 3 | built |
| S-not-found | No route matches | The router's default "Not Found"; no app page | 1 | 2 | 2 | implied |
| E-forbidden | `403`: a member tries an admin action | Controls are hidden from `allowed_actions`, so only a race (demoted meanwhile) shows it, as an inline error | 1 | n/a | 2 | implied |
| E-conflict | `409` `version_conflict` | Inline message; the view re-renders from the server | 1 | 1 | 4 | built |
| G-offline | Network gone, `503` | Per-page errors after two retries | 1 | n/a | 3 | implied |
| — | `500` | The message carries only a request ID (engineering-principles.md Errors §4); nothing presents it for a bug report | 1 | 1 | 2 | implied |
| G-session-expired | `401` | Redirect to S-sign-in | 2 | n/a | 4 | built |

E-conflict and G-session-expired (server 4) are justified in their own entries above.

## Gaps and open questions

### Use cases with no screen

1. **Heap weights.** Admins should edit age, deadline and value weights (Req Custom parameters §2, Off Roles).
   Neither the API nor the UI has them, and no doc says whether they belong on S-parameters (beside the parameter
   weights they compete with) or on S-heap-settings.
2. **Heap settings page.** readable-ids.md gives heaps a `settings` route and slugs, while heap rename and archive
   live on S-team today. The list "(tags, parameters, activity, closed…)" in its URL table reads as either sub-pages
   of the heap or the contents of one settings page. Decide which.
3. **Password and account.** No password change, reset ("forgot password"), email change, or "sign out everywhere".
   The server has no email sending, so reset needs an admin path or a design.
4. **CLI tokens.** `/me/tokens` and P-tokens are designed but have no layout, and the OAuth consent step is
   undecided (S-oauth-authorize).
5. **Invitations.** Planned for later; S-invite and an Invite panel on S-team are placeholders.
6. **Item key and Copy link.** Settled: the key is the copy-link button in the item's header.
7. **Finding what blocks archiving a parameter.** `parameter_in_use` doesn't say which open items hold a value. A
   link to the list filtered by that parameter's options would answer it.
8. **New heap from the switcher.** Settled: the switcher ends with "New heap in \<team\>" for the heap's own team.
9. **Where an added item landed.** M-add-item closes without saying the new item's position; the CLI does.
10. **Onboarding.** No first-run explanation of how ranking works, what effort, impact and loose due times mean, or
    that items are never deleted. Only E-heap-empty's one line.
11. **Help for keys** (M-shortcuts) and a **404 page** (S-not-found).
12. **Bulk actions.** React Aria `GridList` was chosen partly for selection (app-ux.md Toolkit §2), but no
    multi-select or bulk state change is designed.
13. **Notifications and due reminders.** None designed; teammates' changes only appear on refetch. Server push
    (SSE) is mentioned as a later replacement for polling.
14. **Archived saved views** have no place of their own; their history is only on S-heap-activity. There's no
    unarchive, by design.
15. **Blocked on a closed blocker.** "Closing a blocker later doesn't unblock what waits for it" (TD Cycle
    detection §3). Nothing nudges the user to move the blocked item on.
16. ~~**Return to page after sign-in.**~~ Settled: a signed-out visit and an expired session both return to their
    page after sign-in (S-sign-in, From a link; G-session-expired).

### Conflicts between docs, or between docs and code

1. **Inline edits in the list.** Req Views: List items §7 says title, effort, impact, due and state are edited in the
   list. app-ux.md moved editing into the item; rows only change state. requirements.md should follow.
2. **Phone item state controls.** app-ux.md The item §2 and Tablet §3 call for Start, Blocked and Done buttons on a
   phone and tablet; the code shows the main-state buttons at every width.
3. **Add item return.** Req Add item §4: "saving takes the user back… ready to add another item." The dialog closes
   unless Add another is ticked, and that box starts unticked.
4. **Personal team.** offering.md Teams §3 says a personal heap's team "works like any other team"; readable-ids.md
   makes it private (no members, no leaving, fixed slug). readable-ids.md lists this as a doc fix to make.
5. **Local build names.** local-build.md: user "Me", team "My heaps", heap "My heap". readable-ids.md and
   `service/src/personal.rs`: team slug `me`, first heap "Personal". local-build.md is stale.
6. **More menu.** app-ux.md lists Tags, Parameters, Team, Account, Sign out; the code adds Teams (or the team's
   name).
7. ~~**Sign-up on this branch.**~~ Settled: S-register sends the username.
8. **"Removable from items in the list"** (progress.md Tags) predates the compact rows; tags are now removed in
   M-item only.

### Undecided

1. **Confirmations.** Archiving a heap and demoting yourself happen on one click, while archiving a view, option or
   parameter, removing a member and leaving all confirm inline. Pick one rule (proposal: confirm anything that
   affects the whole team or the user's own access; undo for the rest).
2. **Inline confirms vs dialogs.** Every confirmation today replaces its button in place. On a phone a bottom-sheet
   confirmation may read better; app-ux.md doesn't say.
3. **Pages not yet fitted to phone width** (app-ux.md Plan step 4): S-heap-activity, S-tags,
   S-parameters, S-team, S-teams. They still use the older stone palette rather than the design tokens.
   S-account uses the tokens (its Profile and Your data cards), but its phone layout waits on a phone board.
4. **Long blocker lists.** P-blocker is a native select of every open item; at a few thousand items it needs search.
5. **Registration closed.** No endpoint tells the sign-in page that registration is off before the user tries.
6. **Slug changes.** Old links stop working with no redirect (readable-ids.md Format). The slug field should warn.
7. **Throttled sign-in.** Whether to count down `Retry-After` on the page.
