# Web app UX

How the web app looks and behaves, at every width and in both builds: the principles, the core flows, the toolkit,
the layout per width, each screen, the rules that settle questions, and what comes later. The mockups live outside
the repository, in the "Upheap layouts" canvas; a board is named where this doc relies on one.

The app is one React app whose layout changes with the width: **phone** under 640 px, **tablet** 640 to 1023 px,
**wide** 1024 px and up. It never ranks, checks permissions or works out labels; the server does
([architecture.md](architecture.md#principles)), and the client adds layout, gestures and animation.

## Principles

1. **Consistent look.** One set of tokens: stone neutrals, amber for "in progress" and for the next action, teal
   for done, red only for a firm due date that's overdue, violet only for parameters, grey for tags. A control
   looks the same wherever it appears: S M L XL buttons, the due choices and the tag chips are identical in the item
   pane, the Add dialog and their phone sheets.
2. **Predictable layout.** Every screen keeps the same areas in the same places. Wide: sidebar, then the list,
   then the item pane. Phone: top bar, then search and Filters, then the list, then the bottom tabs with + in the
   middle. Overlays come in only two kinds: a centred dialog or popover on a tablet or wide screen, a bottom sheet
   on a phone.
3. **At your fingertips.** Whatever is used several times a session is visible without opening anything: the ranked
   list, Add, state changes, search. On a phone it sits within thumb reach at the bottom.
4. **One step ahead.** Whatever the user is likely to do next is one click away. Everything they can do with what's
   on screen is at most two obvious clicks away (open, then act). Keyboard shortcuts come later, on top of this,
   not instead of it ([Later: keyboard](#later-keyboard)).
5. **Only where it pays.** Being one step ahead is for the moments when the next step is clear and saves real
   effort, or prevents a loss. Elsewhere the app just does the thing and stays quiet. Each suggestion, extra
   button or explanatory line costs attention on every visit, and a screen full of them reads as noise. Before
   adding one, check three things: is the next step clear in most cases, rather than one of several guesses; does
   it save more than one click, or prevent a mistake or a loss of data; would the user miss it if it were gone? If
   not, leave it out. When it's worth having, show it only in the state where it applies (a nudge when a backup is
   old, not always), and prefer one quiet option to several.

## Core flows

The jobs the app must support. **Loop** flows happen many times a day, **setup** flows once or rarely,
**recovery** flows after a mistake or a failure.

### A team

1. **Sign up and sign in** (setup). Create account with a name, username, email and password lands on the empty
   personal heap. Later visits sign in; when the session ends, sign-in brings the user back to the page they were on
   (recovery).
2. **Getting into a shared heap** (setup). Someone creates a team on Teams and becomes its admin; the Team page
   opens. The admin adds a heap, adds each teammate (who must have an account; invitations come later), and makes a
   second admin so the team doesn't hang on one person. A teammate finds the heap in the sidebar or the heap
   switcher.
3. **Daily loop** (loop). Pick the heap. The list shows in-progress items pinned on top, then Up next. Start one with
   its row's play button, `s`, a swipe left or Start in the item; finish it with its row's circle, a swipe right, or
   Done in the item (its main button once started, else under ⋯); each change gets an undo toast. Add what came up with the Add button, `N`, or + on a phone.
   Re-size or re-date in the item and watch it move. Block an item, naming what it waits for; it leaves the list,
   since blocked items are hidden by default, and "Show them" brings them back to unblock. Find an item with search. In a shared heap,
   teammates' changes arrive as the list refetches, and rows animate to their new places.
4. **Weekly review** (weekly). Closed: reopen what was closed by mistake, within its week, which is also the fix for
   a missed Undo (recovery). Back in the list, close what won't happen as Won't do and re-size the rest. A reload
   re-ranks at the current time.
5. **Coordinating.** See what others changed in the refreshed list, an item's history or the heap's Activity.
   Saving over a teammate's change shows a conflict and their version; redo the change if it still applies
   (recovery). Point someone at an item with Copy link: it opens the item, after sign-in if needed, or says the item
   can't be seen.
6. **Admin basics** (setup, occasional). On the Team page: add members, make or unmake admins, remove someone; add,
   rename and archive heaps, with the archived banner to unarchive; custom parameters in the heap's Settings; leave,
   or read why the only admin can't.

## Toolkit

1. **Tailwind v4**, with the design tokens in `@theme` (surface, ink, muted, accent, state colours, radii), so light
   and dark modes come from one place instead of `dark:` pairs on every element. Which one shows is the Account
   page's Appearance setting: System (the default) follows the device, and Light or Dark overrides it. It's kept per
   browser in localStorage, not on the server, as it's about the screen rather than the account, and applied by an
   inline script in `index.html` before the first paint, so a reload doesn't flash the other theme.
2. **React Aria Components** for behaviour: dialogs and sheets, menus, radio and toggle groups, date picker, tag
   group, and the list itself (`GridList`, for keyboard navigation and selection). It is unstyled, so it takes our
   Tailwind classes, and it is one dependency rather than copied code. It handles touch properly: a scroll doesn't
   fire a press, focus stays sane when the on-screen keyboard opens, and every control is accessible
   ([decisions.md](decisions.md#24-react-aria-components-rather-than-a-copied-component-library)).
3. **Motion** for sheet drag, swipe rows and the list's reorder animation.
4. **lucide-react** for icons.

## Layout

Breakpoints are Tailwind's: `sm` 640 px and `lg` 1024 px.

| Width                | Navigation                                                                                                                                                                                                                | List                                                     | Item                                                  |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------- |
| Phone, under 640 px  | Heap name at the top, with its team ("Product · Acme") unless the heap is personal, switches heap; bottom tabs Open, Waiting, + (Add), Closed, More                                                                       | Full width, compact rows                                 | Bottom sheet over the list                            |
| Tablet, 640–1023 px  | An 80 px rail: Waiting, the heap button, then the views as icons with labels, the heap's team, Teams and Account at the bottom                                                                                            | Beside the rail, with the wide screen's toolbar and rows | Panel over the right of the list, the list still live |
| Wide, 1024 px and up | Sidebar: Waiting for me with its count and a thin rule under it, a heap button ("Product · Acme") that opens the heap switcher, then the heap's views with Open's count, the heap's team, Teams and Account at the bottom | Middle column                                            | Pane beside the list while an item is selected        |

**More** on a phone holds Settings, Activity, Team, Teams and Account, as a card of 52 px rows above the tabs. Account is the last entry there and in the sidebar, and Sign out is on the Account page, under
its cards, which are Profile, Password, Appearance and Command line. The **+** in the middle of the tabs opens the Add dialog from any of the heap's pages, under the thumb, so
the Add button beside the heap name shows only from a tablet up.

The sidebar and the rail list the heap's views (Open, Closed, Activity, Settings) and, below them at the bottom,
the team in the URL (a link to its page, shown on the heap's pages and the team's own), Teams and Account. The heap's title names its team too ("Product ·
Acme"), on the phone's top bar as well, except for a heap in the personal team.

A phone's tabs have room for four: Waiting sits left of the + and Closed right of it, and Settings and Activity go
into More with the account pages.

**Waiting for me** is the first entry of the sidebar and the rail, above the heap switcher: an inbox icon, "Waiting for
me" in the sidebar with its count on the right, "Waiting" in the rail and the tabs with the count as a small badge on
the icon. There is no count at zero, and the entry stays. The count is the number of open questions across the
person's heaps, fetched with the page, kept fresh for 30 seconds and fetched again on focus and after a reply or an
item's change; nothing polls.

Pages with no heap in their address keep the heap opened last in the shell, so going back is one tap: the switcher
shows it, the heap's views link to it with none marked active, the tabs show on a phone, and Add adds to it. For now
that is `/waiting`; Teams and Account keep their shell without one. The heap kept is the one opened last that is still
there and active, else the first active heap, else the first.

On a wide screen the sidebar and the content are one centred block, at most the sidebar plus the content column
wide, so the menu stays next to what it opens; the sidebar's colour runs on to the left edge. The item pane widens
the block when it opens.

### Heap settings

**Settings** is one page for a heap's configuration, with a tab each for **General**, **Tags**, **Parameters** and **Ranking**.
Each tab is a link with its own path (`/settings`, `/settings/tags`, `/settings/parameters`, `/settings/ranking`), so Back steps between
them and a tab can be linked to; the old `/tags` and `/parameters` paths lead to their tabs. General holds the name
and the link name, edited in place, a note that a new link name breaks old links, the heap's history, a Context
card, and Archive heap or Unarchive heap. While the link name is being changed, the new path shows under it ("New
link: /teams/{team}/heaps/{slug}"). Each control shows only where `allowed_actions` allows it; everyone else reads
the values.

The **Context** card shows the heap's context as written, with its line breaks and no rendering, or a muted line
saying what a context holds when there is none. Any member sees an Edit button (not on an archived heap), which
turns the card into a form: a textarea that grows with its text, Save and Cancel. A failed save keeps the form and
the draft open and says why; Escape does nothing, so a long draft is never lost to a stray key. When someone else
saved a different context meanwhile, it shows above the kept draft under "Saved meanwhile", and Save again replaces
it; when only the heap's other settings changed, the message says so and Save again saves the draft. Save and
Cancel give focus back to Edit.

The **Agents** card, under Context, has a switch, "Agents prepare new notes", with the line "Each note added here is
sent to an agent to fill in its context, then comes back to you." It saves as it is turned, and can't be turned again until the save ends. Only an admin sees the
switch (`set_agents_prepare`); others, and everyone in an archived heap, read "Agents prepare new notes: on" or
"off".

The tabs share one look: an intro line, then cards in the design tokens. **Tags** has a New tag card (Name,
Description, Add tag) and a card headed "{n} tags" whose rows read as the tag's name in a chip and its description
(or "No description"); an edit button on the row turns it into the form (name, description, Save, Cancel, History),
highlighted in the accent. **Parameters** opens its add form from a New parameter button, and gives each parameter a
card: the name with Rename, the weight, the options as a numbered list (↑ ↓ and Archive on each; Archive turns the
row into a red confirm that asks where open items go), Add option, the tags that suggest it, and a foot with History
and Archive parameter. Archived parameters are listed last. On a phone everything stacks in one column, main
buttons span the width, and every control is 44 px to a finger.

**Ranking** (the canvas boards "Wide · Ranking" and "Phone · Ranking") sets how much each factor counts. A Built in
card has a row each for need, age, deadline and value per effort, and a Parameters card, shown when the heap has
parameters in use, has one per parameter, with its options as the description. A row shows the factor's name, its
share of the total as a bar and a percentage (plain arithmetic on the weights on screen; "–" when all are 0), and for
editors a slider and a number input that move together. One Save weights writes every changed weight at once; a "was
{n}" chip and an "{n} unsaved" count mark what changed, Cancel (or Escape) puts the weights back, and Reset all to 10
sets every draft to the default without saving. Others read "Weight {n}" and a note that only admins can change it. A
**Top of the list** card, for everyone, shows the first five items as the draft weights would rank them, each with how
far it moves (↑2, ↓1, New), so the effect shows before saving. Wide: a sticky save bar under the cards and the preview
beside them. Phone: one column, the preview under the cards, the save bar fixed above the bottom tabs.

### Tablet

A tablet gets its own layout rather than a stretched phone or a squeezed wide screen:

1. **The rail.** The sidebar folds into an 80 px rail. Its top button shows the heap's initial and opens the heap
   list as a popover; below it are the heap's views (Open, Closed, Activity, Settings) as icons with short labels;
   the account sits at the bottom.
2. **The list** takes the rest of the width, with the wide screen's toolbar (search, Sort by, Filters) and rows at
   touch sizes (44 px targets).
3. **The item** opens as a 400 px panel over the right of the list. The list beside it stays live: tapping another
   row switches the panel, and the selected row stays highlighted. Swiping the panel right or tapping ✕ closes it.
   It uses the wide screen's layout, with popovers, and the touch cues of The item §16 (▾ always shown, 44 px targets).
4. Swipe actions on rows work here too, as on a phone.

### Phone details

Phone pages respect the safe-area insets (`viewport-fit=cover`) under the bottom tabs and sheets, and inputs are at
least 16 px so iOS doesn't zoom in on focus. Sheets sit above the on-screen keyboard: Android Chrome resizes the
page for it (`interactive-widget=resizes-content`), and on iOS, which doesn't, the sheets are lifted by the height
the keyboard covers, from the visual viewport. The heap switcher and More open as sheets too; More's sits above the
tabs.

Pickers that are lists (heaps, due choices, a parameter's options) are 52 px rows with a check on the current one.
The heap switcher groups heaps by team, with archived ones folded per team behind "N archived". From 8 unarchived
heaps it adds a search field that filters as you type, fold headers for the teams, and Recent: the last 4 heaps
opened in this browser, not the one on screen, as chips on a phone. Recent and the folds are kept in localStorage,
since a "last opened" on the server would cost a write on every heap visit;
they don't follow the user to another device. Search gives a flat list with the team and an
"archived" tag, a "New heap “…”" row, and a message when nothing matches. Arrows move, Enter opens and Escape
clears the field, then closes. It ends with "New heap in \<team\>", in the heap's own team, when the team allows
it. On a tablet it is a popover beside the rail.

Gestures are offered where the pointer is coarse (a finger), on phones and tablets alike:

1. A row dragged right past about 100 px slides away and marks the item done; dragged left, it stays open on
   buttons, Agent (when the row's robot would show, The list point 3), In progress and Blocked (Open in place of the
   state it's in), until one is picked or the row is tapped. Each is followed by a toast with Undo. A tap just after a drag doesn't open the item.
2. The item sheet drags down by its top bar to close; the tablet panel drags right. Letting go short of 120 px
   springs it back.

## The list

1. A row is one or two lines: three buttons (point 3), the title, the due chip and tags, and below them the server's
   contributors ("Due this week · small effort"), then, at the end, the kind and stage icons. A row shows no position
   and no sizes: the order is the list's, and the contributors say why. A task has a checklist icon, and a reminder a
   calendar-clock icon, in a fixed slot at the end of the row (a note leaves the slot empty, so the icons line up),
   and an item with context or defined has a page or a clipboard icon in the slot after it. The row shows no words
   for them; a screen reader hears them after the title ("Rotate the certs, task, defined"). A task with steps shows
   how many are ticked, small and muted, after the title (`2/5`), and a screen reader hears ", 2 of 5 steps ticked"
   after the title. Dropped steps don't count, and a task without an open or ticked step shows nothing.
2. Custom parameters with a value show as violet "Name: Option" chips ("Energy: Low") after the tags on a wide
   screen, and on their own line under the contributors on a phone or tablet. Unset parameters don't show. Violet is
   kept for parameters everywhere, so they read apart from tags (grey) and due dates.
3. A row starts with three fixed slots, from a tablet up, so titles line up; a slot the person can't use stays
   empty. First the **state button**: Start (a play circle) on an open row, Unblock (a red closed padlock in a
   circle, drawn open while the row is hovered or focused, and always with a finger) on a blocked row, Pause (an
   accent circle, lightly filled, with two bars) on an in-progress row. Start moves the item to in progress; Unblock
   and Pause move it to open. A screen reader hears ", blocked" after a blocked row's title. Second **Done**, a circle
   with a check, on every row. Third the **robot**: a button that sends the item to an agent, one step at once, with
   no line, when the server allows it and no agent has the item; a filled robot mark in ink when an agent has
   it, with a small accent dot when it waits for the person; empty otherwise (a reminder, a blocked item, a reader).
   With a mouse, Done and the robot are muted until the row is hovered or has focus within; with a finger they are
   always in full colour. A quick send shows the toast "Sent to an agent, going to: with context." with Undo (The item
   point 15), gives focus to the row, and a failed one says why in a toast. A phone shows the state button only (44 px)
   and swipes instead (point 5). Without permission to change the state, the buttons are absent and the slots stay
   empty. An undo toast follows every state change from the list: bottom centre, above the tabs on a phone, gone after
   a few seconds. Changes made in the item save silently and have no undo. The toast sits over the open item, so Sent
   to an agent can offer Undo from inside a phone's sheet; the Add dialog and the other sheets stay above it.
4. Tapping or clicking a row opens the item. Keyboard on a wide screen: up and down move, Enter opens, `s` starts
   (an open or blocked item), `x` marks done and `a` sends one step to an agent, where the robot button shows; `N`
   and `/` stay. Tab goes the state button, Done, the robot, then the row's link, then its tags and parameter chips.
5. On a touch screen, swiping a row right marks it done; swiping left offers Agent (where the robot button would
   show, sending one step at once as the button does), In progress and Blocked.
6. Saved views are hidden in the web app for now, and `view=` in the URL is ignored: a views bar duplicated the
   Filters panel, which has no place for saving one yet. The API keeps them, for the CLI's `--view`. When they
   return, "Save as view…" goes in the Filters panel.
7. One Filters panel holds what the list shows and its filters: Show (Open, In progress, Blocked as Pinned, Shown or
   Hidden, a dot on a changed one; shown blocked items in their own group or mixed in; Reminders to come, Hidden or
   Shown, point 16), then Narrow down (tags, need,
   effort, impact, parameters), with Reset all, "6 of 14 items" and Done. It's a popover from a tablet up and a sheet
   on a phone. The toolbar is search, Sort by and Filters; on a phone it's search and an icon Filters button, and the
   sheet, "Filters & sort", starts with the sort and ends with "Show 6 items". The Filters button counts the filters
   and changed settings, amber while any are on; on a phone, where the sort is out of sight in the sheet, it counts a
   sort other than Priority too ("Filters and sort, 1 on"). The list updates live behind the panel.
8. Under the toolbar a chip takes off each filter ("Need: Critical") or changed setting ("Blocked: shown"), with
   "6 of 14" while filters are on and Reset all. Reset all keeps the sort and the search.
9. Rows have no hover actions besides the colour of Done and the robot (point 3). The other states (Blocked, Won't
   do) are in the item.
10. After Done, the toast offers Undo. Only when nothing else in the heap is in progress does it also offer "Start
    next: \<the top open item\>"; if something else is already in progress, the user is carrying on with it, and the
    offer would be noise.
11. Active filters and changed view settings show as the removable chips of point 8, so the user always sees why the
    list is short.
12. The hidden-items note offers the fix: "3 blocked items are hidden. Show them."
13. An open item with an open question says **waits for you** at the end of its row, small, semibold and in the accent,
    before the kind and stage icons. On a phone, where the end of the row is tight, a small accent dot before the
    title stands for it. A screen reader hears ", waits for you" after the title on every width. The closed list never
    shows it.
14. An open item an agent has shows the robot mark in its slot (point 3), with a dot while it waits for the person: an
    item that waits for the person is not the agent's, and says so in words (point 13). On a phone a small robot mark
    before the title stands for it. A screen reader hears ", with an agent" after the title. Who has an item never
    changes its place. The closed list never shows it.
15. Moving an item an agent has to a state that takes it back, from the list, asks first, naming the item: "An
    agent has “Rotate the certs”. Marking it done takes it back from the agent." The state button, Done, a
    swipe, `x` and `s` go through the one ask, which is a dialog on a wide screen and a sheet
    on a phone, with "Take back and mark done" or "Take back and start", and Cancel. Cancel
    puts a swiped row back and gives focus to the row. Blocked and Open don't ask. The change's Undo toast
    restores the state only: the turn stays taken back.
16. A reminder before its date is not in the ranked list, and nothing counts it. **Reminders to come** in Filters ›
    Show (Hidden by default, a chip when Shown) adds a group of that name under the list, after Blocked: a row has
    the reminder mark, the title, the date it joins ("12 March") and the tags, and no state button or swipe.
    They are in date order, soonest first, and the filters narrow them like the rest. A row opens the item, where
    the date can be changed. With no reminders to come the group isn't there. While text is in the search box the
    page asks for them whatever the setting says, so the search finds a reminder with a wrong date; they show in
    that group, and clearing the search puts the list back as it was.

17. A tag in a row filters the list. Clicking it adds the tag to the tag filter (items with any of its tags), and
    clicking it again takes it off; the usual chip under the toolbar follows. While a tag is in the filter, its chips
    in every row are outlined in the accent. A parameter chip ("Energy: Low") does the same with its option: it joins
    the parameter filter, and is outlined in violet while it's on. Tags and parameters on the item, Waiting for me and
    the closed list are plain text, and so is a chip of an archived parameter.
18. Sort by offers Priority, Due soonest, Most needed, Smallest effort and Largest impact, then each custom parameter
    that has an order. Most needed lists Critical first, then Expected, Nice to have and Idea; items of the same
    need keep priority order.

## The item

1. The item route (`/teams/$team/heaps/$heap/items/$item`) shows the list with the item over it or beside it,
   instead of a separate page. Links and the back button keep working: going back closes the sheet.
2. It opens to read, in this order: the title with the main state button and ⋯ beside it (point 6), a status line, Waits for
   you when something does (point 12), the fields (point 16), the context (point 17), the comments (point 18), a task's Steps (point 11),
   Questions (points 13 and 14) and History, collapsed until it's clicked, on every item it opens. On a phone the
   main button and ⋯ sit in a bar at the bottom of the sheet instead, the main one wider. The **status line** is one
   quiet line under the title that may wrap: the kind and stage with their icons ("Task · With context"; the kind
   alone when the item is closed), a reminder's date ("Reminder · 12 March"), the state ("In progress" in the
   accent), and for a closed item when it closed, or "It can't be reopened." A blocked item says "Blocked by" and
   links to the item it waits for, which opens in the pane, or just "Blocked" while none is named. Then the chips:
   the item's place in the list, "#3" (point 9), and who has the item when an agent does, or the button that sends it to
   one when none does (point 15). A
   reminder still ahead has a line under it, "Joins the list on 12 March". The header bar names the section the item
   is in ("Up next") and has no rank.
3. Each parameter is picked like effort and impact: a row of option buttons when it has a few options (up to four),
   and a menu row when it has more or long labels. A parameter linked to one of the item's tags shows up as soon as
   the tag is added, in Add item too.
4. Changes save as they are made, each with its own `If-Match`. They go one at a time, each with the version the
   previous one brought back, so quick changes don't conflict with each other.
5. Closing it (✕, Escape, or Back) goes back where it was opened from: Back when the app opened it, the list when it
   was opened from a link. Opening another row while one is open replaces it in the history.
6. The main button is the likeliest next state: Start for open, Done for in progress, Unblock for blocked, Make it a task
   for a reminder that has arrived (its state's own choice then moves into ⋯), Reopen for a closed item while it can
   be. A reader, an archived heap or a closed item that can't be reopened has no main button. ⋯ is named More, is
   always there, and holds, each only when the server allows it and in this order: the other state changes, Won't do,
   Make it a task…, Change blocker… on a blocked item, Copy link, and the Stage choices.
   Block saves the state at once; the blocker is named after, with Change blocker…, which opens the list of items in
   a popover (a sheet on a phone) and saves on a pick. Copy link confirms with a toast, "Link copied", as the menu has
   closed by then. Make it a task asks first, naming the item, since a task can't go back to a note (or to a
   reminder). A reminder has an exact Date in place of Due (point 16), which can be moved, also back into the future,
   and not cleared; it has no steps and no Send to an agent button. Stage lists as written and with context, and defined on
   a task only (a note says "Defined needs a task"); picking one saves silently, like every change in the item.
7. On a wide screen and a tablet, ↑ ↓ in the pane header walk the list without closing the pane, so triage is open,
   change, next. On a phone the sheet closes and the next row is one tap away, so the sheet has no Next button.
8. The key (`k3x9a`) shows in the header, and Copy link is in ⋯. Nothing else has Copy link.
9. The rank chip, "#3" in the status line, is a button that opens "Why it's #3": a popover (a sheet on a phone) with
   the contributors and their `share_percent` as bars. Below them, "Held back by" lists what keeps the item from
   ranking higher: a low need, no due date, unset sizes, in that order, as lines of text without bars. What can fix
   it shows itself on the fields (point 16): an unset effort, impact or due date it names is marked there. A low
   need isn't marked: the need was set on purpose. The item shown first in its list has no held-back lines, nor does
   a blocker ranked by what it blocks, nor any item while the list is sorted by something other than priority; with
   nothing to say the chip is plain text, and an item that isn't in the list on screen has no chip. The list row
   doesn't show any of this.
10. The item says when it was created and by whom ("Added by Sam 4 days ago"), on the line of History's toggle.
11. **Steps**, on a task, sit inline at every width (on a phone they are not a row that opens a sheet) under the
    heading "Steps 2/5", with the count of point 1 (just "Steps" when there is none). Each step is a row: a
    checkbox that ticks and unticks it, the title edited in place (Enter or leaving the field saves, Escape or a
    blank title puts it back), a chevron that shows its context (a plain-text field, saved on leaving it, closed to
    begin with) and ⋯ with Drop, or Restore on a dropped step. A dropped step is struck through and muted, with a
    dash in place of its checkbox, and stays in the list. "Add a step" opens a one-line field in place: Enter adds
    the step and keeps the field for the next, leaving it with text adds the step and closes it, Escape closes it
    without adding and puts the focus back on the button. Nothing is optimistic: a change shows once the server has
    it, and one that fails is said under the section. A closed item, an archived heap or a missing `edit` leaves the
    steps and the count to read, with no checkbox to change, no ⋯ and no Add.
12. **Waits for you**, while the item is open and has open questions or a stuck agent turn, is the summary at the top of
    the item, under the status line, a tinted, outlined block (the accent at a low fill, a 2 px accent border) like the
    open-questions box in Questions, the same on
    the pane, the tablet panel and the phone sheet. A stuck turn is its first row (see Waiting for me, 7), with Retry
    and Take back. When questions wait, one line says "1 question waits for you" or "N questions wait for you", with
    **Answer ↓** on its right: it scrolls the first open question into view and focuses its first control. The reply
    controls are not here but in Questions (point 13).
13. **Questions** is a section under the Steps, shown when the item has any question, under the heading "Questions".
    The open questions come first, in the same tinted, outlined box, one row each in number order, separated by a thin
    rule: a quiet line with the kind ("Question", "Request", "Question · pick any") and `· step "<title>"` when it
    names one, the short text in semibold, "Details ▸" when it has details (opening them in place), then the reply
    controls. A question without choices has an "Answer" field and Reply, off until the answer has text. With choices
    it has radios (one) or checkboxes (any), "Comment, optional" and Reply, off until something is picked. A request
    has "Comment, optional" and two buttons, Done and Can't be done, which reply as soon as they are pressed, sharing
    one row on a phone. On a phone Reply spans the width, and every control is 44 px. After a reply there is no toast:
    the row leaves the box and joins the fold (point 14), the focus goes to the next row's first control, else to the
    item, and a polite live region says Replied to “<text>”. While it is sending, that row's buttons are off. If it
    fails the row stays with its picks and comment and the reason shows under it. If someone else replied first, the
    row leaves and the section says "Someone else replied to “…” meanwhile." until the next reply. In an archived
    heap a row shows its text and details with no controls. A closed item has no open rows: closing withdrew its open
    questions, which read in the fold as "Withdrawn".
14. The fold under the open questions, **N answered**, is closed to begin with and lists the answered questions and the
    withdrawn ones in number order: the kind line, the short text, Details ▸, then `→ <picked labels>. <comment as
written>` and "<who>, <when>" in quiet text, or a muted "Withdrawn". It reads "N closed" when none is answered
    and all are withdrawn. It only reads; changing a reply is later.
15. **Who has it** is a chip in the status line, shown only while an agent has the item: "Agent · to with context" (or "Agent ·
    all the way" for a turn sent all the way, whose goal is only its first stop), "Agent · waiting for you", or "Agent stuck" in the amber tone. It opens a popover (a sheet on a phone) titled "Who has it"
    with "With an agent since 14:02" (the time today, else the date as the server words it: "yesterday", "3 Nov"),
    "going to: <state>" and ", all the way" for that reach, the person's line in quotes, and "Prepared on add" for an
    item the heap's setting sent. If the item waits for the person it reads "An agent is waiting for your answer,
    going to: …", and if its turn is stuck, "An agent is stuck, going to: …." with no buttons: Retry and Take back are
    in Waits for you. **Take back** is in the popover, and asks first, "Take this back from the agent? It stops, and
    what it changed stays.", inline in the popover and in a sheet on a phone; afterwards the chip is gone and the
    focus goes to ⋯, with no toast. With no agent there is no chip and nothing says "With you". Before an agent has the
    item, the same place holds the **Send to an agent** button (a robot and those words, in the chip's style), shown
    when the server allows it: not on a reminder, a closed item, in an archived heap, or on a **blocked item**, which
    can't be sent until it is unblocked (the server refuses it: "This item is blocked. Unblock it before sending it to
    an agent."). It opens a form: a popover under the button on a wide screen and a tablet, a sheet on
    a phone. The form has two choices, "To <where it goes>" (the next stable state: with context, defined or done)
    and "All the way", or on a defined task the one line "To done"; "A line for the agent (optional)"; Send and
    Cancel. Cancel returns focus to the button. Send acts at once, closes the form, gives focus to the chip that
    replaces the button and shows a toast, "Sent to an agent, going to: with context.", with Undo, which takes it back
    without asking or a second toast. The list row's robot sends the same way but for one step with no line, and shows the same toast. A failed send or take back
    says why in the popover, and the form keeps its line. Someone else's send first says "Someone already sent this
    to an agent." and the chip shows it. Start, Done and Won't do in the item ask before they take an item back ("An
    agent has this. Starting it takes it back. Take it back and start it?"), inline under the title or in a sheet;
    Blocked and Open don't.
16. **Fields** are labelled values that open their editor on a click. Need, Effort and Impact share a row of three
    columns, each a small label over its value; Due (a reminder's Date), Tags and each parameter follow, one row
    each, with the label on the left. A parameter's label is violet. An unset effort, impact or due date reads "Set
    effort", "Set impact", "Set due" (a parameter "Set <name>") in muted text; when the server's held-back lines
    (point 9) name it, it is marked with an amber pill. With a mouse a value has no border at rest, a tint and ▾ when
    pointed at or focused; on a touch screen the ▾ always shows and the value is 44 px tall. A click opens a popover
    under the value on a wide screen and a tablet, a sheet on a phone: the need's four steps, S M L XL (pressing the
    picked one clears it), the due choices (staying open while an exact date is chosen), the date, the tags (staying
    open for several picks) or the parameter's options. A pick saves at once and closes it, and focus goes back to
    the value. Without `edit` (a reader, an archived heap, a closed item) the values are plain text.
17. The **context** is the item's description rendered as Markdown (GitHub-flavoured: headings, emphasis, links, code,
    lists, task lists as disabled checkboxes, quotes, tables), with an Edit button, shown with `edit`. Raw HTML is
    never rendered and images are never loaded: an image shows as a link. An in-page anchor (a footnote's) moves in
    place, every other link opens in a new tab, and an unsafe one (`javascript:`) reads as plain text. A line break
    in the text stays one, so plain text written before reads as it did. With none it says "No context yet." and the
    button reads Add. Edit swaps the text in place for a form with Save and Cancel, and on a phone opens the same form
    in a sheet; it reads and behaves as the heap's Context card does (Heap settings). ⌘ or Ctrl with Enter saves, and
    Escape does nothing, so a long draft is never lost to a stray key or closes the item. Save closes the form once
    the save has landed, the text is read-only while it is in flight, and a failed save keeps the draft and says why.
    When someone else saved a different context meanwhile, it shows above the draft under "Saved meanwhile", Save
    reads Save again and replaces it, and Cancel keeps theirs; until Save again, nothing else saves over theirs (Done
    doesn't go on, and leaving doesn't save the draft). Dismissing the phone's sheet (✕ or a tap outside)
    with a changed draft saves it like Save, and stays open if that doesn't land; an unchanged draft just closes. Done
    and Won't do from the pane save a changed draft first, and don't go on if it can't be saved. Leaving the item with
    a changed draft, by closing it or walking on, saves it when nothing else changed meanwhile, and otherwise doesn't
    save it; either way a toast says the context for that item wasn't saved, and why. When the item can no longer be edited (closed
    elsewhere, an archived heap, a changed role) the draft stays as read-only text that can be selected, with "This item
    can't be edited any more; your text wasn't saved." and a Close button.

18. **Comments n**, under the context at every width and always open (not a disclosure, not a sheet row), before
    the Steps, lists the item's comments oldest first with a thin rule between them. Each is a quiet
    line, "<name> · <time>" (the time today, else the date as the server words it), and the text under it as plain
    text with its line breaks kept; a comment is never edited or deleted, so a row has no controls. The heading is
    "Comments 3", or just "Comments" when there are none. Under the list, where the server offers `comment` (also on
    a closed item), is the field "Add a comment" that grows with its text, and a Comment button, off until the field
    has text and while it is sending; on a phone the button spans the width and is 44 px, on wider screens it is at
    the right. Nothing is optimistic: the comment shows from the server's answer, the field clears, the focus stays
    in it and a polite live region says "Comment added". If it fails, the field keeps its text and the reason shows
    under it. What is typed lives apart from the item, so another edit saving or a refetch doesn't touch it. Where
    `comment` isn't offered (an archived heap) the comments read with no field, and with none the section is absent.

## Adding an item

1. The Add item button beside the heap name (or `N`) opens the Add dialog: a modal over the list on a wide screen and
   a tablet. On a phone the + in the middle of the tabs opens it as a sheet above the keyboard.
2. The dialog has every field Add item takes: title (focused), context, need, effort, impact, due, tags, and the
   parameters linked to the chosen tags, which appear as soon as a tag is added, each saying which tag asked. Each
   is optional apart from the title and the need: Add stays off until a need is picked. Nothing is picked for you, so
   every need is someone's choice. Under the four steps a line says what the picked one means. On a phone the need,
   effort, impact, tags and due are chips in rows that scroll sideways, effort and impact opening a sheet of sizes;
   the context and the parameters are under More fields.
3. It starts from where it was opened, silently: the heap on screen, and the list's filtered tags, and effort,
   impact or a parameter where the filters pick one value. No label explains the prefill; the tag picker's own
   suggestions are enough. The title, "Add to \<heap\> ▾", chooses another heap; tags and parameter values belong to
   a heap, so they clear, and the rest stays.
4. Add saves and closes (`⌘↵` / `Ctrl↵` on a keyboard), and a toast says where the item landed ("Added “X” at #4 in
   Up next", read from the list as the server ranks it) with Open it. With "Add another" ticked, the dialog stays
   open, says the same in a line of its own, and goes back to how it started, keeping anything changed while it was
   being added. Add another is remembered on the device. Escape, Cancel or a tap outside closes it; if a title was
   typed, it asks first. An add that fails says so in the dialog, or in a toast once the dialog is closed.
5. **Note | Reminder** sits at the top of the dialog, Note picked. A reminder has a required **Date**, an exact
   calendar date, in place of Due, since its date is its deadline; Add stays off until one is picked. Everything else
   is as for a note. Switching back to Note keeps the other fields and doesn't send the date. A reminder for a later
   date isn't in the list, so the toast says "Added. It joins the list on 12 March." instead of where it landed; one
   for today or earlier is in the list and gets the usual toast.

## Due

1. **Rough choices first.** The due picker offers the near future as rough choices: In a few days, Next week, In a
   few weeks, Next month, In a few months. Picking one saves a loose due time (`firm: false`).
2. **Exact date as the last choice.** Only "Exact date" opens a date field, and saves a firm due time.
3. **A loose due time never shows as a date.** The list, the item and the Add dialog show the server's fuzzy label
   ("in a few days") for it; only an exact due time shows its date, and only it can show as overdue. The item's due
   field opens the same picker, with the current choice selected.
4. **The server owns the choices.** Which dates the rough choices stand for, and what they're called, must agree
   with the fuzzy labels the server gives back, or "Next week" could come back as "in a few days". So the server
   lists the choices (`GET /due-choices`), each with its label and date in the request's timezone, and the client
   only shows them ([decisions.md](decisions.md#26-the-due-picker-offers-rough-choices-the-server-owns)).
5. **Due starts at None** in the Add dialog, with the rough choices one tap away. On a phone's row of due chips
   nothing is picked instead, and a second tap on a chip clears it. In the item, the picker is a sheet of 52 px rows
   with Exact date and Clear due.

## Waiting for me

The page at `/waiting`: every open question on an open item in the person's active heaps, answered in
its row, and every item whose agent turn is stuck, with the ways out.

1. The heading is "Waiting for me · 5" (no count at zero) and under it a quiet line, "All heaps, most important
   first". The order is the server's: the item's score in its own heap, then heap name, position and question number.
   Rows are one column at the app's content width, separated by thin rules; no item pane opens beside the page.
2. A row is the item's open-question row in Questions, the same component, with a head line of its own: the kind ("Request",
   "Question", "Question · pick any"), the item's title as an accent, underlined link to the item in its heap, the heap's
   name, the team's name only when the person is in more than one team, and `step "<title>"` when the question names
   one. Then the short text, and the reply controls as in the item. A row whose question lost its `reply` action
   shows no controls.
3. **Details ▸** holds the details as written and under them **Context ▸**, which unfolds the item's context and the
   heap's, each under a small heading ("This item", the heap's name), or "No context". They are fetched when Context is
   opened, not with the page. A question without details shows Context alone.
4. After a reply there is no toast: the row leaves, the count drops, the focus goes to the row that took its place if it
   was lost with the row, else to the heading, and a polite live region says Replied to “<text>”. If it fails the row
   stays with its picks and comment and the reason shows under it. If someone else replied first, or the item closed
   first, the row leaves and a note at the top of the list says so; the closed case keeps the comment so it can be
   copied. The notes stay until the next reply or until the person leaves the page. Drafts belong to their row and
   survive a refresh.
5. On a phone every control is 44 px, Reply spans the width, and Done and Can't be done share a row at half the width
   each.
6. Loading says "Loading…". With nothing waiting the page says "Nothing waits for you." and, quiet under it, "Questions
   from agents and teammates show up here." A failed refresh keeps the rows. Past 500 rows the first 500 show, with
   "Showing the 500 most important of n."
7. **A stuck row** is an item whose agent turn failed three runs in a row, so the agents' queue skips it. It is ranked
   among the question rows by the same order, before that item's questions, and counts in the heading and the
   navigation. Its head line reads "Stuck · <item> · <heap> · <team>" (and, in the item, "Stuck · going to: …"); under
   it "3 runs in a row failed: <why the last one failed>", cut to two lines with **Show all** when it runs longer
   (Show less folds it). Then **Retry**, the primary button, which acts at once, and **Take back**, which asks first
   inline, "Take this back from the agent? It stops, and what it changed stays." Retry and Take back make the row
   leave, as a reply does: the focus goes to the row that took its place, else to the heading, and a polite live
   region says "Retried “<title>”." or "Took back “<title>”." A reader sees the row without either button. If
   someone else took the turn back or retried it first, the row leaves and a note at the top says "Someone else
   changed this meanwhile." A reply draft in another row survives a stuck row leaving. On a phone both buttons are
   44 px, sharing a row.

## Closed

Done and won't-do items, most recently closed first, under Today, This week and Earlier from each item's
`closed_group`. One intro line explains the week limit. Each row has the title, which opens the item, "Done · today
by you" or "Won't do · 3 Nov by Sam", and Reopen while the item is inside its week, 44 px on a phone; once it can't
be reopened the row shows nothing more, with no per-row dates or "closed for good" labels. Load more pages. A
closed item opens over the Open list, and closing it returns to Closed.

## Teams and Team

Teams lists the user's teams ("Admin · 4 members · 2 heaps", "Only you · 1 heap" for the personal team) with Open,
and + Create team, which opens a form with Name and an optional Link name; the server's errors show under the field
they name. After sign-up, the empty heap offers the next two steps, Add your first item and Create a team; once a
team exists, the Team page itself shows New heap and Add member, with no extra prompt.

Among other teams, the personal team shows everywhere as "<name>'s team", by the user's display name whatever it is
stored as, so it has no Rename. The label is the web app's: the stored name is unchanged, because storing "\<name\>'s team" would need a migration and wouldn't
follow a display-name change. Activity entries name the team as stored.

The Team page follows the settings layout ([Rules](#rules) §3): the name with the rarer actions (Rename, Link name,
Activity) under a ⋯, then Heaps with + New heap and rows with their open count, Open and a ⋯ (Rename, Link name,
Archive or Unarchive, History), then Members with a search whose results are a listbox under it (arrows move, Enter
adds), member rows with an initials avatar, role badge, Make admin or Make member and Remove, and Leave team or a
line saying why the only admin can't leave. Removing a member says what happens: "They lose access to Launch and
Support. Their items stay." It's confirmed inline on a wide screen and in the member's actions sheet on a phone,
where heap rows are 52 px and open the heap, and a member's ⋯ opens a sheet headed "Sam Ortiz · @sam · Member".

For an admin, Members also holds **Invite link** beside the search (under it on a phone) and the team's working links
as rows ("Made by Ana · ends 16 Nov") with Copy and Revoke; the link just made is marked New and takes focus on its
Copy. Revoking is confirmed the same way, saying the link stops at once and people who joined stay, and on a phone a
row's actions are in its ⋯ sheet. A copy the browser refuses shows the link in a field to copy by hand.
Every link has a **History** (made, revoked, each person who joined through it), a button on a wide row and an entry in
a phone row's sheet. Links that ended or were revoked are under a collapsed **Past links (N)**, each saying who made
it, when it ended or who revoked it and when, and how many joined; on a phone their History is a button on the row.
The team's Activity, shown to admins like other membership lines, says "Ana made an invite link", "Ben revoked Ana's
invite link" and "Sam joined through Ana's invite link".

## Sign in and create account

A card on a wide screen and the bare form on a phone, with a Sign in / Create account switch. Sign in takes "Email
or username". Create account takes name, username, email and password; the username is permanent and part of every
link, so its hint says so. A problem about an input shows under it, with focus on it; any other (wrong password, too
many attempts, registration closed) shows above the button. A taken email offers **Sign in instead**, which switches
to Sign in with the address carried over and focus on Password. Under Sign in's password, a small **Forgot your
password?** button (`aria-expanded`) shows one line under it, "Whoever runs this Upheap server can send you a reset
link.", and hides it again: no form and no request, since the server sends no email. Create account has none. The
invite page's Sign in side has it too. A signed-out link says where it leads
("Sign in to open the link to k3x9a") and opens it after sign-in; an expired session returns the same way.

## Invite page

`/invite/{token}` is where an invite link leads, outside the app's shell like sign-in and in the same card, and needs
no sign-in. The server decides the state (the link's state, the viewer's standing and `allowed_actions`), in this
order, and the page only shows it:

- **Checking the link…** while it loads, with no form.
- **Not an invite link** for any token the server doesn't know, naming no team.
- **Already a member:** no page; they land in the team (below) with "You're already in Acme.", whatever the
  link's state.
- **Ended or revoked:** says which, who the link was for and from, and to ask the team for a new one.
- **Removed:** "This link can't add you back. Ask one of the team's admins."
- **Signed in:** "Join Acme?", who invited them, and who they're signed in as, with **Join Acme** and "Not you? Sign
  out", which reloads the same page signed out.
- **Signed out:** "Join Acme" and who invited them, over the sign-in switch with Create account first
  ([Sign in and create account](#sign-in-and-create-account)). Create account and join makes the account and joins in
  one act, also when registration is closed. A taken email offers **Sign in instead**, which carries the address
  over. Signing in leads to the signed-in state, where they press Join: it doesn't join by itself, so they see who
  they joined as.

The states that can't be used end in an **Open Upheap** button. A failed read says why, with Try again. A link that stops
working while a form or the confirm is open, or a session that ends, refetches the invitation and the page becomes
the state that says so; it never sends the user to sign-in. When the page changes state, focus moves to its heading.

A new member lands on the team's heap when it has exactly one that isn't archived, otherwise on the Team page, which
lists the heaps. Landing replaces the invite page in the history. The app's toast says "You joined Acme." (or "You're
already in Acme."), belongs to the page they land on like any toast, and is shown once: the line is taken out of the
history entry as it is shown, so a reload doesn't repeat it.

## Reset page

`/reset/{token}` is where a password reset link leads, outside the app's shell like sign-in and in the same card, the
same way at every width, and needs no sign-in: holding the link is the permission, so a signed-in visitor sees the
same page. The operator makes the link with the server's command (README, Resetting a password). The server decides
the state, and the page only shows it:

- **Checking the link…** while it loads.
- **Not a reset link** for any token the server doesn't know: "This isn't a reset link.", "Check that the whole link
  was copied, or ask whoever runs this server for a new one."
- **Used**, **cancelled** (a newer link was made, or the password was changed: "A newer one was made, or the password
  was changed since.") and **ended** say which, in a heading ("This reset link was already used.", "… no longer
  works.", "… has ended."), then "Ask whoever runs this server for a new one."
- **Working:** "Set a new password", "for Sam Ortiz (@sam)", the **New password** input (focused, hint "At least 10
  characters."), **Set password and sign in**, and under it "This signs you out everywhere else."

The states that can't be used end in an **Open Upheap** button, which leads to sign-in when signed out. A failed read
says why, with Try again. Enter submits, and the button is disabled while it is pending and until the landing. A
problem about the password shows under the input, marked invalid and focused, with what was typed kept: "Enter a new
password.", "Use at least 10 characters." and "That password is too long." The link stays working after them. A `404`
or `409` means the link changed meanwhile (another tab used it, it ended), so the page reads it again and becomes the
state that says so; it never shows the raw error. Anything else is the server's message above the button. When the
page changes state, focus moves to its heading. A `401` on this page never sends the user to sign-in.

Setting the password signs the browser in as the link's user, whoever it was signed in as, and ends every other
sign-in of that user. It lands on `/`, which goes to the user's heap, replacing the reset page in the history, with
the toast "Password changed. You're signed in.", shown once like the invite page's.

## Password

Account's Password card sits between Profile and Appearance. Closed, it is the heading "Password"
and a **Change password** button on one row (wrapping on a narrow phone). Pressing the button replaces it with a
form: **Current password** (focused) and **New password** with the hint "At least 10 characters.", then **Change
password** and **Cancel**, and under them "This signs you out of your other browsers and the command line." Enter in
either input submits, and both buttons are disabled while it is pending. What was typed is sent as typed.

Errors sit under their input, which is marked invalid and described by them, and focus moves to the first one: "Enter
your current password.", "That isn't your current password.", "Enter a new password.", "Use at least 10 characters."
and "That password is too long." Typed values stay, except Current password after a wrong one, which is cleared and
focused for a password manager to fill again. Too many tries, or anything else, is the server's message above the
buttons. Cancel and Escape close the form, clear it and give focus back to Change password.

A success closes the form and shows a status line under the heading, "Password changed. 2 other sign-ins ended.",
"… 1 other sign-in ended." or "… No other sign-ins were open.", until the form is opened again or the page is left.
Focus goes to Change password, and the Command line card is read again, since the change ended its sign-ins. The
browser the change was made in stays signed in, on a new session cookie the server sets; another browser of the user
lands on sign-in at its next request. A change also cancels the user's working reset link ([Reset page](#reset-page)),
so a copy of it can't be used afterwards. The card is there when the server's `change_password` action is.

At the bottom of the card, whether the form is closed or open, is a collapsed **History** like the ones on a tag or a
member, with the same chevron and touch target. It loads only when opened, and lists the user's password events newest
first, each a sentence from the server and its time: "Password changed. 2 other sign-ins ended.", "Reset link made by
whoever runs the server." and "Password set with a reset link. 3 sign-ins ended." It names no actor, since it is the
user's own account, and holds nothing else of the account (not sign-ups or profile changes). Empty, it says "No
password changes or resets yet."; an error shows the server's message; **Load more** fetches the next page. A password
change adds its entry to an open History at once, and to a closed one by the time it is opened.

## Command line

Account's Command line card lists where the `upheap` CLI is signed in as the user. A row has a
terminal icon, the machine's name (the client's name when it sent none), "Signed in 3 Nov · last used 2 days ago", and
a **Sign out** button named for the machine. The server words the name and both dates, in the request's time zone. The
button turns the row into an inline confirm that names it and says the CLI there stops working until someone runs
`upheap auth login` again; one row confirms at a time, and Cancel gives focus back to the row's button. While a
sign-out is pending, the other rows' buttons are disabled. After a sign-out the card says so in a status line and focus
moves to its heading, since the row it was on is gone. With none signed in, the card says so and shows the command to
start with, with this server's address. The card is there when the server's `manage_sign_ins` action is. Ending a sign-in never touches the browser's own
session.

## Conflicts and errors

1. **Conflicts say what was lost.** "Sam changed the due date a moment ago. You're seeing their version; your
   change (due: next week) wasn't saved. Apply mine again." The retry is one tap, and the pane re-renders from the
   server.
2. **Loading** keeps the previous view, dimmed, rather than a blank page.
3. **A `401` mid-session** sends the user to sign-in with the way back, as a signed-out visit does, dropping the
   last user's cached data; signing in returns them to the page.
4. Other item errors show inline in the pane with the server's message: a closed item only changes state, a reopen
   past its week, an option archived meanwhile.

## Rules

1. **Confirmations.** A reversible change acts at once and offers Undo in a toast: state changes, archiving a heap,
   a view or a parameter. A change that can't be undone, or that affects other people, asks first, inline or in a
   dialog on a wide screen and in a sheet on a phone, and names the object: removing a member, giving up your own
   admin role, revoking an invite link, and discarding a typed title.
2. **The phone and the wide screen share components and differ only in presentation.** On the phone, the item
   sheet's main state button and ⋯ sit in a bar at its bottom; on a wide screen they sit beside the title.
3. **Every page that edits a list of things** (Tags, Parameters, Members, Heap settings) uses one settings layout:
   title, one-line explanation, a way to add one near the top (a "+ New" button, or a short form when adding takes a
   field or two), rows that turn into a form when edited, and the rarer actions out of the way (a ⋯, or the foot of
   a card).
4. **After the session expires, signing in returns the user to the page they were on.**
5. **Editing lives in the item.** Rows are for reading, ordering and changing state; everything else is edited in
   the item ([decisions.md](decisions.md#23-editing-lives-in-the-item-rows-only-change-state)).

## Testing

The Playwright golden paths run on phone (Pixel 7 emulation) and tablet (iPad emulation) projects as well as desktop
Chromium, against the real server.

## Later: keyboard

The wide screen gets shortcuts once the screens work without them, shown in the MainKeyboard board:

1. Hints on the controls they trigger: `N` on Add item, `/` in the search box, `S` `B` `X` on a row's Start, Block
   and Done, `G O` and `G C` beside Open and Closed, `⌘↵` on Add in the Add dialog.
2. ↑ ↓ to move between rows and Enter to open; ↑ ↓ in the item pane walk the list.
3. A hint bar under the list with the keys for what's on screen, and `?` for all of them.
4. ⌘K in the sidebar: find any item by title or key in every heap; Tab acts on the highlighted item (start, done,
   due, copy link), Enter opens it, and it can also create ("Add 'safari' as a new item") and navigate. Searching
   every heap needs a server search endpoint; today's search filters only the loaded list.

The keys that are already built (`N`, `/`, ↑ ↓, Enter, `x`, `⌘↵`) keep working. They just aren't shown yet.
