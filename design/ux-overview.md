# UX overview

Draft. The principles behind the web app's screens, and the concrete choices they lead to. It sits on top of
[app-ux.md](app-ux.md) (layout per width) and [screens.md](screens.md) (every screen and overlay, with how often
each is used). The mockups are in the "Upheap layouts" canvas. This covers the web browser (1024 px and up) and the
phone (under 640 px). The tablet keeps app-ux.md's layout and has no mockups here.

## Principles

1. **Consistent look.** One set of tokens: stone neutrals, amber for "in progress" and for the next action, teal
   for done, red only for a firm due date that's overdue, violet only for parameters, grey for tags. A control
   looks the same wherever it appears: S M L XL buttons, the due choices and the tag chips are identical in the item
   pane, the Add dialog and their phone sheets.
2. **Predictable layout.** Every screen keeps the same areas in the same places. Browser: sidebar, then the list,
   then the item pane. Phone: top bar, then search and Filters, then the list, then the bottom tabs, with the + button above
   the tabs. Overlays come in only two kinds: a centred dialog or popover in the browser, a bottom sheet on the
   phone.
3. **At your fingertips.** Whatever is used most (screens.md, ratings 4–5) is visible without opening anything: the
   ranked list, Add, state changes, search. On a phone it sits within thumb reach at the bottom.
