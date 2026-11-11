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

The jobs each build must support. **Loop** flows happen many times a day, **setup** flows once or rarely,
**recovery** flows after a mistake or a failure.

### One person, the local build

1. **First open** (setup). The page opens the browser's database and makes the user, the team and the heap
   "Personal". If storage can't open, every page shows an error; if Upheap is already open in another tab, a page
   says so and opens once that tab closes. The empty heap says to add the first thing, and the storage note says the
   data lives only in this browser. The first items go in through Add, with Add another keeping the dialog open, and
   come back ranked, each with its contributors.
2. **Daily loop** (loop). The list shows in-progress items pinned on top, then Up next. Start one from the item or
   by swiping its row left; finish it with the state circle, a swipe right, or Done in the item; each change gets an
   undo toast. Add what came up with the Add button, `N`, or + on a phone. Re-size or re-date in the item and watch
   it move. Block an item, naming what it waits for; it leaves the list, since blocked items are hidden by default,
   and "Show them" brings them back to unblock. Find an item with search.
3. **Weekly review** (weekly). Closed: reopen what was closed by mistake, within its week, which is also the fix for
   a missed Undo (recovery). Back in the list, close what won't happen as Won't do and re-size the rest. A reload
   re-ranks at the current time.
4. **Keeping the data safe** (setup, then recovery). The storage note leads to Account's Your data: download a copy
   now and then. To restore, or to move to another browser, open a copy, confirm the replace, and the page reloads
   with it.

Not core here: more heaps, the settings tabs, saved views, filters and sort. One heap and its list do the job.

### A team, the server build

1. **Sign up and sign in** (setup). Create account with a name, username, email and password lands on the empty
   personal heap. Later visits sign in; when the session ends, sign-in brings the user back to the page they were on
   (recovery).
2. **Getting into a shared heap** (setup). Someone creates a team on Teams and becomes its admin; the Team page
   opens. The admin adds a heap, adds each teammate (who must have an account; invitations come later), and makes a
   second admin so the team doesn't hang on one person. A teammate finds the heap in the sidebar or the heap
   switcher.
3. **Daily loop in a shared heap** (loop). Pick the heap, then the single-user loop. Teammates' changes arrive as
   the list refetches, and rows animate to their new places.
4. **Coordinating.** See what others changed in the refreshed list, an item's history or the heap's Activity.
   Saving over a teammate's change shows a conflict and their version; redo the change if it still applies
   (recovery). Point someone at an item with Copy link: it opens the item, after sign-in if needed, or says the item
   can't be seen.
5. **Admin basics** (setup, occasional). On the Team page: add members, make or unmake admins, remove someone; add,
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

| Width | Navigation | List | Item |
| --- | --- | --- | --- |
| Phone, under 640 px | Heap name at the top, with its team ("Product · Acme") unless the heap is personal, switches heap; bottom tabs Open, Waiting, + (Add), Closed, More (the local build: Open, Closed, +, Settings, More) | Full width, compact rows | Bottom sheet over the list |
| Tablet, 640–1023 px | An 80 px rail: Waiting (server build), the heap button, then the views as icons with labels, the heap's team, Teams and Account at the bottom | Beside the rail, with the wide screen's toolbar and rows | Panel over the right of the list, the list still live |
| Wide, 1024 px and up | Sidebar: Waiting for me with its count and a thin rule under it (server build), a heap button ("Product · Acme") that opens the heap switcher, then the heap's views with Open's count, the heap's team, Teams, Account and the storage note at the bottom | Middle column | Pane beside the list while an item is selected |

**More** on a phone holds Settings (server build), Activity, Team, Teams and Account (with Your data in the local build), as a card of 52 px
rows above the tabs. Account is the last entry there and in the sidebar, and Sign out is on the Account page, under
its cards, which are Profile, Appearance, Command line (server build) and Your data (local build). The **+** in the middle of the tabs opens the Add dialog from any of the heap's pages, under the thumb,
so the Add button beside the heap name shows only from a tablet up.

The sidebar and the rail list the heap's views (Open, Closed, Activity, Settings) and, below them at the bottom,
the team in the URL (a link to its page, shown on the heap's pages and the team's own, and not in the local build,
where Teams already leads to the one team), Teams and Account. The heap's title names its team too ("Product ·
Acme"), on the phone's top bar as well, except for a heap in the personal team.

