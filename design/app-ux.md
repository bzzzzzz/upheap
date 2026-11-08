# Web app UX for phones and wide screens

Draft. A change spec on top of [technical-design.md](technical-design.md). Status lives in [progress.md](progress.md).
The mockups are in the "Upheap layouts" canvas linked from the pull request that adds this file.

## Goal

Make the web app work well at every width, from a phone to a wide screen, in both builds (the server's and the
[local build](local-build.md)). It stays a website: one React app whose layout changes with the width.

Making it an installable app comes after this and is out of scope for now ([Later](#later-an-installable-app)).

### Non-goals

1. A second UI. One React app serves every width; the layout changes, the components don't fork.
2. Client-side ranking or rules. The server still decides order, permissions and states; the client adds layout,
   gestures and animation only ([engineering-principles.md](engineering-principles.md)).
3. Installing, offline use and app stores, for now.

## What's wrong today

The app is one 768 px column with a top bar of text links. That works on a laptop and breaks down elsewhere:

1. **Every row is a form.** Effort, Impact and Due pickers and a state select sit in every row. On a phone a row is
   five lines tall and shows four items per screen. The list should be for reading and ordering; editing belongs to
   the item.
2. **The navigation wraps.** On a phone the header wraps onto two lines of small links.
3. **Targets are small.** Native selects of 20 px height, text links of 14 px.
4. **Wide screens are wasted.** At 1280 px half the width is empty, and editing an item leaves the list.

## Toolkit

1. **Tailwind v4, kept**, with our design tokens in `@theme` (surface, ink, muted, accent, state colours, radii), so
   light and dark modes come from one place instead of `dark:` pairs on every element.
2. **React Aria Components** for behaviour: dialogs and sheets, menus, radio and toggle groups, date picker, tag
   group, and the list itself (`GridList`, for keyboard navigation and selection). It is unstyled, so it takes our
   Tailwind classes, and it is one dependency rather than copied code. It handles touch properly: a scroll doesn't
   fire a press, focus stays sane when the on-screen keyboard opens, and every control is accessible. The
   alternative is shadcn/ui (Radix plus copied components); it's popular but puts a component library's worth of code
   in our tree.
3. **Motion, kept**, for sheet drag, swipe rows and the list's reorder animation.
4. **lucide-react** for icons.

## Layout

Breakpoints are Tailwind's: `sm` 640 px and `lg` 1024 px.

| Width | Navigation | List | Item |
| --- | --- | --- | --- |
| Phone, under 640 px | Heap name at the top switches heap; bottom tabs Open, Closed, + (Add), Activity, More | Full width, compact rows | Bottom sheet over the list |
| Tablet, 640–1023 px | An 80 px rail: the heap button, then the views as icons with labels, account at the bottom | Beside the rail, with the wide screen's toolbar and rows | Panel over the right of the list, the list still live |
| Wide, 1024 px and up | Sidebar: heaps, then the heap's views, account and the storage note at the bottom | Middle column | Pane beside the list while an item is selected |

**More** on a phone holds Team, Teams, Tags, Parameters, Account (with Your data in the local build) and Sign out,
as a card of 52 px rows above the tabs. The **+** in the middle of the tabs opens the Add dialog from any of the
heap's pages, so the Add field above the list shows only from a tablet up.

On a wide screen the sidebar and the content are one centred block, at most the sidebar plus the content column
wide, so the menu stays next to what it opens; the sidebar's colour runs on to the left edge. The item pane widens
the block when it opens.

### Tablet

A tablet gets its own layout rather than a stretched phone or a squeezed wide screen:

1. **The rail.** The sidebar folds into an 80 px rail. Its top button shows the heap's initial and opens the heap
   list as a popover; below it are the heap's views (Open, Closed, Activity, Tags, Parameters) as icons with short
   labels; the account sits at the bottom.
2. **The list** takes the rest of the width, with the wide screen's toolbar (search, Filters, View) and rows,
   including the effort and impact sizes, at touch sizes (44 px targets).
3. **The item** opens as a 400 px panel over the right of the list. The list beside it stays live: tapping another
   row switches the panel, and the selected row stays highlighted. Swiping the panel right or tapping ✕ closes it.
   Its controls are the phone sheet's (Start, Blocked, Done buttons; S M L XL buttons).
4. Swipe actions on rows work here too, as on a phone.

### Phone details

Phone pages respect the safe-area insets (`viewport-fit=cover`) under the bottom tabs and sheets, and inputs are at
least 16 px so iOS doesn't zoom in on focus. Sheets sit above the on-screen keyboard: Android Chrome resizes the page
for it (`interactive-widget=resizes-content`), and on iOS, which doesn't, the sheets are lifted by the height the
keyboard covers, from the visual viewport. The heap switcher and More open as sheets too; More's sits above the
tabs. Lists in a sheet (heaps, due choices, a parameter's options) are 52 px rows with a check on the current one.

Gestures are offered where the pointer is coarse (a finger), on phones and tablets alike:

1. A row dragged right past about 100 px slides away and marks the item done; dragged left, it stays open on two
   buttons, In progress and Blocked (Open in place of the state it's in), until one is picked or the row is tapped.
   Each is followed by the undo toast. A tap just after a drag doesn't open the item.
2. The item sheet drags down by its top bar to close; the tablet panel drags right. Letting go short of 120 px
   springs it back.

### The list

1. A row is one or two lines: a state circle, the position, the title, the due chip and tags, and below them the
   server's contributors ("Due this week · small effort"). On a wide screen the effort and impact sizes sit at the
   right edge.
2. Custom parameters with a value show as violet "Name: Option" chips ("Energy: Low") after the tags on a wide
   screen, and on their own line under the contributors on a phone or tablet. Unset parameters don't show. Violet is
   kept for parameters everywhere, so they read apart from tags (grey) and due dates.
3. The state circle marks the item done in one tap. An undo toast follows every state change from the list.
4. Tapping or clicking a row opens the item. Keyboard on a wide screen: up and down move, Enter opens, `x` marks done;
   `N` and `/` stay.
5. On a phone, swiping a row right marks it done; swiping left offers In progress and Blocked.
6. Saved views stay chips above the list, scrolling sideways on a phone.
7. Filters and View open as a popover on a wide screen and as a sheet on a phone.

### The item

1. The item route (`/teams/$team/heaps/$heap/items/$item`) shows the list with the item over it or beside it, instead of a
   separate page. Links and the back button keep working: going back closes the sheet.
2. It opens with the state (the likeliest next state as the main button, then the others, with Won't do under ⋯, on
   a wide screen; on a phone the same buttons sit in a bar at the bottom of the sheet, the main one wider, with Won't
   do under ⋯, and the state is a line under the title), then
   "Why it's #3" from the contributors, then the need's four steps, effort and impact as S M L XL buttons, due, tags,
   parameters, the
   description and the history. On a phone due, tags, parameters and the description are rows of label and value,
   each opening a sheet; the description saves as its sheet closes.
3. Each parameter is picked like effort and impact: a row of option buttons when it has a few options (up to four),
   and a menu row when it has more or long labels. A parameter linked to one of the item's tags shows up as soon as
   the tag is added, in Add item too, as it does today.
4. Changes save as they are made, each with its own `If-Match`. They go one at a time, each with the version the
   previous one brought back, so quick changes don't conflict with each other.
5. Closing it (✕, Escape, or Back) goes back where it was opened from: Back when the app opened it, the list when it
   was opened from a link. Opening another row while one is open replaces it in the history.

### Adding an item

1. Clicking or tapping "Add an item" (or pressing `N`) opens the Add dialog: a modal over the list on a wide screen
   and a tablet, a sheet above the keyboard on a phone. The field above the list stays only as the way in.
2. The dialog has every field Add item takes: title (focused), description, need, effort, impact, due, tags, and the
   parameters linked to the chosen tags, which appear as soon as a tag is added. Each is optional apart from the
   title and the need: Add stays off until a need is picked. Nothing is picked for you, so every need is someone's
   choice. Under the four steps a line says what the picked one means. On a phone the need, effort, impact, tags and
   due are chips in rows that scroll sideways, effort and impact opening a sheet of sizes; the description and the
   parameters are under More fields.
3. It starts from where it was opened: the heap on screen, and the list's filtered tags, and effort, impact or a
   parameter where the filters pick one value. Nothing says so. The title, "Add to \<heap\> ▾", chooses another heap;
   tags and parameter values belong to a heap, so they clear, and the rest stays.
4. Add saves and closes (`⌘↵` / `Ctrl↵` on a keyboard), and a toast says where the item landed ("Added “X” at #4 in
   Up next", read from the list as the server ranks it) with Open it. With "Add another" ticked, the dialog stays
   open, says the same in a line of its own, and goes back to how it started, keeping anything changed while it was
   being added. Add another is remembered on the device. Escape, Cancel or a tap outside closes it; if a title was
   typed, it asks first. An add that fails says so in the dialog, or in a toast once the dialog is closed.

### Due

1. **Rough choices first.** The due picker offers the near future as rough choices: In a few days, Next week, In a
   few weeks, Next month, In a few months. Picking one saves a loose due time (`firm: false`).
2. **Exact date as the last choice.** Only "Exact date" opens a date field, and saves a firm due time.
3. **A loose due time never shows as a date.** The list, the item and the Add dialog show the server's fuzzy label
   ("in a few days") for it; only an exact due time shows its date, and only it can show as overdue. The item's due
   field opens the same picker, with the current choice selected.
4. **The server owns the choices.** Which dates the rough choices stand for, and what they're called, must agree
   with the fuzzy labels the server gives back, or "Next week" could come back as "in a few days". So the server
   lists the choices (`GET /due-choices`), each with its label and date in the request's timezone, and the client
   only shows them. The fuzzy terms include "next week" and "next month", and their bands leave room around each
   choice, so a picked choice reads back as itself whatever the time of day.

## Testing

The Playwright golden paths run on phone (Pixel 7 emulation) and tablet (iPad emulation) projects as well as desktop
Chromium, against both the server and the local build.

## Plan

1. **Foundations.** Tokens, icons, React Aria, and the app shell at each width: sidebar, rail, bottom tabs, safe
   areas. The phone and tablet Playwright projects start here.
2. **List and item**, in two changes:
   1. The due picker and its choices from the server, the S M L XL buttons, and the Add dialog.
   2. Compact rows with the state circle, the item pane and sheet in place of the Edit item page, undo toast,
      keyboard navigation.
3. **Phone interactions.** The Add dialog as a sheet above the keyboard, swipe actions on rows, dragging the item
   sheet down and swiping the tablet panel right to close them, Filters and View sheets, heap switcher.
4. **Every other page at phone width.** Closed, Activity, Tags, Parameters, Team, Account, sign-in.

## Later: an installable app

Once the website works at every width, the local build can become a Progressive Web App (PWA): the same site,
installable from the browser and working offline. App-store builds can follow by wrapping it in Tauri 2. React Native
is not the route: it would mean rebuilding every screen without the DOM, and its JavaScript engine can't run the local
build's WebAssembly backend. Nothing in the plan above is thrown away by either step.

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
