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
| Shell, browser | G-shell-wide, G-storage-note | 4 / 4 | Main | 🟡 | 🟡 | Heaps grouped by team with open counts; Team and Account at the bottom, Sign out on Account. Built: the storage note, "Stored only in this browser · Download a copy", mentions the last download only when it's over two weeks old or never happened |
| Shell, phone | G-shell-phone, M-more | 4 / 4 | PhoneList, PhoneMore | 🟡 | ✅ | Built: tabs Open, Closed, +, Activity, More, with + opening Add from any of the heap's pages; More as a card of 52 px rows above the tabs, with the team's name, the team count and the username as notes. Left: the member count beside Team, which the server now sends (`member_count` on `GET /me`'s teams) |
| Heap switcher | M-heap-switcher | 2 / 4 | PhoneSwitcher | 🟡 | 🟡 | Built: a sheet of 52 px rows by team (a popover beside the rail on a tablet), a check on the current heap, "New heap in \<team\>" when the team allows it, which asks for a name and opens the new heap. Left: open counts, which the server now sends (`open_count` on `GET /me`'s heaps) |
| The list | S-heap, P-row, P-search, P-hidden-count, P-saved-views | 5 / 5 | Main, PhoneList | 🟡 | ✅ | Built: hover Start and ⋯; filters as removable chips with "6 of 14" (the total is the list response's `unfiltered_count`); the changed view's dot, with Save and Undo behind the chip; "Show them" |
| Undo toast | G-toast | 3 / 3 | Main, PhoneList | 🟡 | ✅ | Built: "Start next" only when nothing else in the heap is in progress, counting items the view or a filter hides (the list response's `in_progress_count`) |
| Item, browser | M-item | 5 / 5 | Main | 🟡 | ✅ | Built: main button, others, Won't do under ⋯; the key in the header and a link sign beside the title that copies the link; ↑ ↓; contributor bars; "Added by Sam 4 days ago" beside History, from the item's `created_by` and `created_label` ("by you" for the viewer's own, as History says "You") |
| Item, phone | M-item | 5 / 5 | PhoneItem | 🟡 | 🟡 | Built: the state buttons in a bar at the bottom of the sheet, the likeliest next state wider, with Won't do under ⋯ at the end of the bar rather than in the header; the state as a line under the title; due, tags, parameters and description as rows that open sheets. The buttons keep the wide screen's names (Block, not Blocked). Left: the "Added" row among the fields; until then "Added by" shows beside History |
| Due picker | P-due | 5 / 5 | AddDialog, PhoneDue | 🟡 | 🟡 | Phone, in the item: built, a sheet of 52 px rows, Exact date with its date field, and Clear due; a rough choice or Clear closes it. Phone, in Add: a row of chips that scrolls sideways, the rough choices then Exact date, a second tap clearing the picked one. Add on a wide screen starts at None. Left: the browser pane's dropdown isn't mocked yet |
| Tag picker | P-tag-picker | 2 / 3 | — | ⬜ | ✅ | The open combobox isn't mocked; today's is fine until then |
| Add an item | M-add-item | 5 / 5 | AddDialog, PhoneAdd | 🟡 | ✅ | Built: "Add to \<heap\> ▾" chooses the heap (tags and parameter values clear, the rest stays); the list's filters prefill tags, and effort, impact or a parameter where they pick one value, silently, also after each Add another; "Added “X” at #4 in Up next" with Open it, in the dialog with Add another and in the toast without, the position read from the list refetched after the add; Add another remembered per device; due starts at None. Phone: a sheet with chips for need, effort, impact, tags and due, and More fields for the description and parameters. Need stays, as its own row of chips, though the mockups predate it. Left: on a phone, a parameter that a newly added tag asks for sits under More fields, closed, unless the prefill opened it; a badge on More fields, or opening it, would show it |
| Conflicts | E-conflict | n/a / 4 | Main | 🟡 | 🟡 | Says what was lost ("your change (due: next week) wasn't saved") with Apply mine again |
| Closed | S-closed, P-reopen | 4 / 4 | WideClosed, PhoneClosed | 🟡 | ✅ | Built: sections Today, This week and Earlier from each item's `closed_group`, one section per run across loaded pages; one intro line on the week limit, shorter on a phone; "Done · today by you" from `closed_by` ("you" as in Added by), with no "by" when the closer isn't known; a state mark on wide screens; Reopen only while allowed, 44 px on a phone; no tag chips. Left: the "team · heap" line over the title, which no other page has yet and the closed response can't fill (it has no team name); "by" in the item's state line |
| Your data (local) | S-account, P-local-export, P-local-import, M-local-import-confirm | 4 / n/a | WideAccount | 🟡 | ✅ | Built: the Your data card with Download a copy, Open a copy… and "2 heaps · 41 items · last downloaded 26 Oct"; the confirm names the file, says what the copy holds and from when, compares item counts and offers Download current first; a file that isn't an Upheap database is refused in the confirm. The worker counts the file in memory (`count`); the last download is kept in localStorage. No phone board yet: on a phone the confirm is a sheet like the others, with full-width 44 px buttons |
| Tab lock (local) | S-tab-lock | 4 / n/a | WideTeams | 🟡 | ✅ | Matches today's page |
| Sign in, create account | S-sign-in, S-register | n/a / 5 | WideSignIn, PhoneSignIn | 🟡 | ✅ | Built: a 380 px card on a wide screen, the bare form on a phone, with a Sign in / Create account switch; a problem whose `field` names an input shows under it (username taken, email taken, password too short, an empty email or password), with focus on it and announced, and any other (wrong password, too many attempts, registration closed) above the button; a signed-out link goes to `/login?redirect=…`, says where it leads ("Sign in to open the link to 3n2ay.", or "…that page.") and opens it after sign-in. The tagline is left out, as on the boards. "Email or username", as the server signs in by either. Left: nothing |
| Session expired | G-session-expired | n/a / 4 | — | ⬜ | ✅ | Signing in again should return to the page the user was on. Built with sign-in: a `401` mid-session, from a read or a write, replaces the page with `/login?redirect=…`, as a signed-out visit does, with the same notice ("Sign in to open that page."), and signing in returns there, with the last user's cached data dropped. The row asks for nothing more; a "your session ended" line would be new |
| Empty heap, first open | E-heap-empty | 1 / 1 | WideSignIn | 🟡 | 🟡 | Add your first item, and (server) Create a team. Not in the must-have set; drawn because it's the first screen after sign-up |
| Teams | S-teams | n/a / 5 | WideTeams | 🟡 | ✅ | Built: + Create team opens a form with Name and an optional Link name (the server makes one from the name when it's left out, so the note shows `/teams/…` only once one is typed); the server's errors (`handle_taken`, `invalid_handle` with its hint, `field_empty`) show under the field they name, with focus on it; Escape or Cancel closes it. Rows: "Admin · 4 members · 2 heaps", "Only you · 1 heap" for the personal team, and Open; 44 px on a phone. The local build still goes straight to its one team. Left: nothing |
| Team | S-team, P-heap-new, P-member-add, P-member-row | n/a / 5 | WideTeam, PhoneTeam | 🟡 | ✅ | Built: "Teams · you're an admin" over the name, with Rename, Link name and Activity under a ⋯ by it (edited in place when chosen); Heaps with + New heap and rows with "14 open" (`open_count`), Archived, Open and a ⋯ for Rename, Link name, Archive or Unarchive and History; Members · 4 with the search above the list, its results a listbox under it (the first that can be added highlighted with "↵ Add as member", arrows move, Enter or a click adds, "Already a member", "No one matches."); rows with an initials avatar, "(you)", @username, the role badge, Make admin or Make member, Remove, and a quiet History button; the remove confirm under the list, "Remove Sam Ortiz from Acme? They lose access to Launch and Support. Their items stay.", with focus back on Remove after Escape or Cancel; Leave team restyled. On a phone: 52 px heap rows that open the heap, with a 44 px ⋯ opening a sheet; the search 48 px tall; 56 px member rows whose ⋯ opens a sheet headed "Sam Ortiz · @sam · Member" that asks before removing. Left: a back button on the phone (the shell has no pattern for one yet; the phone keeps its top bar and the "Teams" link); M-self-demote; optional link name for a new heap |
| Loading | E-loading | 2 / 3 | — | ⬜ | 🟡 | Keep the previous view, dimmed. No mockup needed beyond a note |

## Open questions

1. **Main's toolbar** still shows sort, Filters and View, which are Later, so the toolbar is laid out once. Keep
   them, or strip them from the first-set boards?
2. **Frequency ratings** the subagent questioned in screens.md: storage note and tablet rail (5 because always on
   screen), hidden-items note (4, not 3), switching saved views (3, not 4), conflicts in a shared heap (2, not 1).
3. **app-ux.md disagrees with the mockups** on the tablet. The phone tabs and the item's state buttons now follow
   the mockups there.
4. ~~**"Added at #4"** needs the new item's position from the Add response, or a list refetch.~~ Settled: the list
   on screen is fetched again after an add anyway, and the position is read from it. An item added to another heap,
   or one the view hides, gets "Added to \<heap\>" without a position.
5. **Username suggestions** when the chosen one is taken: worth the server work?
6. **Who can add to which heap:** the heap chooser in Add offers every unarchived heap, as every role can add
   items today. If a role that can't add ever appears, the server should say so per heap (as `allowed_actions`), not
   the client.
7. **Links:** screens.md, ux-overview.md and this file aren't linked from CLAUDE.md, app-ux.md or progress.md yet.

## Next steps

1. Review the first-set boards and mark each row's Design ✅, or list what to change.
2. Draw the missing states: the open tag picker, the browser due dropdown, Your data on a phone, the phone remove
   confirm.
3. Settle the open questions and bring app-ux.md in line.
4. Build in this order, the core loop first:
   1. ~~The list and item changes.~~ Done. One API change brought what they waited on: `created_by` and
      `created_label` on Item for "Added by", and `unfiltered_count` (the "14" in "6 of 14", so the client makes no
      second list request) and `in_progress_count` (for "Start next") on the ranked list. A count rather than a
      yes or no, because the item just marked done may be the one in progress.
   2. ~~The phone tabs, item bar and sheets.~~ Done, apart from open counts in the heap switcher and the member
      count in More, which wait on the server, and the "Added" row among the phone's fields.
   3. ~~Add.~~ Done, with no server change: the position comes from the list refetched after the add.
   4. ~~Closed.~~ Done. One API change: `closed_by` and `closed_group` on Item, and a nullable `items.closed_by`
      column, filled in for closed items from the audit log. Items with no matching entry show no "by".
   5. ~~Your data.~~ Done, with no API change: the worker's `count` message reads a copy before it replaces
      anything, and the last download is kept in this browser.
   6. ~~Sign in and create account.~~ Done. Two API changes: `email_taken` names `field` `email`, as `handle_taken`
      names `username`; and sign-in answers an empty email or password with `422` `field_empty` naming the field,
      before counting the attempt, where an empty email used to get `401` `invalid_credentials`.
   7. ~~Team and Teams.~~ Done. One API change: `open_count` on `Heap` and on each heap `GET /me` lists (as
      `HeapOverview`, so `HeapSummary` in the list responses stays as it was), counted as the list's
      `unfiltered_count` with the default view, in one grouped query; and `member_count` on `GET /me`'s teams. The
      sidebar, heap switcher and More can now show open and member counts too.
   8. ~~Session expiry return.~~ Done with step 6: the `401` handler sends the user to sign-in with the way back,
      as the route guard does. Nothing is left unless a "your session ended" line is wanted, which the row doesn't
      ask for.

## Later

Mocked up already, not in the first set: **WideCommand** (⌘K), **WideFilters** (Filters with counts) and
**MainKeyboard** (the list with shortcut hints). MainKeyboard still shows the row buttons and the toast from
before the "only where it pays" pass, and needs that pass when it moves up.

Not designed: tablet, keyboard shortcuts help, sort and view settings, saved-view editing, tags, parameters,
activity, heap settings, history views, the remaining team and account pages, error pages, and the installable app.
The full list is in screens.md, "Deferred".

