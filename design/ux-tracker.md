# UI and UX work tracker

Tracks the web app's design and build against [ux-overview.md](ux-overview.md) (principles and decisions),
[screens.md](screens.md) (every screen, with frequency and must-have ratings) and the "Upheap layouts" canvas
(the mockups). [app-ux.md](app-ux.md) is the layout spec per width; [progress.md](progress.md) tracks the product
as a whole. Last reviewed 2026-11-07.

Scope now: the **first design and build set** from screens.md, for the browser (1024 px and up) and the phone
(under 640 px). Tablet, keyboard shortcuts and everything else in screens.md's Deferred list come later.

Status: ✅ done · 🟡 partial · ⬜ not started.

- **Design:** ✅ mocked up and agreed · 🟡 mocked up, waiting for review, or missing a state · ⬜ no mockup.
- **Build:** ✅ the app matches the design · 🟡 built, but differs from the design · ⬜ not built.

## First set

| Area | Entries | Must L / S | Boards | Design | Build | What the build has to change |
| --- | --- | :---: | --- | :---: | :---: | --- |
| Shell, browser | G-shell-wide, G-storage-note | 4 / 4 | Main | 🟡 | 🟡 | Heaps grouped by team with open counts; Team, Account, Sign out at the bottom; storage note mentions the last download only when it's over two weeks old or never happened |
| Shell, phone | G-shell-phone, M-more | 4 / 4 | PhoneList, PhoneMore | 🟡 | ✅ | Built: tabs Open, Closed, +, Activity, More, with + opening Add from any of the heap's pages; More as a card of 52 px rows above the tabs, with the team's name, the team count and the username as notes. Left: the member count beside Team, which needs it from the server |
| Heap switcher | M-heap-switcher | 2 / 4 | PhoneSwitcher | 🟡 | 🟡 | Built: a sheet of 52 px rows by team (a popover beside the rail on a tablet), a check on the current heap, "New heap in \<team\>" when the team allows it, which asks for a name and opens the new heap. Left: open counts, which need the server |
| The list | S-heap, P-row, P-search, P-hidden-count, P-saved-views | 5 / 5 | Main, PhoneList | 🟡 | ✅ | Built: hover Start and ⋯; filters as removable chips with "6 of 14" (the total is a second list request while filtering, until the list response carries it); the changed view's dot, with Save and Undo behind the chip; "Show them" |
| Undo toast | G-toast | 3 / 3 | Main, PhoneList | 🟡 | ✅ | Built: "Start next" only when nothing else is in progress. Left: it only sees the items the view shows, so an in-progress item hidden by the view or a filter doesn't stop the offer; the list response should say whether anything in the heap is in progress |
| Item, browser | M-item | 5 / 5 | Main | 🟡 | 🟡 | Built: main button, others, Won't do under ⋯; key as the copy-link button; ↑ ↓; contributor bars. Left: "Added by Sam 4 days ago" beside History, which needs `created_by` and a created label on Item from the server |
| Item, phone | M-item | 5 / 5 | PhoneItem | 🟡 | 🟡 | Built: the state buttons in a bar at the bottom of the sheet, the likeliest next state wider, with Won't do under ⋯ at the end of the bar rather than in the header; the state as a line under the title; due, tags, parameters and description as rows that open sheets. The buttons keep the wide screen's names (Block, not Blocked). Left: the "Added" row, once Item carries `created_by` |
| Due picker | P-due | 5 / 5 | AddDialog, PhoneDue | 🟡 | 🟡 | Phone, in the item: built, a sheet of 52 px rows, Exact date with its date field, and Clear due; a rough choice or Clear closes it. Left: the phone Add dialog still has the pills (step 4.3), and the browser pane's dropdown isn't mocked yet |
| Tag picker | P-tag-picker | 2 / 3 | — | ⬜ | ✅ | The open combobox isn't mocked; today's is fine until then |
| Add an item | M-add-item | 5 / 5 | AddDialog, PhoneAdd | 🟡 | 🟡 | Heap chooser in the title; prefill from the active filter, without a label; "Added at #4 · Open it" (needs the position from the server); Add another remembered per device; due starts at None. Phone: one-tap chips and More fields |
| Conflicts | E-conflict | n/a / 4 | Main | 🟡 | 🟡 | Says what was lost ("your change (due: next week) wasn't saved") with Apply mine again |
| Closed | S-closed, P-reopen | 4 / 4 | WideClosed, PhoneClosed | 🟡 | 🟡 | Grouped by day; one intro line explains the week limit; who closed it; Reopen while allowed, nothing after |
| Your data (local) | S-account, P-local-export, P-local-import, M-local-import-confirm | 4 / n/a | WideAccount | 🟡 | 🟡 | The confirm names the file, compares item counts and offers Download current first. No phone board yet |
| Tab lock (local) | S-tab-lock | 4 / n/a | WideTeams | 🟡 | ✅ | Matches today's page |
| Sign in, create account | S-sign-in, S-register | n/a / 5 | WideSignIn, PhoneSignIn | 🟡 | 🟡 | Errors inline under their field (username taken, password too short); a signed-out link says where it leads and opens the item after sign-in |
| Session expired | G-session-expired | n/a / 4 | — | ⬜ | 🟡 | Signing in again should return to the page the user was on |
| Empty heap, first open | E-heap-empty | 1 / 1 | WideSignIn | 🟡 | 🟡 | Add your first item, and (server) Create a team. Not in the must-have set; drawn because it's the first screen after sign-up |
| Teams | S-teams | n/a / 5 | WideTeams | 🟡 | 🟡 | Create team inline, with name and link name |
| Team | S-team, P-heap-new, P-member-add, P-member-row | n/a / 5 | WideTeam, PhoneTeam | 🟡 | 🟡 | The settings layout; member search with results under it; the remove confirm says what happens; on a phone, member actions in a sheet |
| Loading | E-loading | 2 / 3 | — | ⬜ | 🟡 | Keep the previous view, dimmed. No mockup needed beyond a note |

