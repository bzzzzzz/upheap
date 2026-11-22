# Web app UX

How the web app looks and behaves, at every width and in both builds: the principles, the core flows, the toolkit,
the layout per width, each screen, the rules that settle questions, and what comes later. The mockups live outside
the repository, in the "Upheap layouts" canvas; a board is named where this doc relies on one.

The app is one React app whose layout changes with the width: **phone** under 640 px, **tablet** 640 to 1023 px,
**wide** 1024 px and up. It never ranks, checks permissions or works out labels; the server does
([architecture.md](architecture.md#principles)), and the client adds layout, gestures and animation.

## Principles

1. **Consistent look.** One set of tokens: stone neutrals, amber for "in progress" and for the next action, teal
   for done, red only for a firm due date that's overdue, violet only for parameters. Tags take their own colour from an
   eight-colour palette (blue, sky, teal, green, lime, rose, pink, grey) on the chip's border and text, with no fill;
   violet, amber and red are left out of it, so parameters, the accent and overdue keep their meaning. A control
   looks the same wherever it appears: S M L XL buttons, the due choices and the tag chips are identical in the item
   pane, the Add dialog and their phone sheets.
2. **Predictable layout.** Every screen keeps the same areas in the same places. Wide: sidebar, then the list,
   then the item pane. Phone: top bar, then search and Filters, then the list, then the bottom tabs with + in the
   middle. Overlays come in only two kinds: a centred dialog or popover on a tablet or wide screen, a bottom sheet
   on a phone.
3. **At your fingertips.** Whatever is used several times a session is visible without opening anything: the ranked
   list, Add, state changes, search. On a phone it sits within thumb reach at the bottom.
4. **One step ahead.** Whatever the user is likely to do next is one click away. Everything they can do with what's
   on screen is at most two obvious clicks away (open, then act). Keyboard shortcuts come on top of this, not
   instead of it ([Keyboard](#keyboard)).
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
   (recovery). Point someone at an item with its link, copied from the key in its header: it opens the item, after
   sign-in if needed, or says the item can't be seen.
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
5. **CodeMirror 6** for the Markdown editor (see Markdown text).

### Markdown text

Longer text that people and agents write shows as rendered Markdown, through one component (`Markdown`): the context of
a heap, an item and a step, a question's details, the comment in a reply, and an item's comments. It is
GitHub-flavoured: headings, emphasis, links, code, lists, task lists as disabled checkboxes, quotes and tables. Raw
HTML is never rendered and images are never loaded: an image shows as a link. An in-page anchor (a footnote's) moves
in place, every other link opens in a new tab, and an unsafe one (`javascript:`) reads as plain text. A line break in
the text stays one, so plain text written before reads as it did. Long words and addresses break, and code blocks and
tables scroll sideways inside their box, so the page never does. Writing a context (a heap's, an item's, a step's) or a comment is in an editor that keeps the Markdown source as typed
and styles it as you type: the markers (`#`, `**`, `-`, backticks, link brackets) stay visible and muted, headings are
semibold, bold is bold, code is mono and link text takes the link colour. It has no toolbar and no preview, and the
saved text is the plain Markdown. It is CodeMirror 6, loaded only when one of these fields is about to show, so pages
that only read don't load it. A turn's line, a stuck reason, error messages and a question's short text and choices are
one line each and stay plain, except that in a question's text and choices a web address (`https://…`) and a Markdown
link to one (`[the mockup](https://…)`) are links that open a new tab; a link in a choice opens without picking it. The
rest of the line shows as written.

## Layout

Breakpoints are Tailwind's: `sm` 640 px and `lg` 1024 px.

| Width                | Navigation                                                                                                                                                                                                                             | List                                                     | Item                                                  |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------- |
| Phone, under 640 px  | Heap name at the top, with its team ("Product · Acme") unless the heap is personal, switches heap; bottom tabs Open, Waiting, + (Add), Agents, More                                                                                    | Full width, compact rows                                 | Bottom sheet over the list                            |
| Tablet, 640–1023 px  | An 80 px rail: Waiting, the heap button, then the views as icons with labels, and Agents, the heap's team, Teams and Account at the bottom                                                                                             | Beside the rail, with the wide screen's toolbar and rows | Panel over the right of the list, the list still live |
| Wide, 1024 px and up | Sidebar: Waiting for me with its count, and a thin rule under it, a heap button ("Product · Acme") that opens the heap switcher, then the heap's views with Open's count, and Agents, the heap's team, Teams and Account at the bottom | Middle column                                            | Pane beside the list while an item is selected        |

**More** on a phone holds Closed, Settings, Activity, Data, Team, Teams and Account, as a card of 52 px rows above the tabs. Account is the last entry there and in the sidebar, and Sign out is on the Account page, under
its cards, which are Profile, Email, Password, Appearance and Command line. The **+** in the middle of the tabs opens the Add dialog from any of the heap's pages, under the thumb, so
the Add button beside the heap name shows only from a tablet up.

The sidebar and the rail list the heap's views (Open, Closed, Activity, Data, Settings) and, below them at the bottom,
the team in the URL (a link to its page, shown on the heap's pages and the team's own), Teams and Account. The heap's title names its team too ("Product ·
Acme"), on the phone's top bar as well, except for a heap in the personal team.

**Find** on a phone is a search icon at the end of the top bar, before Select, named "Find in all heaps", on every page
with the top bar: one tap opens it (Keyboard, point 5). The list's own field below says "Search this list", so the
search that covers every heap and the one that filters the list on screen don't read as the same thing.

A phone's tabs have room for four: Waiting sits left of the + and Agents right of it, and Closed, Settings, Activity
and Data go into More with the account pages. More shows as active on the pages it holds, Closed among them.

**Waiting for me** comes right after Find at the top of the sidebar and the rail, above the heap switcher: an inbox
icon, "Waiting for me" in the sidebar with its count on the right, "Waiting" in the rail and the tabs with the count
as a small badge on the icon. There is no count at zero, and the entry stays. The count is the number of open
questions across the person's heaps, fetched with the page and again every 30 seconds while the tab is visible, as the
list is, so a question an agent asks shows in the count as soon as on its row. It's also fetched again on focus and
after every change the person makes to an item, a question, a step or an agent's turn. The heaps' open counts and
the Agents page's Your decision are fetched again with it, all three after each such change, since one change can move
any of them: a reply can close its item, and sending an item on takes it off Your decision.

**Agents** is the first entry of the block at the bottom of the sidebar and the rail, above the heap's team: a bot
icon and "Agents", with no count. What agents finished is there to look through when the person wants to, not
something that waits as a question does, so nothing in the navigation counts it; the page's Your decision heading
does.

**All items** is reached from the heap switcher, on every width: nothing in the sidebar, the rail or the tabs leads
to it. The switcher starts with an "All heaps" row with the person's open items at its right: the sum of the open
counts of the person's active heaps, the numbers the heaps show, so it costs no request and changes when those do. A
person in more than one team also finds, at the right of each team's line, "All items · 16" with that team's open
count, which opens `/all?team=<slug>` (All items §7) and which a screen reader hears as "All <team> items, 16 open";
neither is among search results. On `/all` and
its Closed and Activity pages the row opened is marked current, where no heap's row is, and the heap button reads as
that row does, "All heaps" or "All <team> items", on the sidebar and the phone's top bar; the rail's button shows the
layers icon in place of a heap's initial. No heap is the current one there, so the views are the pages across heaps
(All items §8): Open with the count the row shows, Closed and Activity in the sidebar and the rail, with no Data or
Settings; on a phone the Open tab and More's Closed and Activity are those pages, More has no Settings or Data, and
the + still adds to the heap opened last. The heaps' open counts, on Open and in the heap switcher, are fetched with the page and
again every 30 seconds while the tab is visible, on focus and after a change as Waiting for me's count is, so an item
someone else adds or closes reaches them about when it reaches the list.

Pages with no heap in their address keep the heap opened last in the shell, so going back is one tap: the switcher
shows it, the heap's views link to it with none marked active, the tabs show on a phone, and Add adds to it. For now
that is `/waiting`, `/agents` and `/all` with its Closed and Activity, which keep it for Add only (All items, above);
Teams, a team's page and its activity, and Account keep their shell without
one. The heap kept is the one opened last that is still there and active, else the first active heap, else the first.
On a phone those pages have no tabs, so the top bar starts with Back: it goes to the page before, or, when the tab
opened on the page from a link, to the heap kept.

On a wide screen the sidebar and the content are one centred block, at most the sidebar, the content column and the
item pane wide, so the menu stays next to what it opens; the sidebar's colour runs on to the left edge. The block is
that wide whether or not an item is open, so opening or closing one never moves the sidebar or the list's left edge:
an item is opened many times a session. Every page fills the room next to the sidebar, so the content's left and
right edges are the same from one page to the next. On a list that opens items beside it (a heap's list, All items)
the pane opens on the right, so the list narrows from the right. A card fills that room too, and a short field in it keeps the width its value needs.

### Heap settings

**Settings** is one page for a heap's configuration, with a tab each for **General**, **Context**, **Tags**, **Parameters** and **Ranking**.
Each tab is a link with its own path (`/settings`, `/settings/context`, `/settings/tags`, `/settings/parameters`, `/settings/ranking`), so Back steps between
them and a tab can be linked to; the old `/tags` and `/parameters` paths lead to their tabs. General holds the name
and the link name, edited in place, a note that a new link name breaks old links, the heap's history, the Agents card, and Archive heap or Unarchive heap. While the link name is being changed, the new path shows under it ("New
link: /teams/{team}/heaps/{slug}"). Each control shows only where `allowed_actions` allows it; everyone else reads
the values.

The **Context** tab holds the Context card. A heap made from a note says "From: “<note title>”, a note in <heap
name>" above it, linking to the note. It shows the heap's context as Markdown text (see Toolkit), or a muted
line saying what a context holds when there is none. Anyone who can open Settings sees the tab. A member with
`edit_context` sees an Edit button (not on an archived heap), which
turns the card into a form: an editor (see Toolkit) that grows with its text, Save and Cancel. A failed save keeps the form and
the draft open and says why; Escape does nothing, so a long draft is never lost to a stray key. When someone else
saved a different context meanwhile, it shows rendered above the kept draft under "Saved meanwhile", and Save again replaces
it; when only the heap's other settings changed, the message says so and Save again saves the draft. Cancel drops
the draft without asking. Save and Cancel give focus back to Edit.

The **Agents** card, on General under Details, has a switch, "Agents prepare new notes", with the line "Each note added here is
sent to an agent to fill in its context, or to make it a defined task when it's small and clear, then comes back
to you." It saves as it is turned, and can't be turned again until the save ends. Only an admin sees the
switch (`set_agents_prepare`); others, and everyone in an archived heap, read "Agents prepare new notes: on" or
"off".

The **Your priority** card, on General after Agents, is the person's own priority for the heap, apart from the
Ranking tab's factor weights: a row of numbers 1 to 10 (five a row on a phone), the current one filled, with the
line "How this heap ranks against your other heaps in Waiting for me and the agents' queue: its items' scores are
multiplied by this. 1 to 10. Only you see it." Pressing a number saves it at once, as the Agents switch does, and
another can't be saved until that save ends. Everyone who can open Settings may set theirs (`set_priority`),
archived heaps too; an error shows under the card.

The tabs share one look: an intro line, then cards in the design tokens. **Tags** has a New tag card (Name,
Description, Add tag) and a card headed "{n} tags of this heap" whose rows read as the tag's name in a chip and its
description (or "No description"), the chip in the tag's colour; an edit button on the row turns it into the form
(name, description, Colour, Save, Cancel, History), where Colour is a row of eight round swatches, the picked one
ringed, with a chip under them that previews the name in that colour (on a phone two rows of four),
highlighted in the accent. Beside it a ⋯ holds Make a team tag, for anyone who may edit the tag (so not in an archived
heap). A second card, "{n} tags of the team {team}", with a Team page link in its header, lists the
team's tags the same way, with History, and edits them in place: the form adds "Changes in every heap of {team}."
and saves through the team's tag (an archived heap too, as the server allows). A team tag's History reads its made,
edited and merged entries, each naming the tag, and a merge lists each heap's tag and its items: "Launch's “billing”
(12 items), Support's “Billing” (3 items)". Adding or renaming a heap tag to a team tag's name is refused under the
field: "“security” is a team tag of Acme. Use it on items here instead."

**The merge confirm.** A team tag's write that would merge heap tags (Add on the Team page, Create team tag in a team
parameter's picker, Make a team tag, or a rename into a name heap tags have) is refused by the server with the list of
what it would merge, and the app asks first, in a dialog on a wide screen and a bottom sheet on a phone. It is headed
"Make “billing” a team tag?" or "Rename “old” to “new”?", and lists a row per heap tag: the heap, the tag as that heap wrote it, and its item count, with
"archived" and, for Make a team tag, "this heap" marked. Under the rows a total says "{n items} in {m heaps} will
carry the team tag.", then the description the team tag will have, and the quiet line "The items keep their history.
This can't be undone." Cancel and Make team tag (Rename and merge for a rename) end it; while saving the button shows
the saving ring (Rules §6) and nothing can be pressed or dismissed. If the list changed while it was open, the server
refuses the button's write with the new list: the dialog stays, shows "This changed while it was open. Check the list
again; the button confirms this one.", marks rows "new" and counts "was {n}", names what left the list, and the button
confirms the new list. Another failure shows above the buttons, with its request ID, and the dialog stays; a rename
someone else changed meanwhile closes it and shows the form's version-conflict line. Success closes it with a toast,
"“billing” is a team tag now: 2 items in 2 heaps carry it." (for a rename, "Renamed to “billing”: …"). Cancel changes
nothing, and the form keeps what was typed.

**Parameters** opens its add form from a New parameter button, and gives each parameter a
card: the name with Rename, the weight, the options as a numbered list (↑ ↓ and Archive on each; Archive turns the
row into a red confirm that asks where open items go), Add option, the tags that suggest it, and a foot with History
and Archive parameter. Each option opens the heap's list filtered by it (`?param=<option id>`): for an admin, whose
label is an input, a list icon after the label, named "Items with Energy: Low"; for a member who can't edit, the
option row itself. Archived parameters are listed last, and their options aren't links. On a phone everything stacks
in one column, main buttons span the width, and every control is 44 px to a finger.

A heap's own parameter, for an admin, has a ⋯ after Rename with **Make a team parameter**. It sends the write without a
token and the server's refusal opens the **parameter merge confirm**. The confirm is the merge confirm of Tags, with the
same dialog on a wide screen and sheet on a phone, the same "This changed while it was open" state, and the same failure
line. It differs in what it lists: it is headed "Make “Customer tier” a team parameter?", a row per heap parameter (the
starting one first, marked "this heap", then the same-named ones, "archived" where their heap is) shows the heap, the
parameter as that heap wrote it, its item count and, when it brings options the team parameter lacks, "adds Platinum".
When one of its options matches an option the team parameter has archived, the row says "moves Silver → Gold": its
open items get the option in use that Silver was replaced by, while closed ones keep Silver. A row whose parameter
is linked to tags says "links billing, urgent": its heap keeps those links to the team parameter. The total says "{n items} in {m heaps} keep their values.", then "Each heap keeps its weight. Rankings can shift in a heap
whose options were in another order." and the quiet line "The items keep their history. This can't be undone."; the
buttons are Cancel and Make team parameter. Success closes it with a toast, "“Customer tier” is a team
parameter now: 3 items in 2 heaps keep their values.".

A team's parameters a heap uses are in the same list by name, each with the team icon after its name (", team
parameter" to a screen reader), its weight read as "Weight {n}", its options read-only, the team's tags that suggest
it read-only, and the heap's own tags that suggest it, which an admin edits in place (each change is saved at once), a
line saying the weight is set on the Ranking page and the name and options on the Team page (both links), and Stop
using for an admin (an error from the server shows under it). A card headed "{n} parameters of the team {team}", with a
Team page link in
its header, lists the team's others, each with its name, its options and a Use button for an admin; with none it says
"{team} has no other team parameters. Make them on the Team page.". Using one gives it the weight it had, or 10.

**Ranking** (the canvas boards "Wide · Ranking" and "Phone · Ranking") sets how much each factor counts. A Built in
card has a row each for need, age, deadline and value per effort, and a Parameters card, shown when the heap has
parameters in use, has one per parameter, with its options as the description ("Low, then Medium, then High"), each
label a violet link to the heap's list filtered by that option. A team's parameter the heap uses is in the card with
the team icon after its name, and its weight is set here with the others. On a phone the links stay inline, the one place they
are smaller than 44 px. Every option shows, however many lines they take. Between them, a **Tags** card holds the
Tags weight as a factor row ("Tag order") and the heap's tag order as a numbered list, the highest first. Each row
says, dim, what the tag does next to an untagged item ("lifts", "as untagged", "holds back"), and a line under the list
says where untagged items sit ("Untagged items score the same as #2."). Editors get ↑ ↓ and Remove on each row (a ⋯
menu with Remove on a phone) and an "Add a tag to the order" picker of the tags not yet ranked, which adds one at the
bottom; the order is a draft saved with Save weights, counts once in "{n} unsaved" and shows "Order changed" on the
card. Others read the order without buttons, or "No tags ranked." A row shows the factor's name, its
share of the total as a bar and a percentage (plain arithmetic on the weights on screen; "–" when all are 0), and for
editors a slider and a number input that move together. One Save weights writes every changed weight at once; a "was
{n}" chip and an "{n} unsaved" count mark what changed, Cancel (or Escape) puts the weights and the order back, and Reset
all to 10 sets every weight's draft to the default, the Tags weight too, without saving or touching the order. Others read "Weight {n}" and a note that only admins can change it. A
**Top of the list** card, for everyone, shows the first five items as the draft weights and tag order would rank them, each with how
far it moves (↑2, ↓1, New), so the effect shows before saving. Wide: a sticky save bar under the cards and the preview
beside them. Phone: one column, the preview under the cards, the save bar fixed above the bottom tabs.

### Tablet

A tablet gets its own layout rather than a stretched phone or a squeezed wide screen:

1. **The rail.** The sidebar folds into an 80 px rail. Its top button shows the heap's initial and opens the heap
   list as a popover; below it are the heap's views (Open, Closed, Activity, Data, Settings) as icons with short labels;
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
In the heap switcher the check comes before the current row's name, as part of it, so the other names keep one left
edge and the counts sit at the panel's right edge.
The heap switcher groups heaps by team, with archived ones folded per team behind "N archived". Each unarchived heap's
row has its open count on the right, the number its Open view shows, which a screen reader hears as ", 2 open" in the
row's name, as on the All rows; archived heaps and Recent have none. From 8 unarchived heaps it adds a search field that filters as you type, fold headers for the teams, and Recent: the last 4 heaps
opened in this browser, not the one on screen, as chips on a phone. Recent and the folds are kept in localStorage,
since a "last opened" on the server would cost a write on every heap visit;
they don't follow the user to another device. Search gives a flat list with the team and an
"archived" tag, a "New heap “…”" row, and a message when nothing matches. Arrows move, Enter opens and Escape
clears the field, then closes. It ends with "New heap in \<team\>", in the heap's own team, when the team allows
it. "New heap" turns into the name and an optional Link name, with the same new link under them as the Team page's
form ([Teams and Team](#teams-and-team)); Add heap opens the new heap. On a tablet it is a popover beside the rail. The popover, on a tablet or wide screen, has no title bar: it starts
with the search field, or with the list when there are few heaps, and its rows are denser than a sheet's. Each
team's name, and Recent's title, sits on a band a shade off the panel, with space above it, and a team's fold header
stays in view while its heaps scroll, with the team's "All items" link at its right, folded or not.

Gestures are offered where the pointer is coarse (a finger), on phones and tablets alike:

1. A row dragged right past about 100 px slides away and marks the item done; dragged left, it stays open on
   buttons, Agent (when the row's robot would show, The list point 3), In progress and Blocked (Open in place of the
   state it's in), until one is picked or the row is tapped. Each is followed by a toast with Undo. A tap just after a drag doesn't open the item.
2. The item sheet drags down by its top bar to close; the tablet panel drags right. Letting go short of 120 px
   springs it back.
3. On a row that can be placed (The list point 20), a finger held for 400 ms within about 8 px lifts it, and from then
   the finger drags it up or down. A move past 8 px before then ends the press, so a sideways move still swipes (point 1)
   and a vertical one scrolls; a quick tap still opens the item. While a row is lifted the page doesn't scroll and the
   row doesn't swipe. The press opens no link menu or callout.

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
   kept for parameters everywhere, so they read apart from tags (outlined, in their own colours) and due dates.
3. A row starts with three fixed slots, from a tablet up, so titles line up; a slot the person can't use stays
   empty. First the **state button**: Start (a play circle) on an open row, Unblock (a red closed padlock in a
   circle, drawn open while the row is hovered or focused, and always with a finger) on a blocked row, Pause (an
   accent circle, lightly filled, with two bars) on an in-progress row. Start moves the item to in progress; Unblock
   and Pause move it to open. A screen reader hears ", blocked" after a blocked row's title, or ", blocked: <reason>"
   when the item says why in words, and the padlock's tooltip carries the reason too; the row itself shows nothing
   more. Second **Done**, a circle with a check, on every row. Third the **robot**: a button that sends the item to an agent, one step at once, with
   no line, when the server allows it and no agent has the item; a blue robot mark on a blue tint when an agent has
   it, on a solid blue tile while a run is live (queued otherwise), with a small accent dot when it waits for the person
   and, in the corner, a badge for where the turn ends (with context, defined, or done, which is also where a turn sent
   all the way ends), or an eye while it watches a pull request; its tooltip says it ("Running now · going to:
   defined", "Queued · going to: with context", "Watching PR #276 · going to: done", "Waits for you · going to: done",
   "Agent stuck: 3 runs failed"); empty otherwise (a reminder, a blocked item, a reader).
   With a mouse, Done and the robot are muted until the row is hovered or has focus within; with a finger they are
   always in full colour. A quick send shows the toast "Sent to an agent, going to: with context." with Undo (The item
   point 15), gives focus to the row, and a failed one says why in a toast. A phone shows the state button only (44 px)
   and swipes instead (point 5). Without permission to change the state, the buttons are absent and the slots stay
   empty. An undo toast follows every state change from the list: bottom centre, above the tabs on a phone, gone after
   a few seconds. Changes made in the item save silently and have no undo. The toast sits over the open item, so Sent
   to an agent can offer Undo from inside a phone's sheet; the Add dialog and the other sheets stay above it.
4. Tapping or clicking a row opens the item. Keyboard on a wide screen: up and down move, Enter opens, `s` starts
   (an open or blocked item), `b` blocks (an open or in-progress item, with no reason, as the swipe does), `x` marks
   done and `a` sends one step to an agent, where the robot button shows; `N` and `/` stay. Tab goes the state
   button, Done, the robot, then the row's link, then its tags and parameter chips.
5. On a touch screen, swiping a row right marks it done; swiping left offers Agent (where the robot button would
   show, sending one step at once as the button does), In progress and Blocked.
6. Saved views live in the Filters panel, in a Views section that comes first (before Sort by in the phone sheet), not
   in a bar. Tapping a view applies it: its Show settings, sort and filters replace the list's in the URL, the agent
   filters are cleared and the search stays. On a phone this also closes the sheet. A view whose settings equal the
   list's has a check. "Save as view…" names the current settings in place. Each view's ⋯ offers Rename…, Update to
   current settings (when the list's settings, the agent filters aside, differ from it) and Archive. Update and
   Archive close the panel, so their toast shows: "Updated <name>.", or "Archived <name>." with Undo. The URL holds
   the settings, never a view, so links keep what they showed. The controls show only where the server allows them.
   The All items page has no saved views.
7. One Filters panel holds the saved views, what the list shows and its filters: Views (point 6), Show (Open, In progress, Blocked as Pinned, Shown or
   Hidden, a dot on a changed one; shown blocked items in their own group or mixed in; Reminders to come, Hidden or
   Shown, point 16), then Narrow down (tags, with the team's tags beside the heap's and marked with the team icon, need,
   effort, impact, parameters, agent state, going to), with Reset all, "6 of 14 items" and Done. It's a popover from a tablet up and a sheet
   on a phone. The toolbar is search, Sort by, Filters and Select (point 19); on a phone it's search and an icon Filters button, with Select in the top bar, and the
   sheet, "Filters & sort", starts with the views, then the sort and ends with "Show 6 items". The Filters button counts the filters
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
    item that waits for the person is not the agent's, and says so in words (point 13). On a phone a small blue robot mark
    before the title stands for it, on a small solid tile while a run is live, with the icon of where the turn ends after
    it, or the eye while it watches a pull request; a dot alone stands for an item that waits for the person. A screen
    reader hears ", with an agent, running now, going to defined", ", queued, going to with context" or ", watching pull
    request 276, going to done" after the title, and ", agent stuck" after ", waits for you"
    for a stuck one. Who has an item never
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
    in every row keep their colour and get an accent ring. A parameter chip ("Energy: Low") does the same with its
    option: it joins the parameter filter, and is outlined in violet while it's on. Tags and parameters on the item,
    Waiting for me and the closed list are plain text, and so is a chip of an archived parameter.
18. Sort by offers Priority, Due soonest, Most needed, Smallest effort and Largest impact, then each custom parameter
    that has an order. Most needed lists Critical first, then Expected, Nice to have and Idea; items of the same
    need keep priority order.
19. **Selecting and acting on several items** works on a heap's own list only: not on All items, not on Closed, and the
    reminders to come aren't selectable. **Select** shows when the heap isn't archived and its team has another heap
    that isn't. On a tablet or wide screen it is a button in the toolbar after Filters; on a phone it is a text button
    at the end of the top bar, with no item open. Select mode is a history entry, so Back leaves it, as do ✕,
    Escape, and Done, Won't do or a move that took every item; changing a filter or the sort doesn't. Entering it closes an open item
    for good, so leaving lands on the list, and starts with nothing selected.
    In select mode a row's first slot holds a checkbox, Done and the robot are hidden, swipes are off, and the row is
    a checkbox: a click or tap or Space toggles it, Shift-click selects every row between the last one toggled and
    this one, and Shift+↑ and Shift+↓ move to the next row and select it. `x`, `s` and `a` do nothing. Tags and
    parameters in a row still filter the list. On a tablet or wide screen a bar takes the toolbar's place: a box that
    selects or clears every row shown (mixed while some are), "3 selected · Select all 14", and, right-aligned,
    **Start** (**Pause** when every selected item is in progress), **Done**, **Won't do**, **Send to an agent…** and
    **Move to…**, then a ✕ that leaves select mode; the chips of the filters stay, and the bar wraps when it is
    too narrow. On a phone a bar over the top bar has ✕, "3 selected" and Select all (Clear all when all are), and a
    bar over the tabs has one row of equal buttons, an icon over a short label: Done, Won't do, Start or Pause,
    Agent… and Move…, each named in full ("Send to an agent…", "Move to…") for a screen reader. The count is
    announced. Only rows the list shows stay selected: a row a filter or the search hides is dropped from the
    selection and doesn't come back when the filter is taken off.
    Every action button is off while nothing is selected and while a bulk request runs. Done, Won't do, Start,
    Pause and Send go item by item, in the list's order, through the item's own endpoints, so each item is checked on
    its own. Done and Won't do touch every selected item, Pause too; Start touches the ones not in progress, blocked
    ones included (starting drops their blocker). Send gives every item its own next goal. No item is left out for
    not allowing the action: the server refuses it and says why.
    Done and Won't do ask first, in a dialog (a sheet on a phone): "Mark 3 items done?" with "Mark 3 done", and
    "Close 3 items as won't do?" with "Close 3". Start asks only when an agent has some of the items it touches. The
    body says "An agent has 2 of them. This takes them back." when it applies, and otherwise, for Done and Won't do,
    "There is no Undo."; Cancel returns focus to the button. Pause never asks, and Send's form is its question.
    Send to an agent… opens a popover under the button (a sheet on a phone) with "How far": One step or All the way,
    "A line for the agent (optional)", Send and Cancel. Send is off while it runs. Closing the popover or the sheet
    keeps the choice and the line for the next open; Cancel and Send drop them, as does leaving select mode.
    When every item changed, the toast says "Marked 3 items done.", "Closed 3 items as won't do.", "Started 3
    items.", "Paused 3 items." or "Sent 3 items to an agent."; after Done and Won't do select mode ends and the rows
    are gone, and after the others the selection stays. There is no Undo. When some were refused (changed by someone
    else meanwhile, closed, or not allowed), a panel under the button, in the send form's place for Send, says
    "Marked 2 of 4 items done." (or "Closed 2 of 4 items as won't do.", "Started…", "Paused…", "Sent 2 of 4 items to
    an agent.", or "None of the 4 items changed."), then "Not marked done:" (or "Not closed:", "Not started:", "Not
    paused:", "Not sent:") and a line for each, "Title — reason", with **Close**; Close keeps select mode with only
    the refused rows selected. A failed request, on the network or not, counts as that item's refusal.
    Move to… opens the item's Move picker (The item §6) for the selection, with its own team's heaps only. Picking a
    heap asks first, "Move 3 items to Marketing?" and "Tags they don't have there are dropped.", with Cancel and
    "Move 3". When every item moves, the picker closes, select mode ends and the list stays where it is, with the
    moved rows gone; the toast says "Moved 3 items to Marketing." (and "2 got new keys." when some keys changed) and
    offers **Open Marketing**. There is no Undo. Each item moves on its own, so when some are refused (changed by someone else meanwhile, closed, the heap
    archived) the picker shows "Moved 2 of 3 items to Marketing." (or "None of the 3 items moved."), then "Not
    moved:" and a line for each, "Title — reason", with **Close**; Close keeps select mode with only the refused
    rows selected, so Move can be tried again. A request that fails as a whole shows its error above the buttons, and
    nothing moved.
20. **Placing a row by hand** moves an item up or down in the ranked group, "Up next" or the untitled group when
    nothing is pinned, and only in the Priority sort. It is off in any other sort, with text in the search box, in
    select mode, on pinned and blocked rows and reminders to come, and for someone who can't edit the items (a reader,
    an archived heap); tag and parameter filters leave it on, as the server places the item next to the row it is dropped
    by. The item lands in the order of the list on screen, so the row stays where it was put as time passes, and the
    server says which way it moved: the row's first reason is "Moved up" or "Moved down". On a wide screen a grip
    sits at the end of the row after the kind and stage icons, in a slot of its own, quiet until the row is hovered or
    has focus within, and above the row's link, so pressing it never opens the item. Dragging it moves the row with the
    pointer; a 2 px accent line shows where it lands, which is always within the group; the pane scrolls near its edge;
    Escape or a cancelled pointer puts the row back and saves nothing, as for a step. A click or Enter on the grip opens
    a menu, "Move “title”", with Move up and Move down, each left out at its end of the group. Alt+↑ and Alt+↓ on the
    focused row move it one place; the focus stays on the row. On a phone or tablet there is no grip: a long press
    (Phone details, gesture 3) lifts the row, which then follows the finger with the same line and
    scrolling, and lifting the finger drops it. A move that lands where the row was does nothing. A move saves at once,
    and the toast says "Moved “Rotate the certs” up." (or down) with Undo, which takes the placing away when the item had
    none, and otherwise puts the item back next to the row that was above it (below it, when it was first). A move the
    server refuses, because the item changed meanwhile or the row it was dropped by closed, puts the row back and says why.

## The item

1. The item route (`/teams/$team/heaps/$heap/items/$item`) shows the list with the item over it or beside it,
   instead of a separate page. Links and the back button keep working: going back closes the sheet.
2. It opens to read, in this order: the title with the main state button and ⋯ beside it (point 6), a status line, Waits for
   you when something does (point 12), with the open questions and their reply controls, the fields (point 16), the context (point 17), the comments (point 18), a task's Steps (point 11),
   what it came from and became (point 19), Questions, the answered ones and Ask a question (points 13 and 14), Runs (point 20) and History, collapsed until it's clicked, on every item it opens. On a phone the
   main button and ⋯ sit in a bar at the bottom of the sheet instead, the main one wider. The **status line** is one
   quiet line under the title that may wrap: the kind and stage with their icons ("Task · With context"; the kind
   alone when the item is closed), a reminder's date ("Reminder · 12 March"), the state ("In progress" in the
   accent), and for a closed item when it closed, or "It can't be reopened." A blocked item says "Blocked by" and
   links to the item it waits for, which opens in the pane, or just "Blocked" while none is named. A reason in words
   follows: "Blocked: <reason>", or "Blocked by <item>: <reason>" with both. It wraps, never shortened. On a
   blocked item, Add reason… in ⋯ (Change reason… once one is set), under Change blocker…, opens "Why it's blocked"
   (a popover, a sheet on a phone): one line, where Enter saves, and Remove reason or an empty save clears it. A
   counter shows as the text nears the server's limit; past it Save is off and nothing is cut. It can't be closed
   while it saves; a failed save keeps the draft and says why, and a save shows no toast. Closing it keeps the draft
   for the next open; Cancel drops it, and the saved reason shows again. Then
   the chips: the item's place in the list, "#3" (point 9), and who has the item when an agent does, or the button that sends it to
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
   there whenever it holds something, and holds, each only when the server allows it and in this order: the other state
   changes, Won't do, Make it a task…, Split… on a note, Change blocker… and Add reason… (Change reason… once one is
   set) on a blocked item, the Stage choices, and last, in a section of its own, Move to another heap…, when any of the
   person's teams has a heap the item can move to. With none of them, as for a reader's item, an archived heap's or a
   closed item, there is no ⋯.
   Move to another heap… opens "Move to" under ⋯ (a sheet on a phone): the heaps that aren't archived of every team the
   person is in, in groups under the team's name, the item's own team first and the others in the order of Teams, a
   team with none left out, the item's own heap marked "(here)" and disabled, with a search field from 8 heaps in all
   (a group whose heaps all miss it goes) and no New heap. A note heaps were made from lists only its own team's, with
   "Heaps were made from this note, so it moves only within <Team>." under the list. A pick asks first, "Move to
   <Heap>?" and "Tags it doesn't have there are dropped.", with Cancel and Move; in another team it asks "Move to
   <Heap> in <Team>?" and says "Tags and values <Team> doesn't have are dropped.", "Its history stays in <Old team>.
   People in <Team> see it from now on, and don't see its links to <Old team>." and, when someone else sent it to an
   agent, "<Name> sent it to an agent. If <Name> isn't in <Team>, the turn is taken back." (a turn nobody can be
   traced for: "Its turn with an agent is taken back."). It can't be closed while it saves. The item then opens in
   its new heap with the address replaced, so Back doesn't return to the old one, also from All items, and a toast says "Moved to <Heap>." ("Moved to <Heap> in <Team>." across teams) and, when the new
   heap already had its key, the key it got. There's no Undo, as the tags can't come back. A refusal shows in the
   picker, above the buttons; a note heaps were made from says it moves only within its team, and a heap the person
   can't move to any more says "You can't move items to that heap any more."
   Block saves the state at once; the blocker is named after, with Change blocker…, which opens a popover (a sheet on a
   phone) as Move to does: "Find an item", focused, over the heap's other open items in list order, without those that
   wait for this one, which would be a cycle. Typing keeps the titles that hold the text, in any case, or says "No item
   matches “<text>”"; arrow keys move, and Enter or a click saves the pick. Nothing picked comes first while the field
   is empty, and the current blocker (Nothing picked when there's none) is marked "(current)" and can't be picked again.
   The field stays put while the list scrolls. Make it a task asks first, naming the item, since a task can't
   go back to a note (or to a reminder). **Split…** on an open note opens a dialog (a sheet on a phone) with three
   outcomes: Tasks, A step, and A heap, the last only for someone who may make heaps. Tasks and a heap's first items are
   rows of title, need and age, with nothing preselected and the needs above the note's need off; age is Keep the
   note's age or Start new. Into tasks starts with two rows, and Add another adds one. Into a step lists the heap's open
   tasks with a filter and takes the note's title and context as they are, at the end of the task's steps. Into a heap
   starts with the note's title as its name and its context, both editable, and first items that are each a task or a
   note; agents don't prepare what's added. Items are made one at a time and the note is closed last; after a failure
   what was made stays as a locked "Made" row, the note stays open, and Make the rest carries on. A note closed
   meanwhile says so and offers only Close. Leaving a draft asks "Discard this split?". The toast says what the note
   became, with no Undo, as Reopen is the way back, and focus goes to Reopen. A reminder has an exact Date in place of Due (point 16), which can be moved,
   also back into the future, and not cleared; it has no steps and no Send to an agent button. Stage lists as written and with context, and defined on
   a task only (a note says "Defined needs a task"); picking one saves silently, like every change in the item.
7. On a wide screen and a tablet, ↑ ↓ in the pane header walk the list without closing the pane, so triage is open,
   change, next. The arrow keys walk too while the focus is on the pane or its header, not in a field, so Space and
   Page Up / Page Down scroll a long item there. On a phone the sheet closes and the next row is one tap away, so the
   sheet has no Next button.
8. The key (`k3x9a`) in the header, with a link icon after it, is the button that copies the item's short link
   (`/h/<team>/<heap>/<key>`), named "Copy link to k3x9a", with "Copy link" on hover. The header stays in view, so the
   icon turns into a check for 2 s to confirm, and a screen reader hears "Link copied"; a copy that fails (no
   clipboard, or the browser refuses) says "Couldn't copy the link." in a toast. Nothing else copies the item's link.
9. The rank chip, "#3" in the status line, is a button that opens "Why it's #3": a popover (a sheet on a phone) with
   the contributors and their `share_percent` as bars. Below them, "Held back by" lists what keeps the item from
   ranking higher: a low need, no due date, unset sizes, in that order, as lines of text without bars. What can fix
   it shows itself on the fields (point 16): an unset effort, impact or due date it names is marked there. A low
   need isn't marked: the need was set on purpose. The item shown first in its list has no held-back lines, nor does
   a blocker ranked by what it blocks, nor any item while the list is sorted by something other than priority; with
   nothing to say the chip is plain text, and an item that isn't in the list on screen has no chip. The list row
   doesn't show any of this.
10. The item says when it was created and by whom ("Added by Sam 4 days ago"), on the line of History's toggle.
    On a phone, where that line is easy to miss, the fields (point 16) end with an Added row as well.
11. **Steps**, on a task, sit inline at every width (on a phone they are not a row that opens a sheet) under the
    heading "Steps 2/5", with the count of point 1 (just "Steps" when there is none). Each step is a row: a
    grip before the checkbox (editors only; on a wide screen it shows when the row is hovered or has focus, on a phone
    it is always there, 44 px), a checkbox that ticks and unticks it, the title edited in place (Enter or leaving the field saves, Escape or a
    blank title puts it back), a chevron that shows its context (closed to begin with; see below) and ⋯ with Move up and Move down (absent on the first and last step), then Drop, or Restore on a dropped step.
    Only the grip drags a step: the row follows the pointer, a 2 px accent line shows where it will land, every place
    can be reached, the list scrolls when the pointer nears the edge of the pane, and Escape puts the row back. After a
    move from the menu focus stays on that step's ⋯, and a screen reader hears "Moved to place 2 of 5". A dropped step is struck through and muted, with a
    dash in place of its checkbox, and stays in the list. "Add a step" opens a one-line field in place: Enter adds
    the step and keeps the field for the next, leaving it with text adds the step and closes it, Escape closes it
    without adding and puts the focus back on the button. Nothing is optimistic but a move: a change shows once the server has
    it, and one that fails is said under the section; a moved step shows in its new place at once, and goes back if the
    server refuses. A closed item, an archived heap or a missing `edit`
    leaves the steps and the count to read, with no grip, no checkbox to change, no ⋯ and no Add. A step's context opens rendered as
    Markdown text (see Toolkit), for everyone, and the chevron shows only when there is one, or the step can be edited.
    An editor sees a small Edit button under the text ("Edit step context"), which swaps it for the field with the
    source, focused, the caret at the end; leaving the field saves a change and goes back to the rendered text once the save has landed;
    a failed save keeps the field and its draft, and Escape there does nothing, so a draft isn't lost to a stray key. An
    editor opening a step with no context gets the field at once. A refetch doesn't touch what is typed.
12. **Waits for you**, while the item is open and has open questions, a stuck agent turn or a gone blocker, is what
    the item waits on, at its top, under the status line: a tinted, outlined block (the accent at a low fill, a 2 px
    accent border), the same on the pane, the tablet panel and the phone sheet. Most of the time an item is opened to
    answer what waits on it, so it is answered here, with no scroll. A gone blocker is its first row (see Waiting for
    me, 8), with Unblock; a stuck turn comes next (see Waiting for me, 7), with Retry and Take back. The open questions
    follow (point 13), under one line that says "1 question waits for you" or "N questions wait for you", with
    **Skip to details ↓** on its right whenever a question waits, also for one, and on a phone too (44 px; the
    bottom bar keeps only the main button and ⋯). Skip scrolls the fields to the top of the pane or sheet and focuses
    the first of them (the fields themselves when none can be edited); it folds nothing, so scrolling up brings the
    questions back. Opening the item doesn't move focus into the block. The notes a reply can leave (someone else
    replied first, the item was closed first) sit under the block.
13. **Open questions** sit in Waits for you, one row each in number order, separated by a thin rule: a quiet line with the kind ("Question", "Request", "Question · pick any", "Review, round N", "Ready to close") and `· step "<title>"` when it
    names one, the short text in semibold, "Details ▸" when it has details (opening them in place), then the reply
    controls. A question without choices has an "Answer" field and Reply, off until the answer has text. With choices
    it has radios (one) or checkboxes (any), "Comment, optional" and Reply, off until something is picked. A request
    has "Comment, optional" and two buttons, Done and Can't be done, which reply as soon as they are pressed, sharing
    one row on a phone. A review and a done proposal are rows as in Waiting for me, 9 and 10: a review has its link
    and piece under the text and Approved and Changes asked; a done proposal has its summary line and Close as done
    and Not yet, and its toast has no Open, as the item is open already. On a phone Reply spans the width, and every control is 44 px. After a reply there is no toast (but see Close as done):
    the row leaves the block and joins the fold (point 14), the focus goes to the next row's first control, else to the
    item, and a polite live region says Replied to “<text>”; after the last one the count line and Skip go, and the
    block with them unless a gone blocker or a stuck turn is in it. While it is sending, that row's buttons are off. If it
    fails the row stays with its picks and comment and the reason shows under it. If someone else replied first, the
    row leaves and "Someone else replied to “…” meanwhile." shows until the next reply. In an archived
    heap a row shows its text and details with no controls, and Skip still shows. A closed item has no open rows:
    closing withdrew its open questions, which read in the fold as "Withdrawn".
    **Questions** is a section under the Steps, shown when the item has any question, or is open and the person may
    ask one (see **Ask a question** below), under the heading "Questions". While questions are open it starts with a
    quiet link, "N open, at the top ↑", which scrolls up to the first of them and focuses its first control (the
    open questions themselves when it has none), for
    someone who is at the bottom, after asking, say. The fold of point 14 comes next.
    **Ask a question** ends the section, after the fold, as a quiet text button with a plus, on an open item in an
    unarchived heap for a member (no button for a viewer, a closed item or an archived heap); with no questions the
    section is only its heading and the button, and on a phone the button spans the width. It opens a form in place,
    at every width, with the focus in the Question field: **Kind** (Question or Request), a one-line **Question** field
    ("What to do" for a request) of 1 to 300 characters with a counter from 250, **Details, optional** as Markdown
    text up to 20,000 characters with a counter from 19,000, and for a question **Answer**: In words, Pick one or
    Pick any. Pick one and Pick any show choice fields, two to begin with, with **Add a choice** up to ten, a ×
    on each when there are more than two, and Enter in a field adding the next one rather than sending; blank fields
    are left out, and two choices equal ignoring case say "Two choices are the same." On a task with steps that are
    not dropped, **About a step** picks one, "No step" to begin with. Switching Kind keeps what was typed. **Ask** is
    off until the text, the details and the choices fit, and while sending; **Cancel** closes the form and drops the
    draft. On a phone Ask is over Cancel, each the full width. After asking nothing shows ahead of the server: the row
    joins Waits for you at the top, "1 open, at the top ↑" appears, the form closes, the focus returns to the button, and a polite live region
    says Asked “<text>”. Escape closes the form while nothing is typed; with a draft it does nothing, and the item
    stays open. If it fails the form stays with its draft and the reason shows under it (an item closed meanwhile
    says "The item was closed meanwhile.").
14. The fold in Questions, **N answered**, is closed to begin with and lists the answered questions and the
    withdrawn ones in number order: the kind line, the short text, Details ▸, then `→ <picked labels>` on its line, the comment below it as Markdown text (see Toolkit; with no picks, the comment alone); a review also shows its link and piece under the text, and "<who>, <when>" in quiet text, or a muted "Withdrawn". It reads "N closed" when none is answered
    and all are withdrawn. On an open item each answered question has **Change reply** under its reply (none for a
    viewer, in an archived heap or on a withdrawn question). It opens the reply controls of an open question, filled
    with the current reply, with **Cancel**; sending shows the new reply, and Cancel drops the draft. After either,
    focus returns to Change reply, or to the row when the change closed the item (Not yet to Close as done); after a change a polite live region says Changed the reply to “<text>”. If it fails the
    controls stay with the draft and the reason shows under them.
15. **Who has it** is a chip in the status line, shown only while an agent has the item: "Agent · to with context" (or "Agent ·
    all the way" for a turn sent all the way, whose goal is only its first stop), "Agent · waiting for you", or "Agent stuck" in the amber tone. It opens a popover (a sheet on a phone) titled "Who has it"
    with "With an agent since 14:02" (the time today, else the date as the server words it: "yesterday", "3 Nov"),
    "going to: <state>" and ", all the way" for that reach; while it watches a pull request, "Watching PR #276 since
    14:05. It comes back on a new comment, a failed check, a merge conflict, a merge or a close.", the PR a link that opens in a new tab;
    the person's line in quotes, and "Prepared on add" for an
    item the heap's setting sent. If the item waits for the person it reads "An agent is waiting for your answer,
    going to: …", and if its turn is stuck, "An agent is stuck, going to: …." with no buttons: Retry and Take back are
    in Waits for you. **Take back** is in the popover, and asks first, "Take this back from the agent? It stops, and
    what it changed stays.", inline in the popover and in a sheet on a phone; afterwards the chip is gone and the
    focus goes to ⋯, with no toast. With no agent there is no chip and nothing says "With you". Before an agent has the
    item, the same place holds the **Send to an agent** button (a robot and those words, in the chip's style), shown
    when the server allows it: not on a reminder, a closed item, in an archived heap, or on a **blocked item**, which
    can't be sent until it is unblocked (the server refuses it: "This item is blocked. Unblock it before sending it to
    an agent."). It opens a form: a popover under the button on a wide screen and a tablet, a sheet on
    a phone. The form has the choices "To <where it goes>" (the next stable state: with context, defined or done), on a
    note as written also "To defined", and "All the way", or on a defined task the one line "To done"; "A line for
    the agent (optional)"; Send and Cancel. Cancel returns focus to the button and drops the line and the choice;
    closing the popover or the sheet keeps them for the next open. Send acts at once, closes the form, gives focus to the chip that
    replaces the button and shows a toast, "Sent to an agent, going to: with context.", with Undo, which takes it back
    without asking or a second toast. The list row's robot sends the same way but for one step with no line, and shows the same toast. A failed send or take back
    says why in the popover, and the form keeps its line. Someone else's send first says "Someone already sent this
    to an agent." and the chip shows it. Start, Done and Won't do in the item ask before they take an item back ("An
    agent has this. Starting it takes it back. Take it back and start it?"), inline under the title or in a sheet;
    Blocked and Open don't.
    **Changes asked…** is a chip beside that place, shown beside the Send button when the server offers it: an
    agent brought the item to a stable state short of done and nobody has sent it on. It opens a form like Send's,
    titled "Changes asked": "An agent brought this to: <stage>.", a required "What's wrong" field, **Send back**, off
    until the field has text, and Cancel, sharing a row on a phone. Send back closes the form, shows the toast "Sent
    back to the agent.", and gives focus to the agent chip, which the new turn brings; Cancel returns focus to the
    button and drops the draft, without asking; closing the popover or the sheet keeps it for the next open. A failed send keeps
    the form and its text and says why under the buttons; if someone sent the item on first, the form closes and a toast says "The agent's work changed meanwhile: there's nothing to
    send back.", the focus goes to the agent chip, and the Changes asked… chip is gone. A reader sees no chip.
16. **Fields** are labelled values that open their editor on a click. Need, Effort and Impact share a row of three
    columns, each a small label over its value; Due (a reminder's Date), Tags and each parameter follow, one row
    each, with the label on the left. On a phone the last row is Added, "Sam · 4 days ago" ("You · …" for your own,
    the age alone when nobody can be named), in plain text that
    never opens an editor. Wider screens show it only on History's line (point 10). A parameter's label is violet. An unset effort, impact or due date reads "Set
    effort", "Set impact", "Set due" (a parameter "Set <name>") in muted text; when the server's held-back lines
    (point 9) name it, it is marked with an amber pill. With a mouse a value has no border at rest, a tint and ▾ when
    pointed at or focused; on a touch screen the ▾ always shows and the value is 44 px tall. A click opens a popover
    under the value on a wide screen and a tablet, a sheet on a phone: the need's four steps, S M L XL (pressing the
    picked one clears it), the due choices (staying open while an exact date is chosen), the date or the parameter's
    options. A pick saves at once and closes it, and focus goes back to the value. Tags are the exception, as the one
    field with many values: a click turns the row itself into the tag picker, on every screen, its box focused, with
    no popover or sheet. It offers the team's tags beside the heap's, marked with the team icon and each with a small
    dot in its colour, and Create makes a heap tag, which the server colours. Its matches float under it on a wide
    screen and a tablet, and sit in the flow in a phone's item sheet. Each add, create or remove saves at once and
    keeps it open for the next; it has no Done. Escape or Enter in the empty box closes it with focus back on the
    value (Escape with text only clears it), and so does leaving it, with focus left where it went. Without `edit`
    (a reader, an archived heap, a closed item) the values are plain text.
17. The **context** is the item's description rendered as Markdown text (see Toolkit), with an Edit button, shown
    with `edit`. With none it says "No context yet." and the
    button reads Add. Edit swaps the text in place for a form with Save and Cancel, and on a phone opens the same form
    in a sheet; it reads and behaves as the heap's Context card does (Heap settings). ⌘ or Ctrl with Enter saves, and
    Escape does nothing, so a long draft is never lost to a stray key or closes the item. Save closes the form once
    the save has landed, the text is read-only while it is in flight, and a failed save keeps the draft and says why.
    When someone else saved a different context meanwhile, it shows above the draft under "Saved meanwhile", Save
    reads Save again and replaces it, and Cancel keeps theirs; Cancel always drops the draft, without asking.
    Until Save again, nothing else saves over theirs (Done doesn't go on, and leaving doesn't save the draft). Dismissing the phone's sheet (✕ or a tap outside)
    with a changed draft saves it like Save, and stays open if that doesn't land; an unchanged draft just closes. Done
    and Won't do from the pane save a changed draft first, and don't go on if it can't be saved. Leaving the item with
    a changed draft, by closing it or walking on, saves it when nothing else changed meanwhile, and otherwise doesn't
    save it; either way a toast says the context for that item wasn't saved, and why. When the item can no longer be edited (closed
    elsewhere, an archived heap, a changed role) the draft stays as read-only text that can be selected, with "This item
    can't be edited any more; your text wasn't saved." and a Close button.

18. **Comments n**, under the context at every width and always open (not a disclosure, not a sheet row), before
    the Steps, lists the item's comments oldest first, each in its own bordered card on a sunken fill, so where one
    ends and the next starts shows at a glance. Each is a line, the name in bold then " · <time>" (the time today,
    else the date as the server words it), and the text under it rendered as Markdown text (see Toolkit). A comment
    an agent wrote in a run reads as the agent's, as its entry in the history does (point 21): the card is tinted
    with the agents' colour, and in place of the name are a robot and "Your agent" or "<name>'s agent", a link to
    the run in Runs; a comment is never edited or deleted, so a row has no controls. The heading is
    "Comments 3", or just "Comments" when there are none. Under the list, where the server offers `comment` (also on
    a closed item), is the field "Add a comment" that grows with its text, and a Comment button, off until the field
    has text and while it is sending; on a phone the button spans the width and is 44 px, on wider screens it is at
    the right. Nothing is optimistic: the comment shows from the server's answer, the field clears, the focus stays
    in it and a polite live region says "Comment added". If it fails, the field keeps its text and the reason shows
    under it. What is typed lives apart from the item, so another edit saving or a refetch doesn't touch it. Where
    `comment` isn't offered (an archived heap) the comments read with no field, and with none the section is absent.
19. **From and Became** sit above Questions, only when they apply, and the items they name open in the pane. An item made
    from another says "From: “<title>”, a note". A note that was split says "Became: 3 tasks" ("items" when the
    kinds are mixed) and lists each, with its state. A note that became a step says "Became: a step" and lists "A step
    of “<task title>”", with the step's state. A note that became a heap says "Became: a heap" when that is all it
    became, and lists "Heap “<name>”", linking to the heap's list. A step made from a note says "From: “<title>”, a
    note" under its context. Each links to the heap that item is in now, and both show read-only too.
20. **Runs** is a folded section after Questions and before History, shown only when the item has runs, under the
    heading "Runs 3 · $1.20": the count and the runs' total cost, with no cost part when no run reported one. Opened,
    a row for each run, newest first: when it started (the time today, else the date as the server words it), "from →
    to" (the stage it started at and the goal), how it ended ("reached", "asked", "failed", "taken back", "handed
    back", or "running"), the tool and model that ran it ("claude · claude-opus-5-5") when recorded, and its cost on
    the right, blank when unknown. Under it the result or the failure, and the links. It only reads, for every role, so it has no controls. Each run can be linked to (point 21).
21. **History** marks a change an agent made in a run: a robot and "Your agent" (or "Ana's agent") in place of the
    name, linking to the run: the item opens with Runs unfolded at that run, tinted for 2 seconds. Without access to
    the run's item, the mark shows with no link. A person's change shows as before. The heap's Activity marks an
    agent's change the same way.
22. **History shows an edited text as a word diff**: an item's title and context, a step's title and context, and a
    heap's context, in the item's History, the heap's and Activity. Removed words are red and struck through, added
    ones teal, both on a soft background, in light and dark; line breaks are kept. These are overdue's red and done's
    teal, an exception to one colour meaning one thing, agreed for the diff on its mockup; they are their own tokens,
    so they can move apart. An unchanged stretch over 24 words folds to the 8 words next to each edit, and the rest
    is a button, "… N more words", that unfolds that stretch. A change of spacing alone reads "Only spacing changed";
    spacing changed beside a word edit shows as the new spacing. Where there is nothing to diff (set from nothing,
    cleared, any other field) or under 40% of the longer version's words stayed the same, the change shows as before:
    the old value struck through, an arrow, the new one.
23. **Full screen** gives a long item room to read. On a tablet or wide screen the header has a button between the
    key and ✕, "Full screen" (⤢), which becomes "Exit full screen" (⤡). Full screen is the item's address with
    `full=1` in the query (`/all?item=…&full=1` in All items), so a reload or a shared link opens it the same way. It
    covers the area beside the sidebar or the rail. The header spans it, and the item's content is the pane's, in a
    column at most 720 px wide. Going full screen from the pane is a step in the history. Escape or ⤡ goes back
    to the pane, ✕ goes to the list, and Back goes one step back. ↑ and ↓ keep walking the list in full screen, and
    leaving after a walk shows the pane of the item walked to. The list stays under it, so it keeps its scroll
    position and selection. A phone has no button, as the sheet already fills most of the screen. A full link opened
    on a phone shows the sheet at full height.
24. An item that was placed by hand (The list point 20) says so in a muted line under the status line: "Moved up in the
    list" or "Moved down in the list", with a small **Reset** for someone who can edit it. Reset puts the item back by
    its fields and says "Reset its place." in a toast with Undo, which places it next to the row it had above it (below
    it, when it was first) in the ranked group on screen; with the item not in that group, the toast has no Undo. A
    closed item shows no line, and the line goes with the placing.

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
its row, every item whose agent turn is stuck, and every blocked item whose blocker is gone, with the ways out.

1. The heading is "Waiting for me · 5" (no count at zero) and under it a quiet line, "All heaps, most important
   first". The order is the server's: the item's score in its own heap, then heap name, position and question number.
   Rows are one column at the app's content width, separated by thin rules; no item pane opens beside the page.
2. A row is the item's open-question row in Waits for you, the same component, with a head line of its own: the kind ("Request",
   "Question", "Question · pick any", "Review, round N", "Ready to close"), the item's title as an accent, underlined link to the item in its heap, the heap's
   name, the team's name only when the person is in more than one team, and `step "<title>"` when the question names
   one. Then the short text, and the reply controls as in the item. A row whose question lost its `reply` action
   shows no controls.
3. **Details ▸** holds the details as Markdown text and under them **Context ▸**, which unfolds the item's context and the
   heap's, rendered as Markdown text too, each under a small heading ("This item", the heap's name), or "No context". They are fetched when Context is
   opened, not with the page. A question without details shows Context alone.
4. After a reply there is no toast (but see 10): the row leaves, the count drops, the focus goes to the row that took its place if it
   was lost with the row, else to the heading, and a polite live region says Replied to “<text>”. If it fails the row
   stays with its picks and comment and the reason shows under it. If someone else replied first, or the item closed
   first, the row leaves and a note at the top of the list says so; the closed case keeps the comment so it can be
   copied. The notes stay until the next reply or until the person leaves the page. Drafts belong to their row and
   survive a refresh.
5. On a phone every control is 44 px, Reply spans the width, and Done and Can't be done share a row at half the width
   each, as do the two buttons of a review and of a done proposal.
6. Loading says "Loading…". With nothing waiting the page says "Nothing waits for you." and, quiet under it, "Questions
   from agents and teammates, and work whose blocker was closed, show up here." A failed refresh keeps the rows. Past 500 rows the first 500 show, with
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
8. **A blocker row** is a blocked item whose blocker was closed as won't do or became, moved to another heap, or closed as done
   without freeing it (it was blocked before that rule, or its blocker had moved away)
   ([architecture.md](architecture.md#cycle-detection-and-locking) §3): it stays blocked, hidden from the list, until
   the person moves it on. It is ranked and counted as a stuck row is. Its head line reads "Blocker closed · <item> ·
   <heap> · <team>", or "Blocker moved · …" after a move; under it "Waits for “<blocker>”, closed as won't do" (or
   "which became other items", or "moved to <heap>"), the blocker's title a link to it, in the heap it is in now. Then **Unblock**, the primary
   button, which moves the item to open at once, and **Open**, which opens the item to close it or pick another
   blocker; closing it comes back here. Unblock makes the row leave as Retry does, and the live region says
   "Unblocked “<title>”."; if the item changed first, the note says "Someone else changed this meanwhile." In the item's
   Waits for you the same row has Unblock only, as the item is open already. A reader sees the row without Unblock. On
   a phone both buttons are 44 px, sharing a row.
9. **A review row** is a question an agent asked for a link to its work. After the short text comes the link, an
   accent, underlined "↗ example.test/pull/7" (without `https://`, wrapping when long) that opens the work in a new
   tab, and, in quiet text, "Piece: <piece>" when the piece differs from the link. Then "Comment, optional" and two
   buttons, **Approved**, the primary one, and **Changes asked**, which reply as soon as they are pressed. Approving
   never closes the task: the agent goes on, and proposes closing when nothing is left. The agent asking again after
   changes asked is the next round, "Review, round 2".
10. **A done-proposal row** ("Ready to close") is the agent saying nothing is left. Under the short text a quiet
    summary line: the steps, "Steps 3/3", and the reviews by their latest round per piece, "review approved",
    "reviews: 2, both approved", "reviews: 3, 2 approved" or "no reviews", joined by " · ". The item is fetched for it,
    one request per proposal; until it arrives, or if it fails, the line is left out. Then the comment, "Comment,
    optional: what is left, if not yet", and **Close as done**, the primary button, and **Not yet**, which sends the
    task back to the agent with the comment. Close as done is the only reply with a toast: "“<title>” is done." with
    **Open**, which opens the closed item (Reopen is there). Not yet has none.

## Agents

The page at `/agents`: what agents finished across the person's active heaps. Your decision is the items an agent
brought to a stable state that now wait for the person; under it is the history of the runs that ended as reached.

1. The heading is "Agents" and under it a quiet line, "What agents finished, across your heaps." Everything is one
   column at the app's content width, as Waiting for me is, and no item pane opens beside the page. A title opens its
   item over the page, and closing it comes back here. A heap is named by its name, with its team's only when the
   person is in more than one team.
2. **Your decision** is left out when nothing is in it. Its heading is a button, "Your decision · 4", with a chevron
   that turns when the section is folded; folding hides the rows and keeps the count, and the person's choice is
   remembered in this browser. A row is the item's title as an accent, underlined link, its heap, the item in words
   ("note, with context", "task, defined"), or, for a note that became other items, what it made ("made 3 tasks",
   "made the heap “Blog move” and 2 tasks"), and how long ago the agent finished it ("today", "yesterday", "4 days
   ago"). A row has no buttons: the item is where the person sends it on, starts it or closes it.
3. A row is there while the agent's work is the last thing that happened to the item: its latest turn ended as
   reached by a run that ended as reached, the item is not in progress or closed, and it is still at the state the run
   reached. Sending it on, starting it, closing it or setting its stage back takes it out. A turn the person reached
   (by setting the stage, or agreeing to a done proposal), or one a claim found already met, has no such run and no
   row, and a note that became a step of a task has none, as the task already existed. A note that became items or
   heaps is one row standing for them, until any of what it made is sent on, started or closed (being blocked does
   not take it out). The order is Waiting for me's: the item's score in its own heap times the person's priority for
   that heap, highest first, then the heap and the position; a note that became items takes the best of its made items
   that are ranked, and sorts as an unranked item when none is.
4. The first 5 rows show, and under them "Show N more" unfolds the rest in place and "Show less" folds them back. A
   reload starts folded again. Past 500 rows the first 500 show, with "Showing the 500 most important of n."
5. **History** lists the runs that ended as reached in the last 30 days, newest first, grouped by day with a heading
   ("Today", "Yesterday", "12 Nov"; the day is the one in the person's time zone). A row is the time, the item's
   title as a link and its heap, then "reached: with context", "reached: defined" or "reached: became", the result
   cut to two lines, and the run's links. When the run went past its turn's goal the state says so: "reached: defined
   (skipped with context)"; a run that ended before the state was recorded says "reached" alone. A link to a pull
   request or merge request reads "↗ pull request"; any other shows its address without `https://` or `http://`. Links open in a new
   tab and the title opens the item. Turns the person reached, and asked or failed runs, are not here: asked and
   failed runs are on the item. A task an agent proposed closing and the person closed has no reached run, so it is
   not in History.
6. Fifty runs load at a time, and "Load more" under the last day loads the next. When the last page has loaded the
   page says, quiet, "That's the last 30 days. Older runs are on their items." Cost is on the item, not here.
7. Loading says "Loading…". With nothing in Your decision and no run in the history the page says "No agent has
   finished anything yet." and, under it, "Start one in a terminal:" with `upheap work run` as code. Both lists and the
   count are fetched again every 30 seconds while the tab is visible, and on focus; a failed refresh keeps the rows.
8. On a phone a row takes two or three lines: the title with the time or how long ago, then the heap and the state,
   then the result. Rows and buttons are at least 44 px tall.

## All items

The page at `/all`: every open item of the person's active heaps in one ranked list, the server's (`GET /me/items`:
the item's score in its heap times the person's priority for that heap). It has no sort, saved views, heap filter,
position numbers or reminders to come, and a tag or parameter filter only with a team chosen (§7).

1. The heading is "All items · 12" (no count at zero) and under it a quiet line, "All your heaps, most important
   first." The groups are the list's: the pinned one titled as there, "Up next" only under a pinned one, and Blocked.
   Past 500 rows, "Showing the 500 most important of n." sits under the list.
2. A row is the list's row, with the name of its heap, quiet, after the tags on a tablet or wide screen and first on the
   second line on a phone; the team's name follows after " · " only when the person is in more than one team. It shows
   no tag or parameter chips. Start, Pause, Unblock, Done, the robot, `x`, `s`,
   `a`, the swipes and Undo work as in the list, in the item's own heap. Done offers no start of the next item.
3. An item opens over the list as on a heap's list (a pane, a panel, a sheet), and nothing switches heap. The address
   is `/all?item=<team>/<heap>/<key>`, as a key is only unique within its heap, and a value that isn't that opens
   nothing. The pane's ↑ and ↓ walk the rows on screen, across heaps, and its position is the one in this list. Closing goes back, or to `/all` with the rest of
   the search, and gives the focus to the row.
4. The toolbar is the search and Filters. The panel has Show (Open, In progress and Blocked, each Pinned, Shown or
   Hidden, and the shown blocked items grouped or mixed in) and Narrow down: Need, Effort, Impact, Agent and Going to (with a team chosen, also Tags and the team's parameters, §7). Its chips, Reset
   all and the hidden note are the list's. The panel can't say how many items the filters leave out of the total, so it
   shows the number the view matches. The view is in the address.
5. Add opens the Add dialog in the heap the shell keeps (the one opened last), with the need, effort and impact
   filters filled in, and the dialog's title can change the heap. It is disabled when there is no such heap or it is
   archived. On a phone the + in the tabs does the same, with no view. Once added, the toast names the heap
   ("Added “X” to Launch."), not a place, as the item's place in its heap's list isn't its place here, and Open it opens
   the item over this page.
6. Loading says "Loading…"; with nothing open, "Nothing open in your heaps."; with filters or a search that leave
   nothing, "No items match."; a failed first load shows its message. A failed refresh keeps the rows. The list is
   fetched again every 30 seconds and after any change that changes a heap's list, and keeps its order within the
   session, as a heap's list does.
7. The list can be limited to one team's heaps: `/all?team=<slug>`. The team is in the address only, never remembered
   elsewhere, and choosing or removing it is a history step, so Back returns to the list as it was; every other
   change of the view replaces the entry. A person in more than one team has a first row in Filters' Narrow down,
   "Team": All teams, then each team by its name in `me.teams` order, with All teams checked when none is set. A person
   in one team sees no such row, and nothing else changes for them, except that a link with their own team in it works
   and shows the chip. With a team of the person's chosen, a chip "Team: <team>" takes it off, shown even when nothing else
   is changed; Reset all leaves the team, and it isn't counted as a filter. The heading stays "All items · n" with n the
   team's count; the line under it reads "<team>'s heaps, most important first." ("Your personal team's heaps, most
   important first." for a personal team); rows show the heap's name alone, with no " · <team>". Add goes to the kept
   heap if it is in that team, else the team's first active heap, and is disabled when the team has none, as the phone's + and
   the shell's heap follow. "Nothing open in your heaps." stays the empty line. With a team chosen, Narrow down also
   has "Tags (any)", the team's tags where a heap list shows its tags, and, after Impact, a row for each team parameter
   in use ("<name> (any)"), with the "(archived, ignored)" rows for chosen options of archived ones, all as a heap
   list shows them; their chips and the count in Filters work as there. Choosing another team or All teams drops
   them. A heap that doesn't use a parameter chosen has no rows.
8. Closed and Activity go with the list, at `/all/closed` and `/all/activity`, for every heap or, with
   `?team=<slug>`, one team's, reached from the views while on any of the three (Layout, All items). **Closed** is a
   heap's Closed page (Closed) over the same heaps, most recently closed first across them: each row ends with its
   heap's name, and its team's after " · " when the person is in more than one and none is chosen. Reopen reopens the
   item in its own heap, and an item opens over its own heap's list, as from that heap's Closed. **Activity** is a
   heap's Activity page over the same heaps, newest first across them, each entry showing what that heap's own
   Activity shows the person, by their role in its team, with the heap named after the time the same way. The line
   under either heading names whose heaps they are, as the list's does. Neither has filters. Archived heaps have
   their closed items and activity on their own pages only.
9. A team the person isn't in (never joined, left, or no such team) is the page's error with "You're not in this team."
   in place of the server's message, with no controls and no fallback to all teams; Closed and Activity show the
   team's not-found page.

## Closed

Done, won't-do and became items, most recently closed first, under Today, This week and Earlier from each item's
`closed_group`. One intro line explains the week limit. Each row has the title, which opens the item, "Done · today
by you", "Won't do · 3 Nov by Sam" or "Became 3 tasks · today by you" ("items" when the kinds are mixed; "Became a step of “<task title>”" for a note that became a step; "Became heap “<name>”" for one that became a heap), and Reopen while the item is inside its week, 44 px on a phone; once it can't
be reopened the row shows nothing more, with no per-row dates or "closed for good" labels. Load more pages. A
closed item opens over the Open list, and closing it returns to Closed.

## Data

What agents cost and how work flows, for a team or one of its heaps. The address is `/teams/{team}/data?heap={slug}&days=7|30|90`:
no `heap` means all the team's heaps, archived ones included; no `days` means 30. Every choice is in the address, and
changing one is a step of the history. It is reached from the heap's views (Data, after Activity, in the sidebar, the
rail and the phone's More), which opens it on that heap, and from the Team page's ⋯ (Data, after Activity), which opens
it on all heaps. Any member of the team sees it. On the page the shell keeps the heap opened last, as on Waiting for me.

Under the title ("Data", and "<team> · what happened, by day.", by week for 90 days) come a heap filter and a period
switch (7, 30, 90 days). The heap filter lists All heaps, the active heaps by name, then Archived under its own
heading; it is a popover on a tablet or wide screen and a sheet on a phone, with a check on the current one. Then four
totals (Agents' spend, Items added, Items closed, Agent runs: four in a row, two by two on a phone), and four charts
(Agents' spend, Items added, Items closed, Agent runs: two by two, one column on a phone). Closed and runs are stacked
bars: done, won't do, became; reached, asked, failed, other. Under the spend total, "N runs reported no cost" when runs
that ended reported none.

The days are the person's own, from their time zone: one bar per day for 7 and 30 days, the last being today, and one
per week, from Monday, for 90 days (13 weeks, so the first reaches back 85 to 91 days). The last bar is the day or week
so far, and says so in its tooltip ("14 Nov · so far"). Days without anything keep their place with no bar. When
nothing happened in the whole period each chart says "Nothing in these 30 days." ("13 weeks" for 90 days).

What each figure counts: spend is the cost of the runs that started in the bucket (a run with no cost adds nothing);
added is items created in it; closed is items whose latest close is in it, by how they closed (a reopened item stops
counting until it closes again); runs are by how they ended, "other" being taken back, handed back, stopped by the usage limit and still running.
An item counts in the heap it is in now, for all its past.

Hovering or focusing a bar (arrow keys move between bars) shows its values, zeros included. On a phone a tap on a bar
puts its values in a line under the chart's title instead. Each chart has its title as its name and its numbers in a
table for screen readers. While another choice loads, the figures stay, dimmed; the first load shows the frames with
"Loading…", and a failed first load shows the problem with Try again.

## Teams and Team

Teams lists the user's teams ("Admin · 4 members · 2 heaps", "Member · Groups · 12 members · 2 heaps" for a
group-managed team, "Only you · 1 heap" for the personal team) with Open and,
for a person in more than one team, All items beside it, which opens `/all?team=<slug>` (All items §7),
and + Create team, which opens a form with Name and an optional Link name; the server's errors show under the field
they name. After sign-up, the empty heap offers the next two steps, Add your first item and Create a team; once a
team exists, the Team page itself shows New heap and Add member, with no extra prompt.

Among other teams, the personal team shows everywhere as "<name>'s team", by the user's display name whatever it is
stored as, so it has no Rename. The label is the web app's: the stored name is unchanged, because storing "\<name\>'s team" would need a migration and wouldn't
follow a display-name change. Activity entries name the team as stored.

The Team page follows the settings layout ([Rules](#rules) §3): the name with the rarer actions (Rename, Link name,
Activity, Data) under a ⋯, then Heaps with + New heap (a form with the name and an optional Link name, as Create team
has; once a link name is typed, "New link: /teams/{team}/heaps/{slug}" shows under them, and a blank one is made by the
server from the name; the server's errors show under the field they name) and rows with their open count, Open and a ⋯ (Rename, Link name,
Archive or Unarchive, History), then Tags, the team's own, used in every heap of the team (a New team tag card, and
rows that are edited in place, with "Changes in every heap of {team}." under the form and the same History; a name
that heap tags of the team have opens the merge confirm, see Tags), then Parameters, the team's own, which each heap
chooses to use (a New parameter button that opens the add form without a weight, and a card per parameter like a
heap's, with the team tags that suggest it, picked from the team's tags alone or made there (Create
team tag “…”, which links the new team tag), a line "Used in Launch and Support" or "Not used in any heap yet"; archiving an option there names the heaps whose open items switch), then Members with a
search whose results are a listbox under it (arrows move, Enter adds), member rows with an initials avatar, role badge,
Make admin or Make member and Remove, and Leave team or a line saying why the only admin can't leave. Removing a member
says what happens: "They lose access to Launch and
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
invite link" and "Sam joined through Ana's invite link". It marks an agent's change like the heap's Activity does.

## Accounts

Accounts is the page of an operator of the server, the person the server's `user make-operator` command made one. It
lists every account, so on a server without mail they can make a reset link for someone who forgot their password,
and turn a local account off and on. Anyone else sees no entry and gets the not-found page at `/accounts`, since the
server answers `404` to everything on it.

**Where it sits.** An entry with a shield icon, shown only to an operator: in the sidebar and the tablet's rail above
Teams and Account, and on a phone as a row in More above Teams. Like Teams and Account, the page keeps the shell
without a heap.

**The page** follows the settings layout ([Rules](#rules) §3): the title "Accounts" and one line, "Every account on
this server.", a search field and a filter (All accounts, Active, Deactivated, Unconfirmed, Operators) as a select, then
the rows. The server sends the whole list, ordered by name, and the page searches (name, username and email, ignoring
case, as it is typed) and filters it in the browser, since a server has tens to a few hundred accounts. It shows the
first 50 matches, and **Load more** adds 50. No match says "No accounts match."; loading and errors read as on the other
settings pages. A row is the Team page's member row: initials avatar, name, "@username · email", small labels for
Provider, Unconfirmed, Deactivated and Operator, the last sign-in on the right ("3 days ago" as the Sign-ins card says
it, or "Never"), and a ⋯. A deactivated account is dimmed and stays in the list.

**Actions** are in the ⋯, a menu on a wide screen and a tablet and a sheet on a phone:

- **Make reset link**, for a local, active account other than your own, and only while password sign-in is on. The link
  opens under the row (in the sheet on a phone) in a read-only field with **Copy**, as an invite link's does, and a
  copy the browser refuses leaves the link in the field; **Done** closes it. The link is shown once and kept nowhere.
- **Deactivate…**, for a local, active account other than your own, asks first, inline under the row (in the sheet on a
  phone), with what ends: "Turn off Sam Ortiz's account? 2 sign-ins and 1 personal token end. Their items and teams
  stay." with **Deactivate** and **Cancel**. The counts are read when the entry is chosen. The row then shows
  Deactivated.
- **Reactivate**, for a deactivated local account, acts at once and offers Undo in a toast, "Sam Ortiz's account is back
  on.", which turns it off again with no question, since nothing is left to end.
- **History**, for every account, opens the account's history under the row (in the sheet on a phone), the History
  card's list of the account, read with the operator's wording.

The ⋯ shows what the account's `allowed_actions` allow. A provider account, and your own row, have History only: the
identity provider owns a provider account's state, and the server refuses turning yourself off. Focus goes back to the row's ⋯ after Done or Cancel. Errors show under the row.

**What the person sees.** Signing in to an account that was turned off says "Whoever runs this server has turned off
your account." and, under it, the operator's contact line when one is set. Their other tabs fall to sign-in on the next
request, since every session of theirs ended.

**History wording.** The History card of the person and the operator's list read the same entries, worded for the
reader. For the operator, who did it is named: "Reset link made by Ana Lee.", "Deactivated by Ana Lee. 3 sign-ins
ended.", "Reactivated by Ana Lee.", and for a command run on the server "Reset link made from the server.",
"Deactivated from the server. 3 sign-ins ended." and "Reactivated from the server.". The person reads "Reset link made
by whoever runs the server.", "Turned off by whoever runs the server. 3 sign-ins ended." and "Turned back on by
whoever runs the server.". Both read "Made an operator from the server." and "Operator role removed from the server.".
The provider's entries and every other entry read the same for both.

## Sign in and create account

A card on a wide screen and the bare form on a phone, with a Sign in / Create account switch. Sign in takes "Email
or username". Create account takes name, username, email and password; the username is permanent and part of every
link, so its hint says so. A problem about an input shows under it, with focus on it; any other (wrong password, too
many attempts, registration closed) shows above the button. A taken email offers **Sign in instead**, which switches
to Sign in with the address carried over and focus on Password. Under Sign in's password, a small **Forgot your
password?** button (`aria-expanded`) opens a disclosure and closes it again. When the server has mail set up, it holds
an **Email** field (empty, no focus move on open) and a **Send me a link** button, which shows the saving ring and ignores presses while the request is
pending. Enter in the field sends the reset and never the sign-in. After the server accepts it, a line under the
button says "If an account uses that address, a link is on its way. It works for an hour." and stays until the next
send; the field and button stay, so the user can send again. An address problem shows under the field; any other
problem (too many requests, a network error) shows above the line, under the button. Without mail, or while the page
doesn't know yet, the disclosure shows one line, "Whoever runs this Upheap server can send you a reset link.", and no
form. Create account has none. The invite page's Sign in side has it too.

With mail set up, Create account doesn't sign in. The card shows **Check your mail** in place of the forms, focus on its
heading: "We sent a link to a@x.dev. It works for a day.", a **Send it again** button (saving ring, no presses while it is pending),
and "Wrong address? **Change it**" (below). Signing in with the
right password for an account that hasn't confirmed its address shows the same card as **Confirm your address**: "We
mailed you a new link. It works for a day." with Send it again and the same Wrong address line. Send it again signs in again
with the login and password the form held, which makes the server mail a fresh link and answer that the address isn't
confirmed; that answer is "sent", and a line under the button says "Sent. It can take a minute to arrive.", shown anew
for each send. If the sign-in goes through (the address was confirmed meanwhile, in another tab) the user is signed in as
a sign-in would. Too many tries, or anything else, is the server's message in place of the line. The invite page's Create
account and Sign in sides do the same.

**Change it** replaces the Wrong address line with a small form under the buttons, for either way the card came up: a
"Right address" email input, focused, with **Send a link there** (saving ring, no presses while it is pending) and **Cancel**. Escape
and Cancel close the form and put focus back on Change it; Enter sends. It posts the login and password the form held
with the address, and the server sets the address of the still unconfirmed account and mails the link there. On
success the form closes, the card's text names the new address ("We sent a link to b@x.dev. It works for a day."),
the "Sent" line of Send it again goes, a line says "Sent to b@x.dev. It can take a minute to arrive.", and focus goes
back to Change it. The card then signs in with the account's username, since the login may have been the mistyped
address. A problem (too many tries, a network error) is the server's message under the form's buttons. An address
another account has looks the same as a free one, so nothing says it was refused: whoever mistyped twice changes it
again.

On a server with registration closed, the sign-in page has no switch and no Create account: the Sign in form shows
alone, and under its button a footnote in Create account's style says "New accounts need an invite link from a team's
admin." With a provider there is no footnote, since a first sign-in through it makes the account. The invite page keeps
its Create account, since the link makes the account. While the page doesn't know yet, the card holds only what it holds
while it waits for the provider; if the read fails, the switch shows. A sign-in the server throttles shows "Too many
attempts. Try again in 12:40." in the line above the button, counting down once a second from the server's wait, and
Sign in is disabled until it ends; editing a field doesn't clear it. When it reaches zero the line goes and Sign in is
enabled.

When the server has an identity provider, the card starts with a bordered button, **Sign in with Acme SSO**, the width
of the card (52 px on a phone, 44 px beyond), and under it an "or" rule, above the switch; the dark Sign in button
stays the main one. It is a plain link that leaves for the provider, carrying the page to return to. Without a
provider, or until the page knows, there is neither button nor rule. A sign-in the provider or the server refused comes
back to this page with one line above the button (`role="alert"`) that says why: "An account with sam@acme.test
exists. Sign in with its password below." (Sign in is selected, "Email or username" is filled and focus is on
Password), "Acme SSO didn't share a verified email address, which Upheap needs.", "Acme SSO didn't sign you in. Try
again.", "That sign-in took too long. Try again." or "Acme SSO can't be reached right now. Try again in a minute." The
line goes when a form is submitted. When the server signs in through its provider only, the card holds the
button and the line above it (and the operator's contact line under a taken email, below) and nothing else: no "or" rule, no switch, no forms, no Forgot your password?; and a
taken email says "An account with that email already exists and can't sign in through Acme SSO. Ask whoever runs this
server." The invite page's signed-out state is the same, its button still returning to the invite.

Wherever a page says to ask whoever runs the server (the forgotten-password line, the provider refusal, the reset page,
a team with no admin), the operator's contact line, when set, shows under it as its own line in the muted text style. The mail that tells an old address it was changed carries it the same way, on its own line under "ask whoever
runs this Upheap server".

A first sign-in through the provider continues at `/login/provider`, in the same card: **Pick a username**, "Signed in
through Acme SSO as:" with the name and email as plain text, the Username input (the same hint, focused, empty),
**Create account**, a **Cancel** button back to sign-in that sends nothing, and the footnote "You'll start in your own
heap. Teams come next." A problem about the username shows under it with focus on it; any other, above the button.
When the step has ended (a reload or a submit after 15 minutes, or after it was used) the page says "This step has
ended." with a **Sign in again** button. Success goes where the sign-in began, or `/`; the invite page, where a sign-in
that began there comes back, shows the signed-in Join step.

A signed-out link says where it leads
("Sign in to open the link to k3x9a") and opens it after sign-in; an expired session returns the same way.

A group-managed team (its members come from the identity provider's groups) takes nobody by hand. Under the Members
heading one line, "Members come from Acme SSO groups: engineering, sre.", stands where the search and Invite link
button would be, with no invite links and no Past links; the ⋯ on the name has no Link name, since the server leaves
that action out. Removing a member adds "If their groups still include this team, they come back at their next
sign-in.", on every Remove, because the page can't tell who signs in through the provider. Members and admins both see
the line. Its Activity says "Sam joined through the engineering group" and "Sam left: no longer in the engineering or
sre group", and "Sam was made an admin by whoever runs the server" on a role change from the `team make-admin` command. A shared team
with no admin among its members, which a sync can leave, says so to every member above that line: "This team has no
admin. Whoever runs this Upheap server can make one." The phone shows the same lines in the same places.

## Invite page

`/invite/{token}` is where an invite link leads, outside the app's shell like sign-in and in the same card, and needs
no sign-in. The server decides the state (the link's state, the viewer's standing and `allowed_actions`), in this
order, and the page only shows it:

- **Checking the link…** while it loads, with no form.
- **Not an invite link** for any token the server doesn't know, naming no team.
- **Already a member:** no page; they land in the team (below) with "You're already in Acme.", whatever the
  link's state.
- **Group-managed team:** "This link no longer works. Acme's members come from its groups." with no form, whatever
  the link's state: the server says so with `group_managed`, and the link works again if the team is unmapped.
- **Ended or revoked:** says which, who the link was for and from, and to ask the team for a new one.
- **Removed:** "This link can't add you back. Ask one of the team's admins."
- **Signed in:** "Join Acme?", who invited them, and who they're signed in as, with **Join Acme** and "Not you? Sign
  out", which reloads the same page signed out.
- **Signed out:** "Join Acme" and who invited them, over the provider button (when the server has one; it returns
  to this page signed in) and the sign-in switch with Create account first
  ([Sign in and create account](#sign-in-and-create-account)). Create account and join makes the account and joins in
  one act, also when registration is closed; with mail set up the account is made and joined, and the card is
  **Check your mail** as a state of this page (the team is in the sidebar once the address is confirmed). A taken email offers **Sign in instead**, which carries the address
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
same page. The link comes from the operator, who makes it with the server's command (README, Resetting a password), or
by mail, when the user asked for it on the sign-in page. The two are the same kind of link. The server decides
the state, and the page only shows it:

- **Checking the link…** while it loads.
- **Not a reset link** for any token the server doesn't know: "This isn't a reset link.", "Check that the whole link
  was copied, or ask whoever runs this server for a new one."
- **Used**, **cancelled** (a newer link was made, or the password or address was changed: "A newer one was made, or
  the password or address was changed since.") and **ended** say which, in a heading ("This reset link was already used.", "… no longer
  works.", "… has ended."), then "Ask whoever runs this server for a new one."
- **Working:** "Set a new password", "for Sam Ortiz (@sam)", the **New password** input (focused, hint "At least 10
  characters."), **Set password and sign in**, and under it "This signs you out everywhere else."

The states that can't be used end in an **Open Upheap** button, which leads to sign-in when signed out. A failed read
says why, with Try again. Enter submits, and the button shows the saving ring and ignores presses while it is pending and until the landing. A
problem about the password shows under the input, marked invalid and focused, with what was typed kept: "Enter a new
password.", "Use at least 10 characters." and "That password is too long." The link stays working after them. A `404`
or `409` means the link changed meanwhile (another tab used it, it ended), so the page reads it again and becomes the
state that says so; it never shows the raw error. Anything else is the server's message above the button. When the
page changes state, focus moves to its heading. A `401` on this page never sends the user to sign-in.

Setting the password signs the browser in as the link's user, whoever it was signed in as, and ends every other
sign-in of that user. It lands on `/`, which goes to the user's heap, replacing the reset page in the history, with
the toast "Password changed. You're signed in.", shown once like the invite page's.

## Change-email page

`/change-email/{token}` is where an email change link leads, outside the app's shell in the Reset page's card, the same
way at every width, and needs no sign-in: holding the link is the permission. Nothing is read when it opens, so a mail
scanner that fetches the link changes nothing: the page asks "Use the address this link was mailed to for your Upheap
account?" and has one button, **Use this address**, which ignores presses and shows the saving ring while it is pending. What the server answers is the state:

- **Changed:** "Your address is now sam@example.com." and **Go to your account**, which leads to `/account` and so to
  sign-in when signed out. A signed-in tab reads `me` again for the new address.
- **Not a working link** for a token the server doesn't know: "This isn't a working link.", "Check that the whole
  link was copied."
- **Used**, **cancelled** (a newer link was asked for) and **ended** show the server's words as the heading ("This link
  was already used.", "This link no longer works: a newer one was asked for.", "This link has ended."), with **Go to
  your account**.
- **Address taken** since it was asked for: "Another account uses this address now, so your address stays the same."
  as the heading, and no button. The link keeps working until it ends.

Anything else is the server's message above the button. When the page changes state, focus moves to its heading.

## Confirm page

`/confirm/{token}` is where an email confirmation link leads, outside the app's shell in the Change-email page's card,
the same way at every width, and needs no sign-in: holding the link is the permission. Nothing is sent when it opens, so
a mail scanner that fetches the link confirms nothing: the page asks "Confirm your address and sign in?" and has one
button, **Confirm and sign in**, which ignores presses and shows the saving ring while it is pending and until the landing. What the server answers is the
state:

- **Confirmed:** the browser is signed in as the link's user, whoever it was signed in as. It lands on `/`, which goes
  to the user's heap, replacing the page in the history, with the toast "Address confirmed. You're signed in.", shown
  once like the reset page's.
- **Not a working link** for a token the server doesn't know: "This isn't a working link.", "Check that the whole
  link was copied."
- **Used**, **cancelled** (a newer one was mailed, which a refused sign-in or Send it again does) and **ended** show
  the server's words as the heading ("This link was already used.", "This link no longer works: a newer one was asked
  for.", "This link has ended."), with **Open Upheap**.

Anything else is the server's message above the button. When the page changes state, focus moves to its heading. A
`401` on this page never sends the user to sign-in.

## Email

Account's Email card sits between Profile and Password, and replaces Profile's Email row. It is there when the server's
`change_email` action is: the user has a password and the server has mail set up. Without it the address stays a
read-only row in Profile. Closed, it is the heading "Email" and a **Change** button on one row (wrapping on a narrow
phone), and under them the address, which wraps. Pressing the button replaces it with a form: **New address**
(focused) and **Current password**, then **Send link** and **Cancel**. Enter submits, and both buttons are disabled
while it is pending. The password is sent as typed.

Errors sit under their input, which is marked invalid and described by them, and focus moves there: "Enter your email
address.", "That doesn't look like an email address.", "Enter your current password." and "That isn't your current
password." Typed values stay, except Current password after a wrong one, which is cleared and focused. Too many tries,
or anything else, is the server's message above the buttons. Cancel and Escape close the form, clear it and give focus
back to Change.

A success closes the form and shows a status line under the address, "Check sam@new.example. Your address changes when
you open the link. It works for a day.", until the form is opened again or the page is left. Focus goes to Change. The
address in the card stays the old one until the link is applied ([Change-email page](#change-email-page)); asking for an
address another account has looks the same, and no mail comes.

## Password

Account's Password card sits between Email (or Profile, without one) and Appearance. Closed, it is the heading "Password"
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
so a copy of it can't be used afterwards. Without the server's `change_password` action, as for a person who signs
in through the identity provider and has no password, the card keeps its heading, and where the button
would be says "You sign in through Acme SSO." ("an identity provider" if the server has none).

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

Below the sign-ins the card has a **Tokens** part for machines with no browser or keychain: a line saying so, and the
user's personal tokens, one row each with a key icon, the name, "Made 3 Nov · last used never · expires 3 Feb" (or
"never expires"), and a **Revoke** button named for the token. Revoke turns the row into an inline confirm, "Revoke ci?
Anything using it stops working at once", and shares the one open confirm with the sign-ins. After a revoke the card
says so in its status line and focus moves to its heading. With none, the part says "No tokens yet." **Make a token**
opens a form in place of the button, with a name, an expiry (90 days by default) and Make token; Enter submits, and
Cancel or Escape closes it and gives focus back to the button. A refusal shows in the form, which keeps what was typed.
A made token replaces the form with a panel that says it is shown once, the secret in a read-only field that selects
itself on focus, a Copy button that has focus (the field is selected for copying by hand when the clipboard isn't
there), and the command to use it with the secret left as `<token>`. Done closes the panel and gives focus back to
Make a token. The secret lives only in the page's state, so leaving the page drops it.

## History

Account's last card, after Command line and before Sign out, is headed "History" and holds one collapsed disclosure,
"Changes to your account", like the ones on a tag or a member, with the same chevron and touch target. It loads only
when opened, and lists the user's password events, their address changes, the sign-in provider or an operator turning the account off and back on, the operator role given or taken away, and
their own heap priority changes, newest first, each a sentence from the server and its time: "Password changed. 2
other sign-ins ended.", "Reset link made by whoever runs the server.", "Reset link sent by mail, as asked on the
sign-in page.", "Password set with a reset link. 3 sign-ins ended.", "Address change to sam@new.example asked. A link was mailed there.", "Address changed from sam@old.example to sam@new.example.", "Turned off by your sign-in provider. 3 sign-ins
ended.", "Turned back on by your sign-in provider." and "You changed your priority for Work from 1 to 3." It names no actor, since it is the user's own account, and holds
nothing else of the account (not sign-ups or profile changes). Empty, it says "No account changes yet."; an error
shows the server's message; **Load more** fetches the next page. A password change, an address change asked or a priority change adds its entry
to an open History at once, and to a closed one by the time it is opened.

## Conflicts and errors

1. **Conflicts say what was lost.** "Sam changed the due date a moment ago. You're seeing their version; your
   change (due: next week) wasn't saved. Apply mine again." The retry is one tap, and the pane re-renders from the
   server.
2. **Loading** keeps the previous view, dimmed, rather than a blank page.
3. **A `401` mid-session** sends the user to sign-in with the way back, as a signed-out visit does, dropping the
   last user's cached data; signing in returns them to the page.
4. Other item errors show inline in the pane with the server's message: a closed item only changes state, a reopen
   past its week, an option archived meanwhile.
5. **Not found.** An address nothing answers, a team, or a heap the server answers `404` for shows a page in the
   content column, inside the shell: a muted "Not found", a heading ("Page not found", "This team isn't here",
   "This heap isn't here"), one line, and **Go to Waiting for me**; a team or heap also gets a quiet **See your teams**
   link. A missing and a not-yours team or heap read the same, as the server's `404` does. A missing item stays in
   the item pane ("This item isn't here", "It doesn't exist, or it's in a heap you can't see.") with a button back to
   the list, named for the heap ("Back to Work"; "Back to the list" on All items), and the list behind stays.
6. **A banner when the connection is down.** A strip at the top of the shell, above everything, in the page flow.
   Offline says "You're offline. Changes won't save until the connection is back." Online but unable to reach the
   server says "Can't reach the server. Changes won't save until it's back." with **Try now**, which refetches what
   is on screen. A proxy's own `502`, `503` or `504` (no problem body: the server process is down) counts as unable
   to reach the server. Offline wins when both hold. A phone gets the short "Won't save." The banner is a `status`, and says
   nothing when it goes: the page refetches when the connection returns, on focus and on Try now. There is no queue:
   an edit while offline fails at once with "You're offline, so this wasn't saved.", and one that can't reach the
   server with "Couldn't reach the server, so this wasn't saved."; the draft stays where it was typed. Loaded data
   stays on screen; a page not visited yet waits while offline.
7. **A server error shows its request ID** under its words, for every `5xx` the server sent one with: the ID in
   monospace, never cut (it wraps on a phone), plain text to select, and a **Copy** button that reads "Copied" for two
   seconds. A toast with an ID stays until it is closed. Offline and unreachable errors have no ID.

## Rules

1. **Confirmations.** A reversible change acts at once and offers Undo in a toast: state changes, archiving a heap,
   a view or a parameter. A change that can't be undone, or that affects other people, asks first, inline or in a
   dialog on a wide screen and in a sheet on a phone, and names the object: removing a member, giving up your own
   admin role, revoking an invite link, and discarding a typed title. Cancel on a form that edits in place (Ask a
   question, Change reply, Changes asked, an item's or a heap's context) is itself the choice to discard, so it
   drops the draft without asking; such a form guards its draft from a stray key and from refetches instead.
   Closing a dialog or a sheet that holds a draft, or leaving the page with one, never drops it silently: Add an
   item and Split… ask first, the item's context saves the draft, and a popover form (Changes asked, Send to an
   agent from the item or a selection, Why it's blocked) keeps it and shows it again when it reopens, until it is sent or cancelled. A reload
   or opening another item still drops these short drafts.
2. **The phone and the wide screen share components and differ only in presentation.** On the phone, the item
   sheet's main state button and ⋯ sit in a bar at its bottom; on a wide screen they sit beside the title.
3. **Every page that edits a list of things** (Tags, Parameters, Members, Heap settings) uses one settings layout:
   title, one-line explanation, a way to add one near the top (a "+ New" button, or a short form when adding takes a
   field or two), rows that turn into a form when edited, and the rarer actions out of the way (a ⋯, or the foot of
   a card).
4. **After the session expires, signing in returns the user to the page they were on.**
5. **Editing lives in the item.** Rows are for reading, ordering and changing state; everything else is edited in
   the item ([decisions.md](decisions.md#23-editing-lives-in-the-item-rows-only-change-state)).
6. **Saving.** The app is never optimistic, so between a press and the server's answer a small ring turns inside the
   control that was pressed, or, when the control goes away on press, on the field or row it changed: a field shows
   the ring before its old value, a list row in its state slot. It shows 150 ms after the press, so a quick answer
   shows no flicker, and stays until the answer and the refetch it starts have landed; then the new state replaces
   it. The control ignores presses meanwhile and isn't faded, as a control disabled for another reason is; its label
   doesn't change. On a failure the ring goes, the old value stays and the error shows where it always did. With
   reduced motion the ring pulses instead of turning.

## Testing

The Playwright golden paths run on phone (Pixel 7 emulation) and tablet (iPad emulation) projects as well as desktop
Chromium, against the real server.

## Keyboard

The wide screen shows its keys, and `?` lists them all:

1. Hints, small key chips, show on a wide screen with a mouse or trackpad: `N` on Add item (on a tablet too), `/` in
   the search box, `S` `B` `X` at the end of the focused row, `⌘↵` (`Ctrl ↵` off a Mac) on Add in the Add dialog,
   `⌘K` (`Ctrl K`) on Find in the sidebar.
2. ↑ ↓ to move between rows and Enter to open; ↑ ↓ in the item pane walk the list. Alt+↑ and Alt+↓ on a row move the
   item up or down (The list point 20), on any width with a keyboard.
3. A hint bar at the bottom of the window, under the heap's list however short it is, has the keys for what's on
   screen: moving and opening, `⌥↑↓` (`Alt ↑↓` off a Mac) where rows can be placed (The list point 20), `S` `B` `X` when a row can change state, `N` and `/` with no item open; walking and
   closing with one open; selecting in select mode. It ends with `?`, All keys.
4. `?` anywhere in the app opens Keyboard shortcuts, the list of every key. Keys are ignored in a field. Open and
   Closed have no keys: they are one click away in the sidebar.
5. `⌘K` (`Ctrl K` off a Mac) anywhere, Find at the top of the sidebar, or the search icon in a phone's top bar, opens
   Find. It finds items by title or key in every heap, goes to a heap, and adds "Add “text” as a new item" to the heap
   on screen. ↑ ↓ and Enter open, Tab shows the highlighted item's actions (Start, Mark done, Due, Copy link, as the
   item allows), and Shift+Tab goes back. Closed items come after the open ones, in the server's order, under a
   "Closed" heading styled like "Items" (with only closed hits, "Closed" is the only one). A closed row's title is
   muted, and its state ("Done" in teal, "Won't do" or "Became" muted) sits just before the key at every width, since a
   phone hides the heap name. ↑ ↓ move across the heading as if it weren't there. `upheap item search` prints a closed
   row faint on a terminal, and plain when piped or with `NO_COLOR`: the state column says it either way. Open, Find is
   a history entry, so Back closes it; going to an item or a heap takes its place, so Back from there returns to where
   it was opened. On a phone it opens at the top, full width, and ends above the on-screen keyboard, with the results
   scrolling; ✕ in the field clears it, or closes Find once it's empty, and a tap outside closes it too.