4. **One step ahead.** Whatever the user is likely to do next is one click away. Everything they can do with what's
   on screen is at most two obvious clicks away (open, then act). Keyboard shortcuts come later, on top of this,
   not instead of it ([Later: keyboard](#later-keyboard)).
5. **Only where it pays.** Being one step ahead is for the moments when the next step is clear and saves real
   effort, or prevents a loss. Elsewhere the app just does the thing and stays quiet. Each suggestion, extra
   button or explanatory line costs attention on every visit, and a screen full of them reads as noise. Before
   adding one, check three things:
   1. Is the next step clear in most cases, rather than one of several guesses?
   2. Does it save more than one click, or prevent a mistake or a loss of data?
   3. Would the user miss it if it were gone?

   If not, leave it out. When it's worth having, show it only in the state where it applies (a nudge when a backup
   is old, not always), and prefer one quiet option to several.

## What "one step ahead" means, by screen

### The list (S-heap)

1. **The state circle is the one-tap action on every row:** it marks the item done. On hover, an open row adds one
   quiet button, Start, and ⋯ for the rest. Nothing else sits on the rows.
2. **After Done, the toast offers Undo.** Only when nothing else is in progress does it also offer "Start next:
   \<the top open item\>". If something else is already in progress, the user is carrying on with it, and the
   offer would be noise.
3. **Active filters show as removable chips** under the toolbar, so the user always sees why the list is short.
   Changed view settings show as chips too. (A changed saved view's dot, with "Save to \<view\>" and "Undo
   changes", is not shown for now: the web app hides saved views.)
4. **The hidden-items note offers the fix** ("3 blocked items are hidden. Show them").

### The item (M-item)

1. **The main button is the likeliest next state:** Start for open, Done for in progress, Unblock for blocked. The
   other states are next to it, and Won't do is under ⋯.
2. **In the browser, ↑ ↓ in the pane header walk the list** without closing the pane, so triage is open, change,
   next. On a phone the sheet closes and the next row is one tap away, so the sheet has no Next button.
3. **A link sign beside the item's title copies its link**, and the key (`k3x9a`) shows in the pane header. Nothing
   else has Copy link. This answers screens.md gap 6.
4. **"Why it's #3"** shows the contributors with their `share_percent` as bars, and nothing more.
5. **The item says when it was created and by whom** ("Added by Sam 4 days ago"), next to a History link.
6. **A blocked item whose blocker has closed** shows "Waiting for X (done 2 days ago)" with an Unblock button
   (gap 15).

### Adding (M-add-item)

1. **Prefilled from context, silently:** the heap you're on, and the active filter's tags, and its effort, impact
   and parameter options where it picks one value. No label explains the prefill, and the tag picker's own
   suggestions are enough. The heap can be changed from the dialog title.
2. **It says where the item landed** ("Added at #4 in Up next. Open it"), inline when Add another is on and in the
   toast when it's off (gap 9).
3. **Add another is remembered per device.** This settles the conflict between requirements.md and the code.
4. **Due defaults to None**, with the rough choices one tap away. On a phone's row of due chips nothing is picked
   instead, and a second tap on a chip clears it.

### Search and navigation

1. **Later: ⌘K finds any item by title or key in every heap.** Tab acts on the highlighted item (start, done, due, copy
   link), Enter opens it, and it can also create ("Add 'safari' as a new item") and navigate. Searching every heap
   needs a server search endpoint; today's search filters only the loaded list.
2. **The search box filters the current list.** Matches stay in ranked order.
3. **The heap switcher shows open counts** and ends with New heap (gap 8). In the browser it opens from the heap
   button at the top of the sidebar, and Open shows the heap's open count.

### Filters and views (M-filters)

1. **One Filters panel** sets both what the list shows (each state pinned, shown or hidden) and the filters (tags,
   need, effort, impact, parameters); the phone's also has the sort. The footer shows "4 of 14 items". The list
   updates live behind the popover, or behind the phone sheet, whose button reads "Show 4 items".
2. **"Save as view…" is not shown for now**, as the web app hides saved views; when they return, it goes in the
   panel, where the user finishes shaping the list.

## Rules that settle open questions

1. **Confirmations.** A reversible change acts at once and offers Undo in a toast: state changes, archiving a
   heap, a view or a parameter. A change that can't be undone, or that affects other people, asks first, in a
   dialog in the browser or a sheet on the phone, and names the object: removing a member, giving up your own admin
   role, replacing all local data, and discarding a typed title.
2. **The phone and the browser share components and differ only in presentation.** On the phone, the item sheet's
   state buttons are Start / Blocked / Done in a bar at the bottom of the sheet. In the browser they're the main
   button plus the others, as above.
3. **Every page that edits a list of things** (Tags, Parameters, Members, Heap settings) uses one settings layout:
   title, one-line explanation, a way to add one near the top (a "+ New" button, or a short form when adding takes a
   field or two), rows that turn into a form when edited, and the rarer actions out of the way (a ⋯, or the foot of
   a card).
4. **After the session expires, signing in returns the user to the page they were on** (gap 16).

## What comes first

[screens.md](screens.md) defines the core flows for each build and gives each entry a must-have rating, local and
server, alongside how often it's used. Its "First design and build set" is what's designed and built first: the
shell, the list, the item, Add, Closed, Your data (local), sign-in and create account (server), and Teams and Team
(server). Everything else waits, including keyboard shortcuts, ⌘K, Filters, sort, saved-view editing, tags,
parameters, activity and heap settings. Those keep working as they are built today. The first-set mockups show
no shortcut hints.

The one-step-ahead ideas above apply to the first set as follows:

1. **The phone's Add button sits in the middle of the tabs** (Open, Closed, +, Settings, More), under the thumb, on
   every page that shows the tabs.
2. **The phone item sheet keeps its state buttons in a bar at its bottom** (Start, Block and Done for an open item,
   named as in the browser). The fields scroll above it.
3. **Pickers that are lists** (due, heap) use 52 px rows with a check on the current choice. The heap switcher shows
   open counts and ends with "New heap in \<team\>".
4. **Conflicts say what was lost.** "Sam changed the due date a moment ago. You're seeing their version; your
   change (due: next week) wasn't saved. Apply mine again." The retry is one tap.
5. **Signed-out links say where they lead** ("Sign in to open the link to k3x9a") and open the item after sign-in.
6. **Create account checks the username as it's typed.** It's permanent and is part of every link. *Proposal:*
   suggest free alternatives when it's taken; that needs the server.
7. **After sign-up, the empty heap offers the next two steps:** Add your first item, and Create a team. After a team
   is created, the Team page itself shows New heap and Add member, with no extra prompt.
8. **Removing a member says what happens:** "They lose access to Launch and Support. Their items stay." It's
   confirmed inline in the browser and in the actions sheet on the phone.
9. **Your data (local) nudges before it's too late, and only then.** The storage note is normally just "Stored in
   this browser · Download a copy". It adds when a copy was last downloaded only when that's over two weeks ago, or
   never. The
   replace confirmation names the file, compares the item counts, and offers "Download current first". Remembering
   the last download is per browser; reading the counts means parsing the file before replacing.
10. **Closed explains the week limit once,** in its one-line intro. Rows show Reopen while they can be reopened and
    nothing once they can't, with no per-row dates or "closed for good" labels.

## Mockups

All in the "Upheap layouts" canvas.

| Board | Build | Shows |
| --- | --- | --- |
| Main | both | List, row actions, filter chips, conflict line, smart toast, item pane |
| AddDialog | both | Prefilled Add with "added at #4" and Add another |
| WideClosed | both | Closed by day, Reopen inside the week |
| WideTeam | server | Team in the settings layout: heaps, add member, roles, remove confirm |
| WideTeams | both | Teams with Create team (server); the tab lock page (local) |
| WideSignIn | server | Create account with a field error; the first-open empty heap |
| WideAccount | local | Your data, with the replace confirmation |
| PhoneList, PhoneItem, PhoneAdd | both | The list, the item sheet, Add above the keyboard |
| PhoneDue, PhoneSwitcher, PhoneMore | both | The due, heap and More sheets |
| PhoneClosed | both | Closed with 44 px Reopen |
| PhoneTeam, PhoneSignIn | server | Team with the member actions sheet; sign in from a link |
| WideCommand, WideFilters | both | Later: ⌘K and Filters, not in the first set |
| MainKeyboard | both | Later: the list with shortcut hints and the hint bar |

## Later: keyboard

The browser gets shortcuts once the first set works without them, shown in the MainKeyboard board:

1. Hints on the controls they trigger: `N` on Add item, `/` in the search box, `S` `B` `X` on a row's Start, Block
   and Done, `G O` and `G C` beside Open and Closed, `⌘↵` on Add in the Add dialog.
2. ↑ ↓ to move between rows and Enter to open; ↑ ↓ in the item pane walk the list.
3. A hint bar under the list with the keys for what's on screen, and `?` for all of them (M-shortcuts).
4. ⌘K in the sidebar ([Search and navigation](#search-and-navigation)).

The keys that are already built (`N`, `/`, ↑ ↓, Enter, `x`, `⌘↵`) keep working. They just aren't shown or designed
for in the first set.

## Needs from the server

1. ~~The new item's position in the Add response, or one list refetch, for "Added at #4".~~ Settled: the list on
   screen is fetched again after an add, and the position is read from it.
2. Later: item search across heaps for ⌘K.
3. Later, optional: username suggestions.