A phone's tabs have room for four: Waiting sits left of the + and Closed right of it, and Settings and Activity go
into More with the account pages. The local build has nothing to wait for, so it keeps Open, Closed, +, Settings and
More, and no Waiting anywhere.

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
Cancel give focus back to Edit. The local build has no Context card.

The **Agents** card, under Context, has a switch, "Agents prepare new notes", with the line "Each note added here is
sent to an agent to fill in its context, then comes back to you." It saves as it is turned, and can't be turned again until the save ends. Only an admin sees the
switch (`set_agents_prepare`); others, and everyone in an archived heap, read "Agents prepare new notes: on" or
"off". The local build has no Agents card.

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
2. **The list** takes the rest of the width, with the wide screen's toolbar (search, Sort by, Filters) and rows,
   including the effort and impact sizes, at touch sizes (44 px targets).
3. **The item** opens as a 400 px panel over the right of the list. The list beside it stays live: tapping another
   row switches the panel, and the selected row stays highlighted. Swiping the panel right or tapping ✕ closes it.
   Its controls are the phone sheet's (Start, Blocked, Done buttons; S M L XL buttons).
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
since a "last opened" on the server would cost a write on every heap visit and wouldn't work in the local build;
they don't follow the user to another device. Search gives a flat list with the team and an
"archived" tag, a "New heap “…”" row, and a message when nothing matches. Arrows move, Enter opens and Escape
clears the field, then closes. It ends with "New heap in \<team\>", in the heap's own team, when the team allows
it. On a tablet it is a popover beside the rail.

Gestures are offered where the pointer is coarse (a finger), on phones and tablets alike:

