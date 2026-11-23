# Web app UX

How the web app looks and behaves, at every width: the principles, the core flows, the toolkit, the layout per
width, each screen, and the rules that settle questions. What the product promises is in [product.md](product.md).
The mockups live outside the repository, in the "Upheap layouts" canvas; a board is named where this doc relies on one.

This is the record of the agreed design: what each screen does, where things sit, and why. Exact copy, sizes,
timings, routes, icons and keys are the code's (`web/src`); this doc says them only when the value is itself the
rule. What a screen reader hears is part of the design and stays. When a cut detail carried a reason the code
doesn't state, the reason moves into a code comment.

The app is one React app whose layout changes with the width: **phone**, **tablet** and **wide**, at Tailwind's
`sm` and `lg` breakpoints. It never ranks, checks permissions or works out labels; the server does
([architecture.md](architecture.md#principles)), and the client adds layout, gestures and animation.

## Principles

1. **Consistent look.** One set of tokens: stone neutrals, amber for "in progress" and for the next action, teal
   for done, red only for a firm due date that's overdue, violet only for parameters. Tags take their own colour from a
   palette on the chip's border and text, with no fill; violet, amber and red are left out of it, so parameters, the
   accent and overdue keep their meaning. A control looks the same wherever it appears: S M L XL buttons, the due
   choices and the tag chips are identical in the item pane, the Add dialog and their phone sheets.
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

1. **Sign up and sign in** (setup). Creating an account lands on the empty personal heap. Later visits sign in; when
   the session ends, sign-in brings the user back to the page they were on (recovery).
2. **Getting into a shared heap** (setup). Someone creates a team on Teams and becomes its admin; the Team page
   opens. The admin adds a heap, adds each teammate (who must have an account, or sends an invite link to one who
   hasn't), and makes a second admin so the team doesn't hang on one person. A teammate finds the heap in the sidebar or the heap
   switcher.
3. **Daily loop** (loop). Pick the heap. The list shows in-progress items pinned on top, then Up next. Start one from
   its row, a key, a swipe or the item; finish it the same ways; each change gets an undo toast. Add what came up
   from anywhere in the heap, + on a phone. Re-size or re-date in the item and watch it move. Block an item, naming
   what it waits for; it leaves the list, since blocked items are hidden by default, and the list offers to show them
   to unblock. Find an item with search. In a shared heap, teammates' changes arrive as the list refetches, and rows
   animate to their new places.
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

1. **Design tokens in one place** (`web/src/index.css`), so light and dark modes come from one place instead of
   pairs on every element. Which one shows is the Account page's Appearance setting: System (the default) follows the
   device, and Light or Dark overrides it. It's kept per browser, not on the server, as it's about the screen rather
   than the account, and applied before the first paint, so a reload doesn't flash the other theme.
2. **React Aria Components** for behaviour, the list included: unstyled, one dependency rather than copied code, and
   sound on touch and for accessibility
   ([decisions.md](decisions.md#24-react-aria-components-rather-than-a-copied-component-library)). The other
   libraries are in `web/package.json`.

### Markdown text

Longer text that people and agents write shows as rendered Markdown, through one component: the context of a heap,
an item and a step, a question's details, the comment in a reply, and an item's comments. It is GitHub-flavoured.
Raw HTML is never rendered and images are never loaded: an image shows as a link. An in-page anchor moves in place,
every other link opens in a new tab, and an unsafe one reads as plain text. A line break in the text stays one, so
plain text written before reads as it did. Nothing makes the page scroll sideways: long words break, and code blocks
and tables scroll inside their box.

Writing a context (a heap's, an item's, a step's) or a comment is in an editor that keeps the Markdown source as
typed and styles it as you type, the markers visible and muted. It has no toolbar and no preview, and the saved text
is the plain Markdown. It loads only when one of these fields is about to show, so pages that only read don't load
it.

Short text stays one plain line: a turn's line, a stuck reason, error messages, and a question's text and choices.
In a question's text and choices, a web address and a Markdown link to one are links that open a new tab; a link in
a choice opens without picking it.

## Layout

Breakpoints are Tailwind's `sm` and `lg`.

| Width  | Navigation                                                                                                                                                                                | List                                                     | Item                                                  |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- | ----------------------------------------------------- |
| Phone  | Heap name at the top, with its team unless the heap is personal, switches heap; bottom tabs Open, Waiting, + (Add), Agents, More                                                          | Full width, compact rows                                 | Bottom sheet over the list                            |
| Tablet | A rail: Waiting, the heap button, then the views as icons with labels, and Agents, the heap's team, Teams and Account at the bottom                                                       | Beside the rail, with the wide screen's toolbar and rows | Panel over the right of the list, the list still live |
| Wide   | Sidebar: Waiting for me with its count, a heap button that opens the heap switcher, then the heap's views with Open's count, and Agents, the heap's team, Teams and Account at the bottom | Middle column                                            | Pane beside the list while an item is selected        |

**More** on a phone holds Closed, Settings, Activity, Data, Team, Teams and Account, as a card above the tabs.
Account is the last entry there and in the sidebar, and Sign out is on the Account page, under its cards. The **+**
in the middle of the tabs opens the Add dialog from any of the heap's pages, under the thumb, so the Add button
beside the heap name shows only from a tablet up.

The sidebar and the rail list the heap's views (Open, Closed, Activity, Data, Settings) and, below them at the bottom,
the team in the URL (a link to its page, shown on the heap's pages and the team's own), Teams and Account. The heap's
title names its team too, on the phone's top bar as well, except for a heap in the personal team.

**Find** on a phone is a search button at the end of the top bar, before Select, which a screen reader hears as
"Find in all heaps", on every page with the top bar: one tap opens it
(Keyboard, point 5). It and the list's own field are named apart, so the search that covers every heap and the one
that filters the list on screen don't read as the same thing.

A phone's tabs have room for four: Waiting sits left of the + and Agents right of it, and Closed, Settings, Activity
and Data go into More with the account pages. More shows as active on the pages it holds, Closed among them.

**Waiting for me** comes right after Find at the top of the sidebar and the rail, above the heap switcher, with its
count; in the rail and the tabs the count is a badge on the icon. There is no count at zero, and the entry stays. The
count is the number of open questions across the person's heaps, fetched with the page and again on the list's
interval while the tab is visible, so a question an agent asks shows in the count as soon as on its row. It's also
fetched again on focus and after every change the person makes to an item, a question, a step or an agent's turn. The
heaps' open counts and the Agents page's Your decision are fetched again with it, all three after each such change,
since one change can move any of them: a reply can close its item, and sending an item on takes it off Your decision.

**Agents** is the first entry of the block at the bottom of the sidebar and the rail, above the heap's team, with no
count. What agents finished is there to look through when the person wants to, not something that waits as a
question does, so nothing in the navigation counts it; the page's Your decision heading does.

**All items** is reached from the heap switcher, on every width: nothing in the sidebar, the rail or the tabs leads
to it. The switcher starts with an "All heaps" row with the person's open items at its right: the sum of the open
counts of the person's active heaps, the numbers the heaps show, so it costs no request and changes when those do. A
person in more than one team also finds, at the right of each team's line, a link to that team's items with its open
count (All items §7), which a screen reader hears as "All <team> items, 16 open"; neither is among search results. On
All items and its Closed and Activity pages the row opened is marked current, where no heap's row is, and the heap
button reads as that row does, on the sidebar and the phone's top bar; the rail's button shows a mark for it in
place of a heap's initial. No heap is the current one there, so the views
are the pages across heaps (All items §8): Open with the count the row shows, Closed and Activity, with no Data or
Settings; on a phone the Open tab and More's Closed and Activity are those pages, More has no Settings or Data, and
the + still adds to the heap opened last. The heaps' open counts, on Open and in the heap switcher, are fetched as
Waiting for me's count is, so an item someone else adds or closes reaches them about when it reaches the list.

Pages with no heap in their address keep the heap opened last in the shell, so going back is one tap: the switcher
shows it, the heap's views link to it with none marked active, the tabs show on a phone, and Add adds to it. For now
that is Waiting for me, Agents and All items with its Closed and Activity, which keep it for Add only (All items,
above); Teams, a team's page and its activity, and Account keep their shell without one. The heap kept is the one
opened last that is still there and active, else the first active heap, else the first. On a phone those pages have
no tabs, so the top bar starts with Back: it goes to the page before, or, when the tab opened on the page from a
link, to the heap kept.

Opening the app's root lands on the person's default heap (Heap settings) while it is active and one of theirs, else
on the first active heap of the first team that has one. The heap kept, above, is unchanged by it.

On a wide screen the sidebar and the content are one centred block, at most the sidebar, the content column and the
item pane wide, so the menu stays next to what it opens; the sidebar's colour runs on to the left edge. The block is
that wide whether or not an item is open, so opening or closing one never moves the sidebar or the list's left edge:
an item is opened many times a session. Every page fills the room next to the sidebar, so the content's left and
right edges are the same from one page to the next. On a list that opens items beside it (a heap's list, All items)
the pane opens on the right, so the list narrows from the right. A card fills that room too, and a short field in it
keeps the width its value needs.

### Heap settings

**Settings** is one page for a heap's configuration, with a tab each for **General**, **Context**, **Tags**,
**Parameters** and **Ranking**. Each tab has its own address, so Back steps between them and a tab can be linked to;
the old addresses of Tags and Parameters lead to their tabs. General holds the name and the link name, edited in
place, a note that a new link name breaks old links (the new link shows while it's changed), the heap's history, the
Agents card, and Archive or Unarchive. Each control shows only where `allowed_actions` allows it; everyone else reads
the values.

The **Context** tab holds the Context card. A heap made from a note names and links to that note above it. It shows
the heap's context as Markdown text (see Toolkit), or a muted line saying what a context holds when there is none.
Anyone who can open Settings sees the tab. A member who may edit the context sees Edit (not on an archived heap),
which turns the card into a form: an editor that grows with its text, Save and Cancel. A failed save keeps the form
and the draft open and says why; Escape does nothing, so a long draft is never lost to a stray key. When someone else
saved a different context meanwhile, it shows rendered above the kept draft, and Save again replaces it; when only
the heap's other settings changed, the message says so and Save again saves the draft. Cancel drops the draft without
asking. Save and Cancel give focus back to Edit.

The **Agents** card, on General, has a switch for whether agents prepare new notes: each note added is sent to an
agent to fill in its context, or to make it a defined task when it's small and clear, then comes back to the person.
It saves as it is turned, and can't be turned again until the save ends. Only an admin sees the switch; others, and
everyone in an archived heap, read whether it's on.

The **Your priority** card, on General after Agents, is the person's own priority for the heap, apart from the
Ranking tab's factor weights: its items' scores in Waiting for me and the agents' queue are multiplied by it, and only
they see it. It goes from 1 to 10. Picking a number saves it at once, as the Agents switch does, and another can't be saved until that save
ends. Everyone who can open Settings may set theirs, archived heaps too; an error shows under the card.

The **Default heap** card follows it: the person's own choice of the heap Upheap opens on, kept on the server so it
follows them to every device, and seen only by them. One button makes the heap the default or clears it, saves at
once and keeps focus as its label changes; an error shows under the card. One heap is the default at a time, and
archiving it stops the app opening on it without clearing it.

The tabs share one look: an intro line, then cards in the design tokens. **Tags** has a New tag card and a card of
the heap's tags, each row the tag's name in a chip in its colour and its description; editing a row turns it into the
form (name, description, colour, History), where the colour is picked from swatches with a chip that previews the
name in it. Beside it a ⋯ holds Make a team tag, for anyone who may edit the tag (so not in an archived heap). A
second card lists the team's tags the same way, with History and a link to the Team page, and edits them in place:
the form says the change reaches every heap of the team, and saves through the team's tag (an archived heap too, as
the server allows). A team tag's History reads its made, edited and merged entries, each naming the tag, and a merge
lists each heap's tag and its items. Adding or renaming a heap tag to a team tag's name is refused under the field,
pointing to the team tag.

**The merge confirm.** A team tag's write that would merge heap tags (Add on the Team page, creating a team tag in a
team parameter's picker, Make a team tag, or a rename into a name heap tags have) is refused by the server with the
list of what it would merge, and the app asks first, in a dialog on a wide screen and a bottom sheet on a phone. It
lists a row per heap tag: the heap, the tag as that heap wrote it, and its item count, with archived heaps and, for
Make a team tag, this heap marked. Under the rows it totals the items and heaps that will carry the team tag, shows
the description the team tag will have, and says quietly that the items keep their history and this can't be undone.
Cancel and a confirm button end it. While saving, the confirm button shows the saving ring (Rules §6) and nothing can be pressed or dismissed. If the list changed
while it was open, the server refuses the confirm's write with the new list: the dialog stays, says it changed, marks
new rows and the old counts, names what left the list, and the button confirms the new list. Another failure shows
above the buttons, with its request ID, and the dialog stays; a rename someone else changed meanwhile closes it and
shows the form's version conflict. Success closes it with a toast that says how many items in how many heaps carry
the tag. Cancel changes nothing, and the form keeps what was typed.

**Parameters** opens its add form from a button, and gives each parameter a card: the name with Rename, the weight,
the options as a numbered list (reorder and Archive on each; Archive turns the row into a confirm that asks where open
items go), Add option, the tags that suggest it, and a foot with History and Archive parameter. Each option opens the
heap's list filtered by it: for an admin, whose label is an input, by a link after the label, named "Items with
Energy: Low"; for a member who can't edit, the option row itself. Archived parameters are listed last, and their
options aren't links. On a phone everything stacks in one column, main buttons span the width, and every control is
finger-sized.

A heap's own parameter, for an admin, has a ⋯ after Rename with **Make a team parameter**. It sends the write without a
token and the server's refusal opens the **parameter merge confirm**. The confirm is the merge confirm of Tags, with
the same dialog and sheet, the same changed-while-open state, and the same failure line. It differs in what it lists:
a row per heap parameter (the starting one first, marked as this heap, then the same-named ones, archived heaps
marked) shows the heap, the parameter as that heap wrote it, its item count and the options it brings that the team
parameter lacks. When one of its options matches an option the team parameter has archived, the row says so: its
open items get the option in use that the archived one was replaced by, while closed ones keep theirs. A row whose
parameter is linked to tags names them: its heap keeps those links to the team parameter. The total says how many
items in how many heaps keep their values, that each heap keeps its weight, and that rankings can shift in a heap
whose options were in another order, then the same quiet line about history.

A team's parameters a heap uses are in the same list by name, each marked as a team parameter (", team parameter" to
a screen reader), its weight read-only, its options read-only, the team's tags that suggest it read-only, and the
heap's own tags that suggest it, which an admin edits in place (each change is saved at once), a line saying the
weight is set on the Ranking page and the name and options on the Team page (both links), and Stop using for an admin
(an error from the server shows under it). A card lists the team's other parameters, with a link to the Team page,
each with its name, its options and a Use button for an admin; with none it points to the Team page. Using one gives
it the weight it had, or the default weight.

**Ranking** (the canvas boards "Wide · Ranking" and "Phone · Ranking") sets how much each factor counts. A Built in
card has a row each for need, age, deadline and value per effort, and a Parameters card, shown when the heap has
parameters in use, has one per parameter, with its options in order as the description, each a link to the heap's
list filtered by that option. A team's parameter the heap uses is in the card, marked as the team's, and its weight is
set here with the others. On a phone the links stay inline, the one place they are smaller than a finger's target.
Every option shows, however many lines they take. Between them, a **Tags** card holds the Tags weight as a factor row
and the heap's tag order as a numbered list, the highest first. Each row says, dim, what the tag does next to an
untagged item (lifts, as untagged, holds back), and a line under the list says where untagged items sit. Editors can
move and remove each row (a ⋯ menu on a phone) and add a tag not yet ranked, at the bottom; the order is a draft saved
with the weights, counts once among the unsaved changes and is marked changed on the card. Others read the order
without buttons. A row shows the factor's name, its share of the total as a bar and a percentage (plain arithmetic on
the weights on screen; no share when all are 0), and for editors a slider and a number input that move together. One
save writes every changed weight at once; what changed is marked with its old value and counted, Cancel (or Escape)
puts the weights and the order back, and Reset sets every weight's draft to the default, the Tags weight too, without
saving or touching the order. Others read the weights and a note that only admins can change them. A **Top of the
list** card, for everyone, shows the first few items as the draft weights and tag order would rank them, each with how
far it moves, so the effect shows before saving. Wide: a sticky save bar under the cards and the preview beside them.
Phone: one column, the preview under the cards, the save bar fixed above the bottom tabs.

### Tablet

A tablet gets its own layout rather than a stretched phone or a squeezed wide screen:

1. **The rail.** The sidebar folds into a rail. Its top button shows the heap's initial and opens the heap list as a
   popover; below it are the heap's views as icons with short labels; the account sits at the bottom.
2. **The list** takes the rest of the width, with the wide screen's toolbar (search, Sort by, Filters) and rows at
   touch sizes.
3. **The item** opens as a panel over the right of the list. The list beside it stays live: tapping another row
   switches the panel, and the selected row stays highlighted. Swiping the panel right or tapping ✕ closes it. It uses
   the wide screen's layout, with popovers, and the touch cues of The item §16.
4. Swipe actions on rows work here too, as on a phone.

### Phone details

Phone pages respect the safe-area insets under the bottom tabs and sheets, and inputs are large enough that iOS
doesn't zoom in on focus. Sheets sit above the on-screen keyboard, on Android, which resizes the page for it, and on
iOS, which doesn't. The heap switcher and More open as sheets too; More's sits above the tabs.

Pickers that are lists (heaps, due choices, a parameter's options) are tall rows with a check on the current one. In
the heap switcher the check comes before the current row's name, as part of it, so the other names keep one left
edge and the counts sit at the panel's right edge.

The heap switcher groups heaps by team, with archived ones folded per team. Each unarchived heap's row has its open
count on the right, the number its Open view shows, which a screen reader hears as ", 2 open" in the row's name, as on
the All rows; archived heaps and Recent have none. Once the list is long it adds a search field that filters as you
type, fold headers for the teams, and Recent: the heaps last opened in this browser, not the one on screen, as chips
on a phone. Recent and the folds are kept per browser and don't follow the user to another device. Search gives a
flat list with the team and an archived mark, a row to make a heap of that name, and a message when nothing matches;
the arrows move through it, Enter opens a heap, and Escape clears the field first, then closes. It ends with a New heap row, in the heap's own team, when the team allows it,
which turns into the same form as the Team page's ([Teams and Team](#teams-and-team)) and opens the new heap. On a
tablet it is a popover beside the rail. The popover, on a tablet or wide screen, has no title bar: it starts with the
search field, or with the list when there are few heaps, and its rows are denser than a sheet's. Each team's name, and
Recent's title, sits on a band a shade off the panel, and a team's fold header stays in view while its heaps scroll,
with the team's All items link at its right, folded or not.

Gestures are offered where the pointer is coarse (a finger), on phones and tablets alike:

1. A row dragged right far enough slides away and marks the item done; dragged left, it stays open on buttons, Agent
   (when the row's robot would show, The list point 3), In progress and Blocked (Open in place of the state it's in),
   until one is picked or the row is tapped. Each is followed by a toast with Undo. A tap just after a drag doesn't
   open the item.
2. The item sheet drags down by its top bar to close; the tablet panel drags right. Letting go before it is dragged far
   enough springs it back.
3. On a row that can be placed (The list point 20), a finger held still for a moment lifts it, and from then the finger
   drags it up or down. A move of more than a small wobble before then ends the press, so a sideways move still swipes (point 1) and a vertical
   one scrolls; a quick tap still opens the item. While a row is lifted the page doesn't scroll and the row doesn't
   swipe. The press opens no link menu or callout.

## The list

1. A row is one or two lines: three buttons (point 3), the title, the due chip and tags, and below them the server's
   contributors, then, at the end, the kind and stage icons. A row shows no position and no sizes: the order is the
   list's, and the contributors say why. The kind takes a fixed slot at the end of the row (a note leaves it empty,
   so the icons line up), and an item with context or defined has its stage icon in the slot after it. The row shows
   no words for them; a screen reader hears them after the title ("Rotate the certs, task, defined"). A task with
   steps shows how many are ticked, small and muted, after the title, and a screen reader hears ", 2 of 5 steps
   ticked" after the title. Dropped steps don't count, and a task without an open or ticked step shows nothing.
2. Custom parameters with a value show as violet chips after the tags on a wide screen, and on their own line under
   the contributors on a phone or tablet. Unset parameters don't show. Violet is kept for parameters everywhere, so
   they read apart from tags (outlined, in their own colours) and due dates.
3. A row starts with three fixed slots, from a tablet up, so titles line up; a slot the person can't use stays
   empty. First the **state button**: Start on an open row, Unblock on a blocked row, Pause on an in-progress row.
   Start moves the item to in progress; Unblock and Pause move it to open. A screen reader hears ", blocked" after a
   blocked row's title, or ", blocked: <reason>" when the item says why in words, and the button's tooltip carries
   the reason too; the row itself shows nothing more. Second **Done**, on every row. Third the **robot**: a button
   that sends the item to an agent, one step at once, with no line, when the server allows it and no agent has the
   item; when an agent has it, a mark that shows whether a run is live or queued, whether it waits for the person,
   and where the turn ends (or that it watches a pull request), with a tooltip that says it in words; empty otherwise
   (a reminder, a blocked item, a reader). With a mouse, Done and the robot are muted until the row is hovered or has
   focus within; with a finger they are always in full colour. A quick send offers Undo (The item point 15), gives
   focus to the row, and a failed one says why. A phone shows the state button only and swipes instead (point 5).
   Without permission to change the state, the buttons are absent and the slots stay empty. An undo toast follows
   every state change from the list, above the tabs on a phone. Changes made in the item save silently and have no
   undo. The toast sits over the open item, so a send can offer Undo from inside a phone's sheet; the Add dialog and
   the other sheets stay above it.
4. Tapping or clicking a row opens the item. On a wide screen the keyboard moves through the rows, opens one, and
   starts, blocks (with no reason, as the swipe does), marks done or sends one step to an agent, each only where the
   row's button would allow it. Tab goes through the row's buttons in their order, then its link, then its tags and
   parameter chips.
5. On a touch screen, swiping a row right marks it done; swiping left offers Agent (where the robot button would
   show, sending one step at once as the button does), In progress and Blocked.
6. Saved views live in the Filters panel, in a Views section that comes first, not in a bar. Tapping a view applies
   it: its Show settings, sort and filters replace the list's in the URL, the agent filters are cleared and the
   search stays. On a phone this also closes the sheet. A view whose settings equal the list's has a check. Saving
   names the current settings in place. Each view can be renamed, updated to the current settings (when the list's
   settings, the agent filters aside, differ from it) and archived. Update and Archive close the panel, so their
   toast shows, Archive's with Undo. The URL holds the settings, never a view, so links keep what they showed. The
   controls show only where the server allows them. The All items page has no saved views.
7. One Filters panel holds the saved views, what the list shows and its filters: Views (point 6), Show (each state
   pinned, shown or hidden, with a mark on a changed one; shown blocked items in their own group or mixed in;
   reminders to come, point 16), then Narrow down (tags, with the team's tags beside the heap's and marked as the
   team's, need, effort, impact, parameters, agent state, going to), with Reset all and how many items it shows. It's
   a popover from a tablet up and a sheet on a phone. The toolbar is search, Sort by, Filters and Select (point 19);
   on a phone it's search and Filters, with Select in the top bar, and the sheet starts with the views, then the sort.
   The Filters button counts the filters and changed settings, in the accent while any are on; on a phone, where the
   sort is out of sight in the sheet, it counts a sort other than Priority too. The list updates live behind the
   panel.
8. Under the toolbar a chip takes off each filter or changed setting, with how many items show while filters are on,
   and Reset all. Reset all keeps the sort and the search.
9. Rows have no hover actions besides the colour of Done and the robot (point 3). The other states (Blocked, Won't
   do) are in the item.
10. After Done, the toast offers Undo. Only when nothing else in the heap is in progress does it also offer to start
    the top open item; if something else is already in progress, the user is carrying on with it, and the offer would
    be noise.
11. Active filters and changed view settings show as the removable chips of point 8, so the user always sees why the
    list is short.
12. The note on hidden items says how many are hidden and offers to show them.
13. An open item with an open question says **waits for you** at the end of its row, in the accent, before the kind
    and stage icons. On a phone, where the end of the row is tight, an accent dot before the title stands for it. A
    screen reader hears ", waits for you" after the title on every width. The closed list never shows it.
14. An open item an agent has shows the robot mark in its slot (point 3), with a dot while it waits for the person: an
    item that waits for the person is not the agent's, and says so in words (point 13). On a phone the mark, with
    where the turn ends, goes before the title; a dot alone stands for an item that waits for the person. A screen
    reader hears ", with an agent, running now, going to defined", ", queued, going to with context" or ", watching
    pull request 276, going to done" after the title, and ", agent stuck" after ", waits for you" for a stuck one. Who
    has an item never changes its place. The closed list never shows it.
15. Moving an item an agent has to a state that takes it back, from the list, asks first, naming the item and saying
    that the change takes it back from the agent. The state button, Done, a swipe and the keys go through the one ask,
    a dialog on a wide screen and a sheet on a phone. Cancel puts a swiped row back and gives focus to the row.
    Blocked and Open don't ask. The change's Undo restores the state only: the turn stays taken back.
16. A reminder before its date is not in the ranked list, and nothing counts it. **Reminders to come** in Filters ›
    Show (hidden by default, a chip when shown) adds a group under the list, after Blocked: a row has the reminder
    mark, the title, the date it joins and the tags, and no state button or swipe. They are in date order, soonest
    first, and the filters narrow them like the rest. A row opens the item, where the date can be changed. With no
    reminders to come the group isn't there. While text is in the search box the page asks for them whatever the
    setting says, so the search finds a reminder with a wrong date; they show in that group, and clearing the search
    puts the list back as it was.

17. A tag in a row filters the list. Clicking it adds the tag to the tag filter (items with any of its tags), and
    clicking it again takes it off; the usual chip under the toolbar follows. While a tag is in the filter, its chips
    in every row keep their colour and are marked as on. A parameter chip does the same with its option: it joins the
    parameter filter, and is marked while it's on. Tags and parameters on the item, Waiting for me and the closed
    list are plain text, and so is a chip of an archived parameter.
18. Sort by offers Priority, by due date, by need, by smallest effort and by largest impact, then each custom
    parameter that has an order. By need lists Critical first, down to Idea; items of the same need keep priority
    order.
19. **Selecting and acting on several items** works on a heap's own list only: not on All items, not on Closed, and the
    reminders to come aren't selectable. **Select** shows when the heap isn't archived and its team has another heap
    that isn't. On a tablet or wide screen it is in the toolbar; on a phone it is in the top bar, with no item open.
    Select mode is a history entry, so Back leaves it, as do closing it, Escape, and Done, Won't do or a move that
    took every item; changing a filter or the sort doesn't. Entering it closes an open item for good, so leaving
    lands on the list, and starts with nothing selected.
    In select mode a row's first slot holds a checkbox, Done and the robot are hidden, swipes are off, and the row is
    a checkbox: a click, a tap or Space toggles it, Shift-click selects a range, and the keys can extend the
    selection row by row. The row's action keys do nothing. Tags and parameters in a row still filter the list. On a
    tablet or wide screen a bar takes the toolbar's place: a box that selects or clears every row shown (mixed while
    some are), the count with Select all, and the actions, **Start** (**Pause** when every selected item is in
    progress), **Done**, **Won't do**, **Send to an agent…** and **Move to…**, and a way out of select mode; the chips
    of the filters stay, and the bar wraps when it is too narrow. On a phone a bar over the top bar has the way out,
    the count and Select all (Clear all when all are), and a bar over the tabs has the same actions under short
    labels, each named in full ("Send to an agent…", "Move to…") for a screen reader. The count is announced. Only
    rows the list shows stay selected: a row a filter or the search hides is dropped from the selection and doesn't
    come back when the filter is taken off.
    Every action is off while nothing is selected and while a bulk request runs. Done, Won't do, Start, Pause and
    Send go item by item, in the list's order, through the item's own endpoints, so each item is checked on its own.
    Done and Won't do touch every selected item, Pause too; Start touches the ones not in progress, blocked ones
    included (starting drops their blocker). Send gives every item its own next goal. No item is left out for not
    allowing the action: the server refuses it and says why.
    Done and Won't do ask first, with the count, in a dialog (a sheet on a phone). Start asks only when an agent has
    some of the items it touches. The ask says how many an agent has and that this takes them back, when it applies,
    and otherwise, for Done and Won't do, that there is no Undo; Cancel returns focus to the button. Pause never asks,
    and Send's form is its question.
    Send to an agent… opens a popover under the button (a sheet on a phone) with how far (one step or all the way),
    an optional line for the agent, Send and Cancel. Send is off while it runs. Closing the popover or the sheet keeps
    the choice and the line for the next open; Cancel and Send drop them, as does leaving select mode.
    When every item changed, a toast says so; after Done and Won't do select mode ends and the rows are gone, and
    after the others the selection stays. There is no Undo. When some were refused (changed by someone else
    meanwhile, closed, or not allowed), a panel under the button, in the send form's place for Send, says how many of
    how many changed, then a line for each refused one with its title and the reason; closing it keeps select mode
    with only the refused rows selected. A failed request, on the network or not, counts as that item's refusal.
    Move to… opens the item's Move picker (The item §6) for the selection, with its own team's heaps only. Picking a
    heap asks first, with the count and that tags they don't have there are dropped. When every item moves, the
    picker closes, select mode ends and the list stays where it is, with the moved rows gone; the toast says so (and
    when some keys changed) and offers to open the heap. There is no Undo. Each item moves on its own, so when some are
    refused (changed by someone else meanwhile, closed, the heap archived) the picker says how many moved, then a line
    for each refused one with the reason; closing it keeps select mode with only the refused rows selected, so Move
    can be tried again. A request that fails as a whole shows its error in the picker, and nothing moved.
20. **Placing a row by hand** moves an item up or down in the ranked group, "Up next" or the untitled group when
    nothing is pinned, and only in the Priority sort. It is off in any other sort, with text in the search box, in
    select mode, on pinned and blocked rows and reminders to come, and for someone who can't edit the items (a reader,
    an archived heap); tag and parameter filters leave it on, as the server places the item next to the row it is
    dropped by. The item lands in the order of the list on screen, so the row stays where it was put as time passes,
    and the server says which way it moved, as the row's first reason. On a wide screen a grip sits at the end of the
    row after the kind and stage icons, in a slot of its own, quiet until the row is hovered or has focus within, and
    above the row's link, so pressing it never opens the item. Dragging it moves the row with the pointer; a line
    shows where it lands, which is always within the group; the pane scrolls near its edge; Escape or a cancelled
    pointer puts the row back and saves nothing, as for a step. A click or Enter on the grip opens a menu to move it
    up or down, each left out at its end of the group, and keys on the focused row move it one place, the focus
    staying on the row. On a phone or tablet there is no grip: a long press (Phone details, gesture 3) lifts the row,
    which then follows the finger with the same line and scrolling, and lifting the finger drops it. A move that lands
    where the row was does nothing. A move saves at once, and the toast offers Undo, which takes the placing away when
    the item had none, and otherwise puts the item back next to the row that was above it (below it, when it was
    first). A move the server refuses, because the item changed meanwhile or the row it was dropped by closed, puts
    the row back and says why.

## The item

1. The item has its own address, and it shows the list with the item over it or beside it, instead of a separate
   page. Links and the back button keep working: going back closes the sheet.
2. It opens to read, in this order: the title with the main state button and ⋯ beside it (point 6), a status line,
   Waits for you when something does (point 12), with the open questions and their reply controls, the fields
   (point 16), the context (point 17), the comments (point 18), a task's Steps (point 11), what it came from and
   became (point 19), Questions, the answered ones and Ask a question (points 13 and 14), Runs (point 20) and History,
   collapsed until it's clicked, on every item it opens. On a phone the main button and ⋯ sit in a bar at the bottom
   of the sheet instead, the main one wider. The **status line** is one quiet line under the title that may wrap:
   the kind and stage (the kind alone when the item is closed), a reminder's date, the state, and for a closed item
   when it closed or that it can't be reopened. A blocked item links to the item it waits for, which opens in the
   pane, and shows the reason in words after it when there is one; it wraps, never shortened. On a blocked item, ⋯
   holds the way to add or change the reason, under Change blocker…: one line, where Enter saves and an empty save
   clears it. Near the server's limit a counter shows; past it Save is off and nothing is cut. It can't be closed
   while it saves; a failed save keeps the draft and says why, and a save shows no toast. Closing it keeps the draft
   for the next open; Cancel drops it, and the saved reason shows again. Then the chips: the item's place in the list
   (point 9), and who has the item when an agent does, or the button that sends it to one when none does (point 15).
   A reminder still ahead says when it joins the list. The header bar names the section the item is in and has no
   rank.
3. Each parameter is picked like effort and impact: a row of option buttons when it has a few options, and a menu
   row when it has more or long labels. A parameter linked to one of the item's tags shows up as soon as the tag is
   added, in Add item too.
4. Changes save as they are made, each with its own `If-Match`. They go one at a time, each with the version the
   previous one brought back, so quick changes don't conflict with each other.
5. Closing it (✕, Escape, or Back) goes back where it was opened from: Back when the app opened it, the list when it
   was opened from a link. Opening another row while one is open replaces it in the history.
6. The main button is the likeliest next state: Start for open, Done for in progress, Unblock for blocked, Make it a
   task for a reminder that has arrived (its state's own choice then moves into ⋯), Reopen for a closed item while it
   can be. A reader, an archived heap or a closed item that can't be reopened has no main button. ⋯ is named More, is
   there whenever it holds something, and holds, each only when the server allows it and in this order: the other
   state changes, Won't do, Make it a task…, Split… on a note, Change blocker… and the reason on a blocked item, the
   Stage choices, and last, in a section of its own, Move to another heap…, when any of the person's teams has a heap
   the item can move to. With none of them, as for a reader's item, an archived heap's or a closed item, there is no
   ⋯.
   **Move to another heap…** lists the heaps that aren't archived of every team the person is in, grouped by
   team, the item's own team first and the others in the order of Teams, the item's own heap marked and disabled, with a
   search once the list is long, and no New heap. A note heaps were made from moves only within its own team, and
   says so. A pick asks first, since tags the other heap lacks are dropped; in another team the confirm also says
   that values that team lacks are dropped, its history stays behind, people there see it from now on but not its
   links to the old team, and a turn someone else started is taken back if they aren't in the new team (a turn
   nobody can be traced for always is). It can't be closed while it saves. The item then opens in its new heap with
   the address replaced, so Back doesn't return to the old one, also from All items, and a toast names where it went
   and, when the new heap already had its key, the key it got. There's no Undo, as the tags can't come back. A
   refusal shows in the picker, above the buttons.
   Block saves the state at once; the blocker is named after, with **Change blocker…**, which opens a picker as Move
   to does, its search focused, over the heap's other open items in list order, without those that wait for this
   one, which would be a cycle. Typing keeps the titles that hold the text, in any case, or says none matches; arrow
   keys move, and Enter or a click saves the pick. Nothing picked comes first while the field is empty, and the
   current blocker is marked and can't be picked again. The field stays put while the list scrolls. Make it a task
   asks first, naming the item, since a task can't go back to a note (or to a reminder).
   **Split…** on an open note has three outcomes: Tasks, A step, and A heap, the last only for someone who may make
   heaps. Tasks and a heap's first items are rows of title, need and age, with nothing preselected and the needs above
   the note's need off; age is the note's, or starts new. Into a step lists the heap's open tasks with a filter and
   takes the note's title and context as they are, at the end of the task's steps. Into a heap starts with the note's
   title as its name and its context, both editable, and first items that are each a task or a note; agents don't
   prepare what's added. Items are made one at a time and the note is closed last; after a failure what was made
   stays, locked, the note stays open, and the rest can be made. A note closed meanwhile says so and offers only
   Close. Leaving a draft asks first. The toast says what the note became, with no Undo, as Reopen is the way back,
   and focus goes to Reopen.
   A reminder has an exact Date in place of Due (point 16), which can be moved, also back into the future, and not
   cleared; it has no steps and can't be sent to an agent. Stage lists as written and with context, and defined on a
   task only (a note says defined needs a task); picking one saves silently, like every change in the item.
7. On a wide screen and a tablet, ↑ ↓ in the pane header walk the list without closing the pane, so triage is open,
   change, next. The arrow keys walk too while the focus is on the pane or its header, not in a field, so the page
   keys scroll a long item there. On a phone the sheet closes and the next row is one tap away, so the sheet has no
   Next button.
8. The key in the header is the button that copies the item's short link, named "Copy link to k3x9a". The header
   stays in view, so the button itself confirms the copy for a moment, and a screen reader hears "Link copied"; a
   copy that fails says so in a toast. Nothing else copies the item's link.
9. The rank chip, the item's place in the status line, is a button that explains it: the contributors and their
   `share_percent` as bars, and below them, what holds the item back from ranking higher: a low need, no due date,
   unset sizes, in that order, as lines of text without bars. What can fix it shows itself on the fields (point 16):
   an unset effort, impact or due date it names is marked there. A low need isn't marked: the need was set on
   purpose. The item shown first in its list has no held-back lines, nor does a blocker ranked by what it blocks, nor
   any item while the list is sorted by something other than priority; with nothing to say the chip is plain text,
   and an item that isn't in the list on screen has no chip. The list row doesn't show any of this.
10. The item says when it was created and by whom, on the line of History's toggle. On a phone, where that line is
    easy to miss, the fields (point 16) end with an Added row as well.
11. **Steps**, on a task, sit inline at every width (on a phone they are not a row that opens a sheet), under a
    heading with the count of point 1. Each step is a row: a grip (editors only), a checkbox that ticks and unticks
    it, the title edited in place (Escape or a blank title puts it back), its context behind a disclosure, closed to
    begin with, and ⋯ with Move up and Move down, then Drop, or Restore on a dropped step. Only the grip drags a step:
    every place can be reached, the list scrolls near the pane's edge, and Escape puts the row back. After a move from
    the menu focus stays on that step's ⋯, and a screen reader hears "Moved to place 2 of 5". A dropped step is
    struck through and muted and stays in the list. Adding a step keeps the field open for the next one; Escape
    closes it without adding and puts the focus back on the button. Nothing is optimistic but a move: a change shows
    once the server has it, and one that fails is said under the section; a moved step shows in its new place at
    once, and goes back if the server refuses. A closed item, an archived heap or a missing `edit` leaves the steps
    and the count to read, with nothing to change. A step's context reads as Markdown text (see Toolkit), for
    everyone, and the disclosure shows only when there is one, or the step can be edited. An editor's Edit swaps the
    text for its source, focused; leaving the field saves a change and goes back to the rendered text once the save
    has landed; a failed save keeps the field and its draft, and Escape there does nothing, so a draft isn't lost to
    a stray key. An editor opening a step with no context gets the field at once. A refetch doesn't touch what is
    typed.
12. **Waits for you**, while the item is open and has open questions, a stuck agent turn or a gone blocker, is what
    the item waits on, at its top, under the status line: one tinted, outlined block, the same on the pane, the
    tablet panel and the phone sheet. Most of the time an item is opened to answer what waits on it, so it is
    answered here, with no scroll. A gone blocker is its first row (see Waiting for me, 8), with Unblock; a stuck turn
    comes next (see Waiting for me, 7), with Retry and Take back. The open questions follow (point 13), under a line
    with their count and **Skip to details ↓** whenever a question waits, on a phone too (the bottom bar keeps only
    the main button and ⋯). Skip scrolls the fields to the top and focuses the first of them; it folds nothing, so
    scrolling up brings the questions back. Opening the item doesn't move focus into the block. The notes a reply can
    leave (someone else replied first, the item was closed first) sit under the block.
13. **Open questions** sit in Waits for you, one row each in number order: the kind, the step it names, the short
    text, its details opening in place, then the reply controls. A question without choices takes an answer in words,
    and Reply is off until it has text. With choices it has radios (one) or checkboxes (any), an optional comment, and
    Reply, off until something is picked. A request has an optional comment and Done and Can't be done, which reply as
    soon as they are pressed. A review and a done proposal are rows as in Waiting for me, 9 and 10; a done proposal's
    toast has no Open, as the item is open already. After a reply there is no toast (but see Close as done): the row
    leaves the block and joins the fold (point 14), the focus goes to the next row's first control, else to the item,
    and a polite live region says Replied to “<text>”; after the last one the count line and Skip go, and the block
    with them unless a gone blocker or a stuck turn is in it. While it is sending, that row's buttons are off. If it
    fails the row stays with its picks and comment and the reason shows under it. If someone else replied first, the
    row leaves and a note says so until the next reply. In an archived heap a row shows its text and details with no
    controls, and Skip still shows. A closed item has no open rows: closing withdrew its open questions, which read
    in the fold as withdrawn.
    **Questions** is a section under the Steps, shown when the item has any question, or is open and the person may
    ask one. While questions are open it starts with a quiet link up to them, which scrolls to the first and focuses
    its first control, for someone who is at the bottom, after asking, say. The fold of point 14 comes next.
    **Ask a question** ends the section, after the fold, on an open item in an unarchived heap for a member (not for
    a viewer, a closed item or an archived heap); with no questions the section is only its heading and the button.
    It opens a form in place, at every width, with the focus in its first field: the kind (Question or Request), the
    one-line text, optional details as Markdown text, and for a question how it is answered: in words, pick one or
    pick any, the last two with choice fields, where Enter adds the next choice rather than sending; blank choices are
    left out, and two that are equal ignoring case are refused. On a task with steps that are not dropped, it can be
    about a step. Switching the kind keeps what was typed. Ask is off until the text, the details and the choices fit
    the server's limits, and while sending; Cancel closes the form and drops the draft. After asking nothing shows
    ahead of the server: the row joins Waits for you at the top, the link up to it appears, the form closes, the focus
    returns to the button, and a polite live region says Asked “<text>”. Escape closes the form while nothing is
    typed; with a draft it does nothing, and the item stays open. If it fails the form stays with its draft and the
    reason shows under it.
14. The fold in Questions, closed to begin with, lists the answered questions and the withdrawn ones in number
    order: the kind, the short text, its details, then what was picked and the comment as Markdown text (see
    Toolkit), and who replied when, or that it was withdrawn; a review also shows its link and piece. On an open item
    each answered question has **Change reply** under its reply (none for a viewer, in an archived heap or on a
    withdrawn question). It opens the reply controls filled with the current reply, with Cancel, which drops the
    draft. After either, focus returns to Change reply, or to the row when the change closed the item (Not yet to
    Close as done); after a change a polite live region says Changed the reply to “<text>”. If it fails the controls
    stay with the draft and the reason shows under them.
15. **Who has it** is a chip in the status line, shown only while an agent has the item: where the turn goes,
    whether it waits for the person, or that it is stuck, in the amber tone. It opens a popover (a sheet on a phone)
    that says since when an agent has it, where it is going and how far, the pull request it watches and what brings
    it back, the person's line, and whether the heap's setting sent it on add. When it waits for the person or is
    stuck it has no buttons: Retry and Take back are in Waits for you. **Take back** is in the popover, and asks first,
    saying the agent stops and what it changed stays; afterwards the chip is gone and the focus goes to ⋯, with no
    toast. With no agent there is no chip and nothing says "With you".
    Before an agent has the item, the same place holds the **Send to an agent** button, in the chip's style, shown
    when the server allows it: not on a reminder, a closed item, in an archived heap, or on a blocked item, which
    can't be sent until it is unblocked. It opens a form (a sheet on a phone) with how far to go (the next stable
    state, on a note as written also defined, or all the way; on a defined task only done), an optional line for the
    agent, Send and Cancel. Cancel returns focus to the button and drops the line and the choice; closing the popover
    or the sheet keeps them for the next open. Send acts at once, closes the form, gives focus to the chip that
    replaces the button and shows a toast with Undo, which takes it back without asking or a second toast. The list
    row's robot sends the same way but for one step with no line, with the same toast. A failed send or take back says
    why in the popover, and the form keeps its line. When someone else sent it first, it says so and the chip shows
    it. Start, Done and Won't do in the item ask before they take an item back, inline under the title or in a sheet;
    Blocked and Open don't.
    **Changes asked…** is a chip beside the Send button, shown when the server offers it: an agent brought the item to
    a stable state short of done and nobody has sent it on. It opens a form like Send's that names the stage the agent
    reached, with a required field for what's wrong, Send back, off until the field has text, and Cancel. Send back
    closes the form, shows a toast, and gives focus to the agent chip, which the new turn brings; Cancel returns focus
    to the button and drops the draft, without asking; closing the popover or the sheet keeps it for the next open. A
    failed send keeps the form and its text and says why; if someone sent the item on first, the form closes, a toast
    says there's nothing to send back, the focus goes to the agent chip, and the Changes asked… chip is gone. A reader
    sees no chip.
16. **Fields** are labelled values that open their editor on a click. Need, Effort and Impact share a row of three
    columns, each a small label over its value; Due (a reminder's Date), Tags and each parameter follow, one row each,
    with the label on the left. On a phone the last row is Added, who and how long ago, in plain text that never opens
    an editor. Wider screens show it only on History's line (point 10). A parameter's label is violet. An unset
    effort, impact or due date reads as a muted invitation to set it; when the server's held-back lines (point 9)
    name it, it is marked in amber. With a mouse a value shows it opens only when pointed at or focused; on a touch
    screen that always shows. A click opens a popover under the value on a wide screen and a tablet, a sheet on a
    phone: the need's four steps, S M L XL (pressing the picked one clears it), the due choices (staying open while an
    exact date is chosen), the date or the parameter's options. A pick saves at once and closes it, and focus goes
    back to the value. Tags are the exception, as the one field with many values: a click turns the row itself into
    the tag picker, on every screen, with no popover or sheet. It offers the team's tags beside the heap's, the team's
    marked, each in its colour, and Create makes a heap tag, which the server colours. Each add, create or remove saves
    at once and keeps it open for the next; it has no Done. Escape or Enter in the empty box closes it with focus back
    on the value (Escape with text only clears it), and so does leaving it, with focus left where it went. Without
    `edit` (a reader, an archived heap, a closed item) the values are plain text.
17. The **context** is the item's description rendered as Markdown text (see Toolkit), with an Edit button, shown
    with `edit`; with none it says so and the button adds one. Edit swaps the text in place for a form with Save and
    Cancel, and on a phone opens the same form in a sheet; it reads and behaves as the heap's Context card does (Heap
    settings). ⌘ or Ctrl with Enter saves, and Escape does nothing, so a long draft is never lost to a stray key or
    closes the item. Save closes the form once the save has landed, the text is read-only while it is in flight, and a
    failed save keeps the draft and says why. When someone else saved a different context meanwhile, it shows above
    the draft, Save becomes Save again and replaces it, and Cancel keeps theirs; Cancel always drops the draft,
    without asking. Until Save again, nothing else saves over theirs (Done doesn't go on, and leaving doesn't save the
    draft). Dismissing the phone's sheet with a changed draft saves it like Save, and stays open if that doesn't land;
    an unchanged draft just closes. Done and Won't do from the pane save a changed draft first, and don't go on if it
    can't be saved. Leaving the item with a changed draft, by closing it or walking on, saves it when nothing else
    changed meanwhile, and otherwise doesn't save it; either way a toast says the context for that item wasn't saved,
    and why. When the item can no longer be edited (closed elsewhere, an archived heap, a changed role) the draft stays
    as read-only text that can be selected, saying it wasn't saved, with a Close button.

18. **Comments**, under the context at every width and always open (not a disclosure, not a sheet row), before the
    Steps, lists the item's comments oldest first, each in its own card, so where one ends and the next starts shows
    at a glance: who, when, and the text as Markdown text (see Toolkit). A comment an agent wrote in a run reads as the
    agent's, as its entry in the history does (point 21): tinted in the agents' colour, with whose agent it is in place
    of the name, a link to the run in Runs. A comment is never edited or deleted, so a row has no controls. The heading
    has the count. Under the list, where the server offers `comment` (also on a closed item), is a field that grows
    with its text and a Comment button, off until the field has text and while it is sending. Nothing is optimistic:
    the comment shows from the server's answer, the field clears, the focus stays in it and a polite live region says
    "Comment added". If it fails, the field keeps its text and the reason shows under it. What is typed lives apart
    from the item, so another edit saving or a refetch doesn't touch it. Where `comment` isn't offered (an archived
    heap) the comments read with no field, and with none the section is absent.
19. **From and Became** sit above Questions, only when they apply, and the items they name open in the pane. An item
    made from another names the note it came from. A note that was split lists the items it became, each with its
    state; a note that became a step names the step's task, with the step's state; a note that became a heap links to
    the heap's list. A step made from a note names it under its context. Each links to the heap that item is in now,
    and both show read-only too.
20. **Runs** is a folded section after Questions and before History, shown only when the item has runs, its heading
    with their count and total cost, when any run reported one. Opened, a row for each run, newest first: when it
    started, from which stage to which goal, how it ended, the tool and model that ran it when recorded, and its cost.
    Under it the result or the failure, and the links. It only reads, for every role, so it has no controls. Each run
    can be linked to (point 21).
21. **History** marks a change an agent made in a run: a robot and whose agent it is in place of the name, linking to
    the run: the item opens with Runs unfolded at that run, which is briefly tinted. Without access to the run's item,
    the mark shows with no link. A person's change shows as before. The heap's Activity marks an agent's change the
    same way.
22. **History shows an edited text as a word diff**: an item's title and context, a step's title and context, and a
    heap's context, in the item's History, the heap's and Activity. Removed words are red and struck through, added
    ones teal, in light and dark; line breaks are kept. These are overdue's red and done's teal, an exception to one
    colour meaning one thing, agreed for the diff on its mockup; they are their own tokens, so they can move apart. A
    long unchanged stretch folds to the words next to each edit, and the rest unfolds on a click. A change of spacing
    alone says only spacing changed; spacing changed beside a word edit shows as the new spacing. Where there is
    nothing to diff (set from nothing, cleared, any other field) or too few words stayed the same for a diff to read
    better than both versions, the change shows as before: the old value struck through, an arrow, the new one.
23. **Full screen** gives a long item room to read, on a tablet or wide screen, from a button in the header. It is
    part of the item's address, so a reload or a shared link opens it the same way. It covers the area beside the
    sidebar or the rail, the item's content in a column of reading width. Going full screen from the pane is a step
    in the history. Escape or the button goes back to the pane, ✕ goes to the list, and Back goes one step back. ↑
    and ↓ keep walking the list in full screen, and leaving after a walk shows the pane of the item walked to. The
    list stays under it, so it keeps its scroll position and selection. A phone has no button, as the sheet already
    fills most of the screen; a full-screen link opened on a phone shows the sheet at full height.
24. An item that was placed by hand (The list point 20) says so, up or down, in a muted line under the status line,
    with a small **Reset** for someone who can edit it. Reset puts the item back by its fields, with Undo in its
    toast, which places it next to the row it had above it (below it, when it was first) in the ranked group on
    screen; with the item not in that group, the toast has no Undo. A closed item shows no line, and the line goes
    with the placing.

## Adding an item

1. The Add item button beside the heap name, or a key, opens the Add dialog: a modal over the list on a wide screen
   and a tablet. On a phone the + in the middle of the tabs opens it as a sheet above the keyboard.
2. The dialog has every field Add item takes: title (focused), context, need, effort, impact, due, tags, and the
   parameters linked to the chosen tags, which appear as soon as a tag is added, each saying which tag asked. Each
   is optional apart from the title and the need: Add stays off until a need is picked. Nothing is picked for you, so
   every need is someone's choice. A line says what the picked need means. On a phone the need, effort, impact, tags
   and due are chips in rows that scroll sideways, effort and impact opening a sheet of sizes; the context and the
   parameters are folded away under More fields.
3. It starts from where it was opened, silently: the heap on screen, and the list's filtered tags, and effort,
   impact or a parameter where the filters pick one value. No label explains the prefill; the tag picker's own
   suggestions are enough. The dialog's title chooses another heap; tags and parameter values belong to a heap, so
   they clear, and the rest stays.
4. Add saves and closes, and a toast says where the item landed, read from the list as the server ranks it, with a
   way to open it. With Add another ticked, the dialog stays open, says the same in a line of its own, and goes back
   to how it started, keeping anything changed while it was being added. Add another is remembered on the device.
   Escape, Cancel or a tap outside closes it; if a title was typed, it asks first. An add that fails says so in the
   dialog, or in a toast once the dialog is closed.
5. **Note | Reminder** sits at the top of the dialog, Note picked. A reminder has a required exact calendar date in
   place of Due, since its date is its deadline; Add stays off until one is picked. Everything else is as for a note.
   Switching back to Note keeps the other fields and doesn't send the date. A reminder for a later date isn't in the
   list, so its toast says the day it joins the list instead of where it landed; one for today or earlier is in the
   list and gets the usual toast.
6. **Agent** sends a note to an agent as it's added, in the same request, so there is no second step to forget. Its
   choices are Don't send (picked) and the three targets, with a line for the agent once a target is picked. On a wide
   screen and a tablet it is the last row of the fields; on a phone it is one chip in a row of its own, opening a sheet
   of the same choices, and the chip then names the target. With a target picked and Note chosen, the Add button says
   it sends too. In a heap whose agents prepare, Don't send says the heap's agents still take it to with context,
   since prepare still opens its turn; a target replaces it. A reminder has no Agent choice and sends nothing, and
   switching back to Note shows the choice as it was. The choice and the line follow a change of heap, and go back to
   Don't send after an add with Add another. An item that was sent says so in the toast, with its target.

## Due

1. **Rough choices first.** The due picker offers the near future as rough choices: In a few days, Next week, In a
   few weeks, Next month, In a few months. Picking one saves a loose due time.
2. **Exact date as the last choice.** Only "Exact date" opens a date field, and saves a firm due time.
3. **A loose due time never shows as a date.** The list, the item and the Add dialog show the server's fuzzy label
   ("in a few days") for it; only an exact due time shows its date, and only it can show as overdue. The item's due
   field opens the same picker, with the current choice selected.
4. **The server owns the choices.** Which dates the rough choices stand for, and what they're called, must agree
   with the fuzzy labels the server gives back, or "Next week" could come back as "in a few days". So the server
   lists the choices, each with its label and date in the request's timezone, and the client only shows them.
5. **Due starts at None** in the Add dialog, with the rough choices one tap away. On a phone's row of due chips
   nothing is picked instead, and a second tap on a chip clears it. In the item, the picker is a sheet with Exact date
   and Clear due.

## Waiting for me

Every open question on an open item in the person's active heaps, answered in its row, every item whose agent turn
is stuck, and every blocked item whose blocker is gone, with the ways out.

1. The heading carries the count (none at zero). The order is the server's: the item's score in its own heap, then
   heap name, position and question number. Rows are one column at the app's content width; no item pane opens
   beside the page.
2. A row is the item's open-question row in Waits for you, the same component, with a head line of its own: the
   question's kind, the item's title as a link to the item in its heap, the heap's name, the team's name only when the
   person is in more than one team, and the step when the question names one. Then the short text, and the reply
   controls as in the item. A row whose question lost its `reply` action shows no controls.
3. **Details** folds the details as Markdown text and under them **Context**, which unfolds the item's context and
   the heap's, each under its own heading. They are fetched when Context is opened, not with the page. A question
   without details shows Context alone.
4. After a reply there is no toast (but see 10): the row leaves, the count drops, the focus goes to the row that took
   its place if it was lost with the row, else to the heading, and a polite live region says Replied to “<text>”. If
   it fails the row stays with its picks and comment and the reason shows under it. If someone else replied first, or
   the item closed first, the row leaves and a note at the top of the list says so; the closed case keeps the comment
   so it can be copied. The notes stay until the next reply or until the person leaves the page. Drafts belong to
   their row and survive a refresh.
5. On a phone every control is a full touch target, Reply spans the width, and the paired buttons (Done and Can't be
   done, a review's, a done proposal's) share a row.
6. With nothing waiting the page says so and what shows up here. A failed refresh keeps the rows. Past 500 rows the
   500 most important show, and the page says how many there are.
7. **A stuck row** is an item whose agent turn failed three runs in a row, so the agents' queue skips it. It is ranked
   among the question rows by the same order, before that item's questions, and counts in the heading and the
   navigation. Its head line says it is stuck, and under it why the last run failed, folded when long. Then
   **Retry**, the primary button, which acts at once, and **Take back**, which asks first inline, saying the agent
   stops and what it changed stays. Retry and Take back make the row leave, as a reply does: the focus goes to the row
   that took its place, else to the heading, and a polite live region says "Retried “<title>”." or "Took back
   “<title>”." A reader sees the row without either button. If someone else took the turn back or retried it first,
   the row leaves and a note at the top says so. A reply draft in another row survives a stuck row leaving.
8. **A blocker row** is a blocked item whose blocker was closed as won't do or became, moved to another heap, or
   closed as done without freeing it (it was blocked before that rule, or its blocker had moved away)
   ([architecture.md](architecture.md#cycle-detection-and-locking) §3): it stays blocked, hidden from the list, until
   the person moves it on. It is ranked and counted as a stuck row is. Its head line says the blocker closed or moved,
   and under it what it waits for and how that ended, the blocker's title a link to it, in the heap it is in now. Then
   **Unblock**, the primary button, which moves the item to open at once, and **Open**, which opens the item to close
   it or pick another blocker; closing it comes back here. Unblock makes the row leave as Retry does, and the live
   region says "Unblocked “<title>”."; if the item changed first, the note says so. In the item's Waits for you the
   same row has Unblock only, as the item is open already. A reader sees the row without Unblock.
9. **A review row** is a question an agent asked for a link to its work. After the short text comes the link, which
   opens the work in a new tab, and the piece when it differs from the link. Then an optional comment and two
   buttons, **Approved**, the primary one, and **Changes asked**, which reply as soon as they are pressed. Approving
   never closes the task: the agent goes on, and proposes closing when nothing is left. The agent asking again after
   changes asked is the next round.
10. **A done-proposal row** is the agent saying nothing is left. Under the short text a quiet summary line: the steps,
    and the reviews by their latest round per piece. The item is fetched for it, one request per proposal; until it
    arrives, or if it fails, the line is left out. Then an optional comment for what is left, and **Close as done**,
    the primary button, and **Not yet**, which sends the task back to the agent with the comment. Close as done is the
    only reply with a toast, which opens the closed item (Reopen is there). Not yet has none.

## Agents

What agents finished across the person's active heaps. Your decision is the items an agent brought to a stable state
that now wait for the person; under it is the history of the runs that ended as reached.

1. Everything is one column at the app's content width, as Waiting for me is, and no item pane opens beside the page.
   A title opens its item over the page, and closing it comes back here. A heap is named by its name, with its team's
   only when the person is in more than one team.
2. **Your decision** is left out when nothing is in it. Its heading folds it and keeps the count, and the person's
   choice is remembered in this browser. A row is the item's title as a link, its heap, the item's kind and state in
   words, or, for a note that became other items, what it made, and how long ago the agent finished it. A row has no
   buttons: the item is where the person sends it on, starts it or closes it.
3. A row is there while the agent's work is the last thing that happened to the item: its latest turn ended as
   reached by a run that ended as reached, the item is not in progress or closed, and it is still at the state the run
   reached. Sending it on, starting it, closing it or setting its stage back takes it out. A turn the person reached
   (by setting the stage, or agreeing to a done proposal), or one a claim found already met, has no such run and no
   row, and a note that became a step of a task has none, as the task already existed. A note that became items or
   heaps is one row standing for them, until any of what it made is sent on, started or closed (being blocked does
   not take it out). The order is Waiting for me's: the item's score in its own heap times the person's priority for
   that heap, highest first, then the heap and the position; a note that became items takes the best of its made items
   that are ranked, and sorts as an unranked item when none is.
4. The first few rows show, and the rest unfold in place. A reload starts folded again. Past 500 rows the 500 most
   important show, and the page says how many there are.
5. **History** lists the runs that ended as reached in the last 30 days, newest first, grouped by day in the person's
   time zone. A row is the time, the item's title as a link and its heap, the state the run reached, the result cut
   short, and the run's links, which open in a new tab. When the run went past its turn's goal the state says what it
   skipped; a run that ended before the state was recorded shows no state. Turns the person reached, and asked or
   failed runs, are not here: asked and failed runs are on the item. A task an agent proposed closing and the person
   closed has no reached run, so it is not in History.
6. Runs load a page at a time. When the last page has loaded the page says older runs are on their items. Cost is on
   the item, not here.
7. With nothing in Your decision and no run in the history the page says no agent has finished anything and how to
   start one in a terminal. Both lists and the count are fetched again on a timer while the tab is visible, and on
   focus; a failed refresh keeps the rows.
8. On a phone a row takes two or three lines: the title with the time or how long ago, then the heap and the state,
   then the result.

## All items

Every open item of the person's active heaps in one ranked list, the server's (the item's score in its heap times
the person's priority for that heap). It has no sort, saved views, heap filter, position numbers or reminders to
come, and a tag or parameter filter only with a team chosen (§7).

1. The heading carries the count (none at zero). The groups are the list's: the pinned one titled as there, Up next
   only under a pinned one, and Blocked. Past 500 rows the page says how many there are.
2. A row is the list's row, with the name of its heap after the tags on a tablet or wide screen and first on the
   second line on a phone; the team's name follows only when the person is in more than one team. It shows no tag or
   parameter chips. Start, Pause, Unblock, Done, sending to an agent, the keys, the swipes and Undo work as in the
   list, in the item's own heap. Done offers no start of the next item.
3. An item opens over the list as on a heap's list (a pane, a panel, a sheet), and nothing switches heap. The address
   names the item by team, heap and key, as a key is only unique within its heap, and anything else there opens
   nothing. The pane's previous and next walk the rows on screen, across heaps, and its position is the one in this
   list. Closing goes back, keeping the rest of the view, and gives the focus to the row.
4. The toolbar is the search and Filters. The panel has Show (Open, In progress and Blocked, each Pinned, Shown or
   Hidden, and the shown blocked items grouped or mixed in) and Narrow down: Need, Effort, Impact, Agent and Going to
   (with a team chosen, also Tags and the team's parameters, §7). Its chips, Reset all and the hidden note are the
   list's. The panel can't say how many items the filters leave out of the total, so it shows the number the view
   matches. The view is in the address.
5. Add opens the Add dialog in the heap the shell keeps (the one opened last), with the need, effort and impact
   filters filled in, and the dialog's title can change the heap. It is disabled when there is no such heap or it is
   archived. On a phone the + in the tabs does the same, with no view. Once added, the toast names the heap, not a
   place, as the item's place in its heap's list isn't its place here, and opening it opens the item over this page.
6. A failed first load shows its message; a failed refresh keeps the rows. The list is fetched again on a timer and
   after any change that changes a heap's list, and keeps its order within the session, as a heap's list does.
7. The list can be limited to one team's heaps. The team is in the address only, never remembered elsewhere, and
   choosing or removing it is a history step, so Back returns to the list as it was; every other change of the view
   replaces the entry. A person in more than one team has a Team row first in Narrow down: All teams, then each team,
   with All teams checked when none is set. A person in one team sees no such row, and nothing else changes for them,
   except that a link with their own team in it works and shows the chip. With a team of the person's chosen, a chip
   takes it off, shown even when nothing else is changed; Reset all leaves the team, and it isn't counted as a filter.
   The count is the team's, the line under the heading names the team, and rows show the heap's name alone. Add goes
   to the kept heap if it is in that team, else the team's first active heap, and is disabled when the team has none,
   as the phone's + and the shell's heap follow. With a team chosen, Narrow down also has the team's tags where a heap
   list shows its tags, and, after Impact, a row for each team parameter in use, with the archived ones' chosen
   options marked as ignored, all as a heap list shows them; their chips and the count in Filters work as there.
   Choosing another team or All teams drops them. A heap that doesn't use a parameter chosen has no rows.
8. Closed and Activity go with the list, for every heap or one team's, reached from the views while on any of the
   three (Layout, All items). **Closed** is a heap's Closed page (Closed) over the same heaps, most recently closed
   first across them: each row ends with its heap's name, and its team's when the person is in more than one and none
   is chosen. Reopen reopens the item in its own heap, and an item opens over its own heap's list, as from that heap's
   Closed. **Activity** is a heap's Activity page over the same heaps, newest first across them, each entry showing
   what that heap's own Activity shows the person, by their role in its team, with the heap named the same way. The
   line under either heading names whose heaps they are, as the list's does. Neither has filters. Archived heaps have
   their closed items and activity on their own pages only.
9. A team the person isn't in (never joined, left, or no such team) is the page's error saying they're not in it, in
   place of the server's message, with no controls and no fallback to all teams; Closed and Activity show the team's
   not-found page.

## Closed

Done, won't-do and became items, most recently closed first, grouped by the server's close group (today, this week,
earlier). One intro line explains the week limit. Each row has the title, which opens the item, how and when it
closed and by whom (for a became item, what it became), and Reopen while the item is inside its week; once it can't
be reopened the row shows nothing more, with no per-row dates or "closed for good" labels. Load more pages. A closed
item opens over the Open list, and closing it returns to Closed.

## Data

What agents cost and how work flows, for a team or one of its heaps, over 7, 30 or 90 days: all the team's heaps,
archived ones included, unless one is chosen, and 30 days unless another period is. Every choice is in the address,
and changing one is a step of the history. It is reached from the heap's views, which opens it on that heap, and
from the Team page's menu, which opens it on all heaps. Any member of the team sees it. On the page the shell keeps
the heap opened last, as on Waiting for me.

Under the title come a heap filter and a period switch. The heap filter lists All heaps, the active heaps, then the
archived ones apart; it is a popover on a tablet or wide screen and a sheet on a phone. Then four totals and four
charts: agents' spend, items added, items closed, agent runs. Closed and runs are stacked by how they closed and how
they ended. Under the spend total, how many runs reported no cost, when any did.

The days are the person's own, from their time zone: one bar per day for 7 and 30 days, the last being today, and one
per week, from Monday, for 90 days (13 weeks). The last bar is the day or week so far, and says so. Days without
anything keep their place with no bar. When nothing happened in the whole period each chart says so.

What each figure counts: spend is the cost of the runs that started in the bucket (a run with no cost adds nothing);
added is items created in it; closed is items whose latest close is in it, by how they closed (a reopened item stops
counting until it closes again); runs are by how they ended, "other" being taken back, handed back, stopped by the
usage limit and still running. An item counts in the heap it is in now, for all its past.

Hovering or focusing a bar (arrow keys move between bars) shows its values, zeros included. On a phone a tap on a bar
puts its values in a line under the chart's title instead. Each chart has its title as its name and its numbers in a
table for screen readers. While another choice loads, the figures stay, dimmed; a failed first load shows the problem
with Try again.

## Teams and Team

Teams lists the user's teams, each with its role, whether it is group-managed, and its members and heaps, with Open
and, for a person in more than one team, All items beside it, which opens All items on that team (All items §7). Create
team takes a name and an optional link name; the server's errors show under the field they name. After sign-up, the
empty heap offers the next two steps, adding a first item and creating a team; once a team exists, the Team page itself
offers New heap and Add member, with no extra prompt.

Among other teams, the personal team shows everywhere as "<name>'s team", by the user's display name whatever it is
stored as, so it has no Rename. The label is the web app's: the stored name is unchanged, because storing "\<name\>'s
team" would need a migration and wouldn't follow a display-name change. Activity entries name the team as stored.

The Team page follows the settings layout ([Rules](#rules) §3): the name with the rarer actions (Rename, Link name,
Activity, Data) under a ⋯, then:

- **Heaps**, with New heap (a name and an optional link name, as Create team has, showing the link a typed link name
  makes; a blank one is made by the server from the name) and a row per heap with its open count, Open and a ⋯
  (Rename, Link name, Archive or Unarchive, History).
- **Tags**, the team's own, used in every heap of the team: added and edited in place like a heap's, saying the change
  reaches every heap of the team, with the same History. A name that heap tags of the team already have opens the merge
  confirm (see Tags).
- **Parameters**, the team's own, which each heap chooses to use: added without a weight, a card per parameter like a
  heap's, with the team tags that suggest it (picked from the team's tags alone, or made there and linked), and which
  heaps use it. Archiving an option there names the heaps whose open items switch.
- **Members**, with a search whose results are a listbox under it (arrows move, Enter adds), member rows with an
  avatar, role, Make admin or Make member and Remove, and Leave team, or why the only admin can't leave. Removing a
  member says which heaps they lose and that their items stay. It's confirmed inline on a wide screen and in the
  member's actions sheet on a phone, where heap rows open the heap.

For an admin, Members also holds **Invite link** beside the search (under it on a phone) and the team's working links
as rows, each saying who made it and when it ends, with Copy and Revoke; the link just made is marked New and takes
focus on its Copy. Revoking is confirmed the same way, saying the link stops at once and people who joined stay, and on
a phone a row's actions are in its ⋯ sheet. A copy the browser refuses shows the link in a field to copy by hand. Every
link has a **History** (made, revoked, each person who joined through it). Links that ended or were revoked are under a
collapsed **Past links**, each saying who made it, when it ended or who revoked it and when, and how many joined. The
team's Activity, shown to admins like other membership lines, says who made or revoked a link and who joined through
whose link, and marks an agent's change like the heap's Activity does.

## Accounts

Accounts is the page of an operator of the server, the person the server's command made one. It lists every account,
so on a server without mail they can make a reset link for someone who forgot their password, and turn a local account
off and on. Anyone else sees no entry and gets the not-found page, since the server answers `404` to everything on it.

**Where it sits.** An entry shown only to an operator, above Teams and Account in the sidebar and the rail, and in More
on a phone. Like Teams and Account, the page keeps the shell without a heap.

**The page** follows the settings layout ([Rules](#rules) §3): a search and a filter (all, active, deactivated,
unconfirmed, operators), then the rows. The server sends the whole list, ordered by name, and the page searches (name,
username and email, as it is typed) and filters it in the browser, since a server has tens to a few hundred accounts.
It shows the first matches, with Load more. A row is the Team page's member row: name, username and email, labels for
provider, unconfirmed, deactivated and operator, the last sign-in, and a ⋯. A deactivated account is dimmed and stays
in the list.

**Actions** are in the ⋯, a menu on a wide screen and a tablet and a sheet on a phone:

- **Make reset link**, for a local, active account other than your own, and only while password sign-in is on. The link
  opens under the row (in the sheet on a phone) with Copy, as an invite link's does, and a copy the browser refuses
  leaves it in the field. The link is shown once and kept nowhere.
- **Deactivate…**, for a local, active account other than your own, asks first, inline under the row (in the sheet on a
  phone), saying how many sign-ins and tokens end and that their items and teams stay. The counts are read when the
  entry is chosen.
- **Reactivate**, for a deactivated local account, acts at once and offers Undo, which turns it off again with no
  question, since nothing is left to end.
- **History**, for every account, opens the account's history under the row, the History card's list, worded for the
  operator.

The ⋯ shows what the account's `allowed_actions` allow. A provider account, and your own row, have History only: the
identity provider owns a provider account's state, and the server refuses turning yourself off. Focus goes back to the
row's ⋯ after Done or Cancel. Errors show under the row.

**What the person sees.** Signing in to an account that was turned off says the server's operator turned it off, with
the operator's contact line when one is set. Their other tabs fall to sign-in on the next request, since every session
of theirs ended.

**History wording.** The person's History card and the operator's list read the same entries, worded by the server for
the reader: the operator sees who did it (or that it was done from the server), the person reads "whoever runs the
server", since it is their own account.

## Sign in and create account

A card on a wide screen and the bare form on a phone, with a Sign in / Create account switch. Sign in takes an email
or a username. Create account takes name, username, email and password; the username is permanent and part of every
link, so its hint says so. A problem about an input shows under it, with focus on it; any other shows above the button.
A taken email offers **Sign in instead**, which switches to Sign in with the address carried over and focus on the
password.

Under Sign in's password, **Forgot your password?** is a disclosure. When the server has mail set up, it holds an email
field and a send button; Enter in the field sends the reset and never the sign-in. Once the server accepts it, a line
says a link is on its way if an account uses that address, without saying whether one does, and stays until the next
send; the form stays, so the user can send again. Without mail, or while the page doesn't know yet, the disclosure says
whoever runs the server can make a reset link, and has no form. Create account has none. The invite page's Sign in
side has it too.

With mail set up, Create account doesn't sign in. The card shows **Check your mail** in place of the forms, with focus
on its heading, naming the address and how long the link works, a **Send it again** button, and a way to change a
mistyped address (below). Signing in with the right password for an account that hasn't confirmed its address shows the
same card as **Confirm your address**. Send it again signs in again with the login and password the form held, which
makes the server mail a fresh link and answer that the address isn't confirmed; that answer counts as sent, and a line
says so anew for each send. If the sign-in goes through (the address was confirmed meanwhile, in another tab) the user
is signed in as a sign-in would. Anything else is the server's message. The invite page's Create account and Sign in
sides do the same.

**Change it** opens a small form under the buttons, either way the card came up: the right address, focused, with a
send button and Cancel. Escape and Cancel close it and put focus back on Change it. It posts the login and password the
form held with the address, and the server sets the address of the still unconfirmed account and mails the link there.
On success the form closes, the card names the new address, a line says it was sent there, and focus goes back to
Change it. The card then signs in with the account's username, since the login may have been the mistyped address. An
address another account has looks the same as a free one, so nothing says it was refused: whoever mistyped twice
changes it again.

On a server with registration closed, the sign-in page has no switch and no Create account: Sign in shows alone, with a
footnote that new accounts need an invite link from a team's admin. With a provider there is no footnote, since a first
sign-in through it makes the account. The invite page keeps its Create account, since the link makes the account. While
the page doesn't know yet, the card holds only what it holds while it waits for the provider; if the read fails, the
switch shows. A sign-in the server throttles says how long to wait, counting down from the server's wait, and Sign in
is disabled until it ends; editing a field doesn't clear it.

When the server has an identity provider, the card starts with a full-width button to sign in through it, named for
the provider, and an "or" rule under it, above the switch; the dark Sign in button stays the main one. It is a plain
link that leaves for the provider, carrying the page to return to. Without a provider, or until the page knows, there
is neither. A sign-in the provider or the server refused comes back to this page with one line above the button,
announced as an alert, that says why; when an account with that email exists with a password, Sign in is selected, the
login is filled and focus is on the password. The line goes when a form is submitted. When the server signs in through
its provider only, the card holds the button and that line (and the operator's contact line under a taken email) and
nothing else: no rule, no switch, no forms, no Forgot your password?. The invite page's signed-out state is the same,
its button still returning to the invite.

Wherever a page says to ask whoever runs the server (the forgotten-password line, the provider refusal, the reset page,
a team with no admin), the operator's contact line, when set, shows under it as its own muted line. The mail that tells
an old address it was changed carries it the same way.

A first sign-in through the provider continues in the same card: **Pick a username**, who the provider signed in (name
and email as plain text), the username input with the same hint, focused and empty, Create account, and Cancel back to
sign-in, which sends nothing. A problem about the username shows under it with focus on it; any other, above the
button. When the step has ended (it lasts minutes, or it was used) the page says so and offers to sign in again.
Success goes where the sign-in began; the invite page, where a sign-in that began there comes back, shows the signed-in
Join step.

A signed-out link says where it leads and opens it after sign-in; an expired session returns the same way.

A group-managed team (its members come from the identity provider's groups) takes nobody by hand. Under the Members
heading one line names the groups, where the search and Invite link would be, with no invite links and no Past links;
the ⋯ on the name has no Link name, since the server leaves that action out. Removing a member adds that they come
back at their next sign-in if their groups still include the team, on every Remove, because the page can't tell who
signs in through the provider. Members and admins both see the line. Its Activity says who joined or left through which
group, and a role change made from the server's command. A shared team with no admin among its members, which a sync
can leave, says so to every member above that line, and that whoever runs the server can make one. The phone shows the
same lines in the same places.

## Invite page

The page an invite link leads to, outside the app's shell like sign-in and in the same card, needing no sign-in. The
server decides the state (the link's state, the viewer's standing and `allowed_actions`), in this order, and the page
only shows it:

- **Checking the link** while it loads, with no form.
- **Not an invite link** for any token the server doesn't know, naming no team.
- **Already a member:** no page; they land in the team (below), told they're already in it, whatever the link's state.
- **Group-managed team:** the link no longer works because the team's members come from its groups, with no form,
  whatever the link's state: the server says so with `group_managed`, and the link works again if the team is unmapped.
- **Ended or revoked:** says which, who the link was for and from, and to ask the team for a new one.
- **Removed:** the link can't add them back; ask one of the team's admins.
- **Signed in:** asks to join, who invited them, and who they're signed in as, with Join and a way to sign out, which
  reloads the same page signed out.
- **Signed out:** the team and who invited them, over the provider button (when the server has one; it returns to this
  page signed in) and the sign-in switch with Create account first
  ([Sign in and create account](#sign-in-and-create-account)). Create account and join makes the account and joins in
  one act, also when registration is closed; with mail set up the account is made and joined, and the card is **Check
  your mail** as a state of this page (the team is in the sidebar once the address is confirmed). A taken email offers
  Sign in instead, which carries the address over. Signing in leads to the signed-in state, where they press Join: it
  doesn't join by itself, so they see who they joined as.

The states that can't be used end in an **Open Upheap** button. A failed read says why, with Try again. A link that
stops working while a form or the confirm is open, or a session that ends, refetches the invitation and the page
becomes the state that says so; it never sends the user to sign-in. When the page changes state, focus moves to its
heading.

A new member lands on the team's heap when it has exactly one that isn't archived, otherwise on the Team page, which
lists the heaps. Landing replaces the invite page in the history. A toast says they joined (or were already in it),
belongs to the page they land on like any toast, and is shown once: a reload doesn't repeat it.

## Reset page

The page a password reset link leads to, outside the app's shell like sign-in and in the same card, the same at every
width, and needing no sign-in: holding the link is the permission, so a signed-in visitor sees the same page. The link
comes from the operator, who makes it with the server's command (accounts.md, Resetting a password), or by mail, when
the user asked for it on the sign-in page. The two are the same kind of link. The server decides the state, and the
page only shows it:

- **Checking the link** while it loads.
- **Not a reset link** for any token the server doesn't know: check the whole link was copied, or ask for a new one.
- **Used**, **cancelled** (a newer link was made, or the password or address was changed since) and **ended** say
  which, and to ask whoever runs the server for a new one.
- **Working:** who the link is for, the new password input, focused, with its rule as a hint, the button that sets it
  and signs in, and under it that this signs you out everywhere else.

The states that can't be used end in an **Open Upheap** button, which leads to sign-in when signed out. A failed read
says why, with Try again. A problem about the password shows under the input, marked invalid and focused, with what
was typed kept; the link stays working after it. A `404` or `409` means the link changed meanwhile (another tab used
it, it ended), so the page reads it again and becomes the state that says so; it never shows the raw error. Anything
else is the server's message above the button. When the page changes state, focus moves to its heading. A `401` on
this page never sends the user to sign-in.

Setting the password signs the browser in as the link's user, whoever it was signed in as, and ends every other
sign-in of that user. It lands on the user's heap, replacing the reset page in the history, with a toast that the
password changed and they're signed in, shown once like the invite page's.

## Change-email page

The page an email change link leads to, outside the app's shell in the Reset page's card, the same at every width, and
needing no sign-in: holding the link is the permission. Nothing is sent when it opens, so a mail scanner that fetches
the link changes nothing: the page asks to use the address the link was mailed to, with one button. What the server
answers is the state:

- **Changed:** names the new address, with a way to the account page (and so to sign-in when signed out). A signed-in
  tab reads `me` again for the new address.
- **Not a working link** for a token the server doesn't know: check the whole link was copied.
- **Used**, **cancelled** (a newer link was asked for) and **ended** show the server's words as the heading, with the
  way to the account page.
- **Address taken** since it was asked for: another account uses it now, so the address stays the same, and no button.
  The link keeps working until it ends.

Anything else is the server's message above the button. When the page changes state, focus moves to its heading.

## Confirm page

The page an email confirmation link leads to, outside the app's shell in the Change-email page's card, the same at
every width, and needing no sign-in: holding the link is the permission. Nothing is sent when it opens, so a mail
scanner that fetches the link confirms nothing: the page asks to confirm the address and sign in, with one button. What
the server answers is the state:

- **Confirmed:** the browser is signed in as the link's user, whoever it was signed in as. It lands on the user's heap,
  replacing the page in the history, with a toast shown once like the reset page's.
- **Not a working link** for a token the server doesn't know: check the whole link was copied.
- **Used**, **cancelled** (a newer one was mailed, which a refused sign-in or Send it again does) and **ended** show
  the server's words as the heading, with **Open Upheap**.

Anything else is the server's message above the button. When the page changes state, focus moves to its heading. A
`401` on this page never sends the user to sign-in.

## Email

Account's Email card sits between Profile and Password, and replaces Profile's Email row. It is there when the server's
`change_email` action is: the user has a password and the server has mail set up. Without it the address stays a
read-only row in Profile. Closed, it is the heading and a Change button on one row, and the address under them. Change
opens a form in its place: the new address (focused) and the current password, then Send link and Cancel. The password
is sent as typed.

Errors sit under their input, which is marked invalid and described by them, and focus moves there. Typed values stay,
except the current password after a wrong one, which is cleared and focused. Anything else is the server's message
above the buttons. Cancel and Escape close the form, clear it and give focus back to Change.

A success closes the form and says, under the address, to check the new address: the address changes when the link is
opened. The line stays until the form is opened again or the page is left, and focus goes to Change. The address in the
card stays the old one until the link is applied ([Change-email page](#change-email-page)); asking for an address
another account has looks the same, and no mail comes.

## Password

Account's Password card sits between Email (or Profile, without one) and Appearance. Closed, it is the heading and a
Change password button on one row. The button opens a form in its place: the current password (focused) and the new
one with its rule as a hint, then Change password and Cancel, and under them that this signs you out of your other
browsers and the command line. What was typed is sent as typed.

Errors sit under their input, which is marked invalid and described by them, and focus moves to the first one. Typed
values stay, except the current password after a wrong one, which is cleared and focused for a password manager to
fill again. Anything else is the server's message above the buttons. Cancel and Escape close the form, clear it and
give focus back to Change password.

A success closes the form and says, under the heading, how many other sign-ins ended, until the form is opened again or
the page is left. Focus goes to Change password, and the Command line card is read again, since the change ended its
sign-ins. The browser the change was made in stays signed in, on a new session cookie the server sets; another browser
of the user lands on sign-in at its next request. A change also cancels the user's working reset link
([Reset page](#reset-page)), so a copy of it can't be used afterwards. Without the server's `change_password` action,
as for a person who signs in through the identity provider and has no password, the card keeps its heading and says,
where the button would be, that they sign in through the provider.

## Command line

Account's Command line card lists where the `upheap` CLI is signed in as the user. A row has the machine's name (the
client's name when it sent none), when it signed in and was last used, and a **Sign out** button named for the machine.
The server words the name and both dates, in the request's time zone. The button turns the row into an inline confirm
that names it and says the CLI there stops working until someone signs it in again; one row confirms at a time, and
Cancel gives focus back to the row's button. While a sign-out is pending, the other rows' buttons are disabled. After a
sign-out the card says so in a status line and focus moves to its heading, since the row it was on is gone. With none
signed in, the card says so and shows the command to start with, with this server's address. The card is there when
the server's `manage_sign_ins` action is. Ending a sign-in never touches the browser's own session.

Below the sign-ins the card has a **Tokens** part for machines with no browser or keychain: the user's personal tokens,
one row each with the name, when it was made, last used and expires, and a **Revoke** button named for the token. Revoke
turns the row into an inline confirm that says anything using it stops at once, and shares the one open confirm with
the sign-ins. After a revoke the card says so in its status line and focus moves to its heading. **Make a token** opens
a form in place of the button, with a name and an expiry; Cancel or Escape closes it and gives focus back to the
button. A refusal shows in the form, which keeps what was typed. A made token replaces the form with a panel that says
it is shown once, the secret in a read-only field that selects itself on focus, a Copy button that has focus (the
field is selected for copying by hand when the clipboard isn't there), and the command to use it, without the secret.
Done closes the panel and gives focus back to Make a token. The secret lives only in the page's state, so leaving the
page drops it.

## History

Account's last card, after Command line and before Sign out, holds one collapsed disclosure of changes to the account,
like the ones on a tag or a member. It loads only when opened, and lists the user's password events, their address
changes, the sign-in provider or an operator turning the account off and back on, the operator role given or taken
away, and their own heap priority changes, newest first, each a sentence from the server and its time. It names no
actor, since it is the user's own account, and holds nothing else of the account (not sign-ups or profile changes).
A password change, an address change asked or a priority change adds its entry to an open History at once, and to a
closed one by the time it is opened.

## Conflicts and errors

1. **Conflicts say what was lost:** who changed what, that you're seeing their version, and which change of yours
   wasn't saved, with a one-tap retry. The pane re-renders from the server.
2. **Loading** keeps the previous view, dimmed, rather than a blank page.
3. **A `401` mid-session** sends the user to sign-in with the way back, as a signed-out visit does, dropping the
   last user's cached data; signing in returns them to the page.
4. Other item errors show inline in the pane with the server's message: a closed item only changes state, a reopen
   past its week, an option archived meanwhile.
5. **Not found.** An address nothing answers, a team, or a heap the server answers `404` for shows a page in the
   content column, inside the shell, with a way to Waiting for me; a team or heap also gets a quiet way to the teams.
   A missing and a not-yours team or heap read the same, as the server's `404` does. A missing item stays in the item
   pane, saying it doesn't exist or is in a heap you can't see, with a button back to the list, and the list behind
   stays.
6. **A banner when the connection is down,** at the top of the shell, above everything, in the page flow. It says
   whether the browser is offline or can't reach the server, and that changes won't save until it's back; the second
   has Try now, which refetches what is on screen. A proxy's own `502`, `503` or `504` (no problem body: the server
   process is down) counts as unable to reach the server. Offline wins when both hold. The banner is a `status`, and
   says nothing when it goes: the page refetches when the connection returns, on focus and on Try now. There is no
   queue: an edit while offline or unable to reach the server fails at once, saying it wasn't saved and why, and the
   draft stays where it was typed. Loaded data stays on screen; a page not visited yet waits while offline.
7. **A server error shows its request ID** under its words, for every `5xx` the server sent one with: never cut,
   plain text to select, with a Copy button. A toast with an ID stays until it is closed. Offline and unreachable
   errors have no ID.

## Rules

1. **Confirmations.** A reversible change acts at once and offers Undo in a toast: state changes, archiving a heap,
   a view or a parameter. A change that can't be undone, or that affects other people, asks first, inline or in a
   dialog on a wide screen and in a sheet on a phone, and names the object: removing a member, giving up your own
   admin role, revoking an invite link, and discarding a typed title. Cancel on a form that edits in place (Ask a
   question, Change reply, Changes asked, an item's or a heap's context) is itself the choice to discard, so it
   drops the draft without asking; such a form guards its draft from a stray key and from refetches instead.
   Closing a dialog or a sheet that holds a draft, or leaving the page with one, never drops it silently: Add an
   item and Split… ask first, the item's context saves the draft, and a popover form (Changes asked, Send to an
   agent from the item or a selection, Why it's blocked) keeps it and shows it again when it reopens, until it is
   sent or cancelled. A reload or opening another item still drops these short drafts.
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
   the ring before its old value, a list row in its state slot. It shows after a short delay, so a quick answer shows
   no flicker, and stays until the answer and the refetch it starts have landed; then the new state replaces it. The
   control ignores presses meanwhile and isn't faded, as a control disabled for another reason is; its label doesn't
   change. On a failure the ring goes, the old value stays and the error shows where it always did. With reduced
   motion the ring pulses instead of turning.

## Keyboard

Keys come on top of clicks, never instead: everything a key does is a click or a tap away too.

1. The wide screen shows its keys, with a mouse or trackpad, as small chips on the controls they press and at the end
   of the focused row.
2. The list and the open item move with the arrows, and a row's item can be moved up or down by key (The list
   point 20), on any width with a keyboard.
3. A hint bar at the bottom of the window, under the heap's list however short it is, has the keys for what's on
   screen, and ends with the one that lists them all.
4. `?` anywhere in the app lists every key (`KeysHelp.tsx` holds them). Keys are ignored in a field. Open and Closed
   have no keys: they are one click away in the sidebar.
5. **Find** opens from a key anywhere, from the top of the sidebar, or from a phone's top bar. It finds items by title
   or key in every heap, goes to a heap, and adds the typed text as a new item in the heap on screen. The highlighted
   item's actions (as the item allows) are a key away, and back. Closed items come after the open ones, in the
   server's order, under their own heading; a closed row's title is muted and its state sits just before the key at
   every width, since a phone hides the heap name. Moving through the results skips the heading. Open, Find is a
   history entry, so Back closes it; going to an item or a heap takes its place, so Back from there returns to where
   it was opened. On a phone it opens at the top, full width, and ends above the on-screen keyboard, with the results
   scrolling; clearing an empty field closes it, and a tap outside closes it too.