## Open questions

1. **Main's toolbar** still shows sort, Filters and View, which are Later, so the toolbar is laid out once. Keep
   them, or strip them from the first-set boards?
2. **Frequency ratings** the subagent questioned in screens.md: storage note and tablet rail (5 because always on
   screen), hidden-items note (4, not 3), switching saved views (3, not 4), conflicts in a shared heap (2, not 1).
3. **app-ux.md disagrees with the mockups** on the tablet. The phone tabs and the item's state buttons now follow
   the mockups there.
4. **"Added at #4"** needs the new item's position from the Add response, or a list refetch.
5. **Username suggestions** when the chosen one is taken: worth the server work?
6. **Links:** screens.md, ux-overview.md and this file aren't linked from CLAUDE.md, app-ux.md or progress.md yet.

## Next steps

1. Review the first-set boards and mark each row's Design ✅, or list what to change.
2. Draw the missing states: the open tag picker, the browser due dropdown, Your data on a phone, the phone remove
   confirm.
3. Settle the open questions and bring app-ux.md in line.
4. Build in this order, the core loop first:
   1. ~~The list and item changes.~~ Done, apart from "Added by", which waits on the server. One API change can
      bring "Added by" (`created_by` and a created label on Item), the unfiltered total for "6 of 14", and whether
      anything in the heap is in progress, for "Start next".
   2. ~~The phone tabs, item bar and sheets.~~ Done, apart from open counts in the heap switcher, the member
      count in More and the item's "Added" row, which wait on the server.
   3. Add.
   4. Closed.
   5. Your data.
   6. Sign in and create account.
   7. Team and Teams.
   8. Session expiry return.

## Later

Mocked up already, not in the first set: **WideCommand** (⌘K), **WideFilters** (Filters with counts) and
**MainKeyboard** (the list with shortcut hints). MainKeyboard still shows the row buttons and the toast from
before the "only where it pays" pass, and needs that pass when it moves up.

Not designed: tablet, keyboard shortcuts help, sort and view settings, saved-view editing, tags, parameters,
activity, heap settings, history views, the remaining team and account pages, error pages, and the installable app.
The full list is in screens.md, "Deferred".