1. A row dragged right past about 100 px slides away and marks the item done; dragged left, it stays open on two
   buttons, In progress and Blocked (Open in place of the state it's in), until one is picked or the row is tapped.
   Each is followed by the undo toast. A tap just after a drag doesn't open the item.
2. The item sheet drags down by its top bar to close; the tablet panel drags right. Letting go short of 120 px
   springs it back.

## The list

1. A row is one or two lines: a state circle, the position, the title, the due chip and tags, and below them the
   server's contributors ("Due this week · small effort"). On a wide screen the effort and impact sizes sit at the
   right edge. A task has a checklist icon in a fixed slot before the title (a note leaves the slot empty, so titles
   line up), and an item with context or defined has a page or a clipboard icon after it. The row shows no words for
   them; a screen reader hears them after the title ("Rotate the certs, task, defined"). A task with steps shows how
   many are ticked, small and muted, after the stage icon (`2/5`; the static build, which shows no stage icon, has it
   in the same place), and a screen reader hears ", 2 of 5 steps ticked" after the title. Dropped steps don't
   count, and a task without an open or ticked step shows nothing. The static build never shows a stage icon.
2. Custom parameters with a value show as violet "Name: Option" chips ("Energy: Low") after the tags on a wide
   screen, and on their own line under the contributors on a phone or tablet. Unset parameters don't show. Violet is
   kept for parameters everywhere, so they read apart from tags (grey) and due dates.
3. The state circle marks the item done in one tap. An undo toast follows every state change from the list: bottom
   centre, above the tabs on a phone, gone after a few seconds. Changes made in the item save silently and have no
   undo. The toast sits over the open item, so Sent to an agent can offer Undo from inside a phone's sheet; the
   Add dialog and the other sheets stay above it.
4. Tapping or clicking a row opens the item. Keyboard on a wide screen: up and down move, Enter opens, `x` marks
   done; `N` and `/` stay.
5. On a phone, swiping a row right marks it done; swiping left offers In progress and Blocked.
6. Saved views are hidden in the web app for now, and `view=` in the URL is ignored: a views bar duplicated the
   Filters panel, which has no place for saving one yet. The API keeps them, for the CLI's `--view`. When they
   return, "Save as view…" goes in the Filters panel.
7. One Filters panel holds what the list shows and its filters: Show (Open, In progress, Blocked as Pinned, Shown or
   Hidden, a dot on a changed one; shown blocked items in their own group or mixed in), then Narrow down (tags, need,
   effort, impact, parameters), with Reset all, "6 of 14 items" and Done. It's a popover from a tablet up and a sheet
   on a phone. The toolbar is search, Sort by and Filters; on a phone it's search and an icon Filters button, and the
   sheet, "Filters & sort", starts with the sort and ends with "Show 6 items". The Filters button counts the filters
   and changed settings, amber while any are on; on a phone, where the sort is out of sight in the sheet, it counts a
   sort other than Priority too ("Filters and sort, 1 on"). The list updates live behind the panel.
8. Under the toolbar a chip takes off each filter ("Need: Critical") or changed setting ("Blocked: shown"), with
   "6 of 14" while filters are on and Reset all. Reset all keeps the sort and the search.
9. On hover, an open row adds one quiet button, Start, and ⋯ for the other states. Nothing else sits on the rows.
10. After Done, the toast offers Undo. Only when nothing else in the heap is in progress does it also offer "Start
    next: \<the top open item\>"; if something else is already in progress, the user is carrying on with it, and the
    offer would be noise.
11. Active filters and changed view settings show as the removable chips of point 8, so the user always sees why the
    list is short.
12. The hidden-items note offers the fix: "3 blocked items are hidden. Show them."
13. An open item with an open question says **waits for you** at the end of its row, small, semibold and in the accent.
    On a phone, where the end of the row is tight, a small accent dot before the title stands for it. A screen reader
    hears ", waits for you" after the title on every width. The closed list and the static build never show it.
14. An open item an agent has says **with an agent** at the end of its row, small, in the quiet ink and at normal
    weight, where "waits for you" would be: the two never show together, as an item that waits for the person is
    not the agent's. On a phone a small robot mark before the title stands for it. A screen reader hears ", with an
    agent" after the title. Who has an item never changes its place. The closed list and the static build never show
    it.
15. Moving an item an agent has to a state that takes it back, from the list, asks first, naming the item: "An
    agent has “Rotate the certs”. Marking it done takes it back from the agent." The circle, a swipe, `x`, the
    hover Start and ⋯ (Start, Won't do) go through the one ask, which is a dialog on a wide screen and a sheet on a
    phone, with "Take back and mark done", "Take back and start" or "Take back and close", and Cancel. Cancel
    puts a swiped row back and gives focus to the row. Blocked and Open don't ask. The change's Undo toast
    restores the state only: the turn stays taken back.

## The item

1. The item route (`/teams/$team/heaps/$heap/items/$item`) shows the list with the item over it or beside it,
   instead of a separate page. Links and the back button keep working: going back closes the sheet.
2. It opens with the kind line under the title ("Task · With context": the icons with their words, and the kind
   alone when the item is closed or in the static build), then the state (the likeliest next state as the main
   button, then the others, with the other choices under ⋯, on a wide screen; on a phone the same buttons sit in a
   bar at the bottom of the sheet, the main one wider, with ⋯ beside them, and the state is a line under the kind
   line), then "Why it's #3" from the contributors, then a task's Steps (point 11), then the need's
   four steps, effort and impact as S M L XL buttons, due, tags, parameters, the context (the item's description)
   and the history. On a phone due, tags, parameters and the context are rows of label and value, each opening a
   sheet; the context saves as its sheet closes.
3. Each parameter is picked like effort and impact: a row of option buttons when it has a few options (up to four),
   and a menu row when it has more or long labels. A parameter linked to one of the item's tags shows up as soon as
   the tag is added, in Add item too.
4. Changes save as they are made, each with its own `If-Match`. They go one at a time, each with the version the
   previous one brought back, so quick changes don't conflict with each other.
5. Closing it (✕, Escape, or Back) goes back where it was opened from: Back when the app opened it, the list when it
   was opened from a link. Opening another row while one is open replaces it in the history.
6. The main button is the likeliest next state: Start for open, Done for in progress, Unblock for blocked. The other
   states are next to it. ⋯ is named More and holds Won't do, Make it a task… and Stage, each only when the server
   allows it; with none to show, ⋯ is absent. Make it a task asks first, naming the item, since a task can't go
   back to a note. Stage lists as written and with context, and defined on a task only (a note says "Defined needs
   a task"); picking one saves silently, like every change in the item.
7. On a wide screen and a tablet, ↑ ↓ in the pane header walk the list without closing the pane, so triage is open,
   change, next. On a phone the sheet closes and the next row is one tap away, so the sheet has no Next button.
8. A link sign beside the title copies the item's short link, and the key (`k3x9a`) shows in the header. Nothing
   else has Copy link.
9. "Why it's #3" shows the contributors with their `share_percent` as bars. Below them, "Held back by" lists what
   keeps the item from ranking higher: a low need, no due date, unset sizes, in that order and without bars, each
   taking an editor to its field (a link on a wide screen; a row on a phone, where the due one opens the Due
   sheet). A reader sees the same lines as text. The item shown first in its list has none, nor does a blocker
   ranked by what it blocks, nor any item while the list is sorted by something other than priority. The list row
   doesn't show them.
10. The item says when it was created and by whom ("Added by Sam 4 days ago"), next to a History link.
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
12. **Waits for you**, while the item is open and has open questions, is the one tinted, outlined block in the item
    (the accent at a low fill, a 2 px accent border), under the state buttons and above "Why it's #3", the same on
    the pane, the tablet panel and, inline at the top, the phone sheet. One row per open question, in number
    order, separated by a thin rule: a quiet line with the kind ("Question", "Request", "Question · pick any") and
    `· step "<title>"` when it names one, the short text in semibold, "Details ▸" when it has details (opening them
    in place), then the reply controls. A question without choices has an "Answer" field and Reply, off until the
    answer has text. With choices it has radios (one) or checkboxes (any), "Comment, optional" and Reply, off until
    something is picked. A request has "Comment, optional" and two buttons, Done and Can't be done, which reply as
    soon as they are pressed, sharing one row on a phone. On a phone Reply spans the width, and every control is
    44 px.
13. After a reply there is no toast: the row leaves Waits for you and joins Questions, the focus goes to the next
    row's first control, else to the item, and a polite live region says Replied to “<text>”. While it is
    sending, that row's buttons are off. If it fails the row stays with its picks and comment and the reason shows
    under it. If someone else replied first, the row leaves and the section says "Someone else replied to “…”
    meanwhile." until the next reply. In an archived heap a row shows its text and details with no controls. A
    closed item has no Waits for you: closing withdrew its open questions, which read in Questions as "Withdrawn".
14. **Questions n**, a closed disclosure just above History, lists the answered questions and the withdrawn ones in
    number order: the kind line, the short text, Details ▸, then `→ <picked labels>. <comment as written>` and
    "<who>, <when>" in quiet text, or a muted "Withdrawn". It only reads; changing a reply is later.
15. **Who has it** is a quiet block under Waits for you and above "Why it's #3", on an open item in an active heap
    (not in the static build). With no agent it says "With you." and has **Send to an agent**, which opens a form:
    a popover on a wide screen and a tablet, a sheet on a phone. The form has two choices, "To <where it goes>" (the
    next stable state: with context, defined or done) and "All the way", or on a defined task the one line "To
    done"; "A line for the agent (optional)"; Send and Cancel. Send acts at once, closes the form, gives focus to
    Take back and shows a toast, "Sent to an agent, going to: with context.", with Undo, which takes it back without
    asking or a second toast. With an agent the block says "With an agent since 14:02" (the time today, else the
    date as the server words it: "yesterday", "3 Nov"), "going to: <state>" and ", all the way" for that reach, the person's line in quotes, and
    "Prepared on add" for a note the heap's setting sent. If the item waits for the person it reads "An agent is
    waiting for your answer, going to: …". **Take back** asks first, "Take this back from the agent? It stops, and
    what it changed stays.", inline on a wide screen and in a sheet on a phone, and gives focus to Send to an agent
    afterwards, with no toast. A failed send or take back says why under the block, and the form keeps its line.
    Someone else's send first says "Someone already sent this to an agent." and the block shows it. Start, Done and
    Won't do in the item ask before they take an item back ("An agent has this. Starting it takes it back. Take it
    back and start it?"), inline beside the state buttons or in a sheet; Blocked and Open don't.

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

The page at `/waiting` (server build): every open question on an open item in the person's active heaps, answered in
its row.

1. The heading is "Waiting for me · 5" (no count at zero) and under it a quiet line, "All heaps, most important
   first". The order is the server's: the item's score in its own heap, then heap name, position and question number.
   Rows are one column at the app's content width, separated by thin rules; no item pane opens beside the page.
2. A row is the item's Waits for you row, the same component, with a head line of its own: the kind ("Request",
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
stored as, so it has no Rename. Alone, as in the local build, it keeps its own name ("My heaps"). The label is the
web app's: the stored name is unchanged, because storing "\<name\>'s team" would need a migration and wouldn't
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

## Sign in and create account

A card on a wide screen and the bare form on a phone, with a Sign in / Create account switch. Sign in takes "Email
or username". Create account takes name, username, email and password; the username is permanent and part of every
link, so its hint says so. A problem about an input shows under it, with focus on it; any other (wrong password, too
many attempts, registration closed) shows above the button. A taken email offers **Sign in instead**, which switches
to Sign in with the address carried over and focus on Password. A signed-out link says where it leads
("Sign in to open the link to k3x9a") and opens it after sign-in; an expired session returns the same way.

## Invite page

`/invite/{token}` is where an invite link leads, outside the app's shell like sign-in and in the same card, and needs
no sign-in. The server decides the state (the link's state, the viewer's standing and `allowed_actions`), in this
order, and the page only shows it:

- **Checking the link…** while it loads, with no form.
- **Not an invite link** for any token the server doesn't know, naming no team. The local build has no invite
  endpoints, so it says this for every link.
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

## Command line

In the server build, Account's Command line card lists where the `upheap` CLI is signed in as the user. A row has a
terminal icon, the machine's name (the client's name when it sent none), "Signed in 3 Nov · last used 2 days ago", and
a **Sign out** button named for the machine. The server words the name and both dates, in the request's time zone. The
button turns the row into an inline confirm that names it and says the CLI there stops working until someone runs
`upheap auth login` again; one row confirms at a time, and Cancel gives focus back to the row's button. While a
sign-out is pending, the other rows' buttons are disabled. After a sign-out the card says so in a status line and focus
moves to its heading, since the row it was on is gone. With none signed in, the card says so and shows the command to
start with, with this server's address. The card is there when the server's `manage_sign_ins` action is, so never in
the local build, which has no accounts and nothing to sign in to. Ending a sign-in never touches the browser's own
session.

## Your data

In the local build, the storage note is normally just "Stored only in this browser · Download a copy". It becomes a
nudge only when it applies: when the browser hasn't agreed to keep the data, and when no copy was downloaded in the
last two weeks, or ever. Account's Your data card has Download a copy, Open a copy… and "2 heaps · 41 items · last
downloaded 26 Oct". The replace confirmation names the file, says what the copy holds and from when, compares the
item counts, and offers Download current first; a file that isn't an Upheap database is refused in the confirm. It's
a modal on a wide screen and a sheet on a phone. How the data is kept is in
[architecture.md](architecture.md#keeping-the-data).

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
   admin role, revoking an invite link, replacing all local data, and discarding a typed title.
2. **The phone and the wide screen share components and differ only in presentation.** On the phone, the item
   sheet's state buttons sit in a bar at its bottom; on a wide screen they're the main button plus the others.
3. **Every page that edits a list of things** (Tags, Parameters, Members, Heap settings) uses one settings layout:
   title, one-line explanation, a way to add one near the top (a "+ New" button, or a short form when adding takes a
   field or two), rows that turn into a form when edited, and the rarer actions out of the way (a ⋯, or the foot of
   a card).
4. **After the session expires, signing in returns the user to the page they were on.**
5. **Editing lives in the item.** Rows are for reading, ordering and changing state; everything else is edited in
   the item ([decisions.md](decisions.md#23-editing-lives-in-the-item-rows-only-change-state)).

## Testing

The Playwright golden paths run on phone (Pixel 7 emulation) and tablet (iPad emulation) projects as well as desktop
Chromium, against both the server and the local build.

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

## Later: an installable app

Once the website works at every width, the local build can become a Progressive Web App (PWA): the same site,
installable from the browser and working offline. App-store builds can follow by wrapping it in Tauri 2. React Native
is not the route: it would mean rebuilding every screen without the DOM, and its JavaScript engine can't run the local
build's WebAssembly backend ([decisions.md](decisions.md#22-one-react-app-for-every-width)).

What the PWA would need, for when it's picked up:

1. A manifest (name, icons with maskable variants, `display: standalone`, `start_url` and `scope` at the base path)
   and an Apple touch icon.
2. A service worker (vite-plugin-pwa) that precaches the app, the WebAssembly backend and SQLite-wasm, with
   navigations falling back to `index.html`.
3. An update prompt, since a new version waits for a reload; migrations already run when the worker opens the
   database.
4. Install where the browser offers it, and Share → Add to Home Screen instructions on iOS. Installed apps also get
   persistent storage more readily, and iOS doesn't clear their data after seven days unused.

The server build needs the network for every request, so it would get only the manifest.
