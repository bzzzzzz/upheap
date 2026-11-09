# Product

What Upheap does and why: the commitments the product makes, whether or not the code has caught up with them.
[progress.md](progress.md) says what's built, and keeps the ideas that aren't commitments. How it's built is in
[architecture.md](architecture.md), how the web app behaves in [ux.md](ux.md), and the command-line client in
[cli.md](cli.md).

## Pitch

Upheap is a todo list for people with more work than they can keep track of. You add items as they come up, with how
much they're needed and a rough size, impact and due date. The app ranks everything by how much it's needed, how long
it has waited, how close its deadline is and how much it's worth, and keeps that ranking current as time passes. A
heap can be personal or shared by a team.

It answers questions like:

- What should I focus on today?
- What's the most important thing on my plate right now?
- What's next after I finish this?
- Why isn't this item higher on the list?

## Distribution

1. Upheap is free and open source.
2. Teams self-host it, on Postgres, MySQL or SQLite. There is no hosted or paid tier.
3. One person can also use it without hosting anything: a static, single-user build runs entirely in the browser and
   keeps the data there ([architecture.md](architecture.md#local-build)). It has no accounts and no teammates, all
   its heaps belong to one team, and it is not a hosted tier, since no data leaves the browser.
4. The server exposes an API that any client can use. There are two clients, the web app and the `upheap`
   command-line client ([cli.md](cli.md)), and the API assumes neither.

## Accounts and sign-in

1. The app runs self-contained, with its own accounts: an email address, a username and a password.
2. Registration is open by default. The operator can close it, and then only existing users sign in.
3. Users can also sign in through a third-party identity provider, for example Okta. Signing in through one for the
   first time creates the user record.
4. Non-web clients such as the CLI sign in through the browser: the client opens the app's sign-in page, the user
   signs in there, and the backend hands a token back to the client (OAuth authorization code flow with PKCE). The
   client never handles the user's password.

## Teams

1. Users form teams. A team has one or more admins.
2. Any user can create a team and becomes its admin.
3. Every new user gets a personal team, named by their username, with a first heap called "Personal". The personal
   team is theirs alone: it holds as many heaps as they like, but it can't be shared. Anything shared lives in
   another team.
4. A team owns its heaps, and can have more than one.
5. A heap belongs to exactly one team, and only that team's members can access it. Everyone in the team sees the
   same items and ranking in its heaps, and can add and edit items.
6. Users join a team only when an admin adds them: the admin picks an existing user, who is added directly.
7. A team admin can make any other member an admin, remove admin rights from another admin, and remove another admin
   from the team.
8. A team always has at least one admin. The last admin can't leave, be demoted or be removed until another member
   has been made admin.
9. Heaps are never deleted. Admins can archive a heap, and its audit history stays. An archived heap is read-only,
   and an admin can unarchive it.

## Heaps

1. A heap is a collection of items with its own ranking, tags and custom parameters.
2. Every heap is owned by a team (Teams §4–5). A personal heap is one in the user's personal team.
3. A person can belong to several teams and heaps, and switches between them.
4. Items have no assignee or owner. A shared heap has one ranking for the whole team, and "my plate" means the heaps I
   belong to. This is deliberate.

## Items

1. An item is a finishable piece of work with a title and an optional description.
2. An item can have an effort size and an impact, each on the scale S, M, L, XL.
   - Every item says how much it's **needed**, picked when it's added: what happens if it never gets done. Need is
     separate from impact, which is what doing the item gains; need is what skipping it costs, so an item nobody
     would miss can be valuable, and an urgent one can be small. The steps, from least to most needed:
     - **Idea:** kept so it isn't forgotten. Nobody has decided it's wanted, and nothing is lost if it never happens.
     - **Nice to have:** wanted, but nobody is waiting for it. Skipping it only misses a gain.
     - **Expected:** someone is counting on it. Skipping it lets them down.
     - **Critical:** it has to happen. Skipping it does real damage: an outage, a lost customer, a broken commitment.
   - Each step is told apart from the next by who is affected when the item is skipped: no one, no one yet, someone,
     or the team and the people it serves.
   - Need can be changed but not cleared.
3. An item can have a due time, given either loosely or as an exact deadline date.
   - A loose due time is picked from rough choices ("next week") rather than a date, and reads back in fuzzy terms
     such as "in a few days", "next week", "in a few weeks", "next month" or "in several months", with the term it
     was picked by.
   - A loose due time never becomes overdue. Once its date has passed, it keeps showing the nearest term, for example
     "in a few days".
   - An exact deadline can become overdue.
4. Effort, impact, due time and custom parameters can all be left unset; need can't. An unset value counts as the
   lowest-priority option for that parameter, and the interface shows it as unset.
5. An item can have any number of tags, used to group similar items. A tag has a name and an optional description.
6. An item is always in one state: open, in progress, blocked, done or won't do. Done and won't do are closed
   (terminal) states. Items are never deleted. Work that won't happen is closed as won't do.
7. A closed item can be reopened within one week of being closed. After that it can't be reopened.
8. A blocked item can point to another item as the reason it's blocked.

## Custom parameters

1. Team admins can define custom parameters for a heap. A parameter has a name and a list of options.
2. The order of a parameter's options determines their relative priority within that parameter. Each parameter also
   has a weight controlling its influence relative to the heap's other factors (Priority §1).
3. A custom parameter can be linked to one or more tags. When a user adds that tag to an item, the app suggests the
   linked parameters.
4. Effort and impact apply to every item. They share one weight: they count together as a single value-per-effort
   factor, so a small high-impact item ranks above a large item with the same impact. The exact combination is part
   of the formula.

## Priority

1. Each item has a combined priority score based on need, age, deadline proximity, effort and impact, and custom
   parameters. Each factor has a weight, set per heap by its admins, and the weights apply to everyone in the team.
   The weights are the heap's whole ranking policy; there is no separate policy setting.
2. **Age** counts from the item's creation, including any time spent in progress or blocked. Items are not expected
   to stay in those states long. A reopened item keeps its original creation time. How fast an item ages depends on
   its need: an idea ages slowly, so old ideas don't climb past fresh work someone is waiting for, and a critical
   item ages fastest.
   - **Need** also counts on its own: the more an item is needed, the higher it ranks. A critical item pulls about as
     hard as an exact deadline that has just passed; an idea adds nothing.
3. **Deadline proximity** rises as the due date approaches. Past an exact deadline the score jumps, and keeps
   climbing the longer the item is overdue. A loose due time never becomes overdue (Items §3): past its date, the
   score keeps rising, slowly.
4. **Unset values** count as the lowest-priority option (Items §4).
5. **Blockers** take on the priority of the most important item they block, with a small boost, because finishing
   both takes more effort than finishing one. This carries through chains: if A blocks B and B blocks C, A can
   inherit C's priority. Blocking cycles are not allowed; the app refuses a block that would create one.
6. The score is calculated live when someone views the list. Once a list has been sorted, it doesn't reorder on its
   own during that session. When the user edits an item, that item moves to its new position right away, and the
   list may re-sort as a whole at that point.
7. **Ties** go to the older item: earlier creation comes first.
8. Only the item's position is shown, never the score itself.
9. In the list view, each item shows the biggest contributors to its position. This is how the app answers "why is
   this here?" and "why isn't this higher?"; there is no separate item-by-item comparison.
10. The formula and its numbers live in the code, versioned ([decisions.md](decisions.md), decision 4). The factors
    above are the commitment; the numbers aren't.

## Views

### List items

The main view and the answer to "what should I work on?"

1. The app opens on this view, with items ordered by combined priority, highest first.
2. In-progress items stay pinned at the top in a compact form so they aren't forgotten. Blocked items are hidden by
   default. All other open items follow in priority order. Closed items appear only in the Closed items view.
3. The user can configure which non-closed states are pinned, shown, or hidden. Shown blocked items can be mixed in
   with the other items in the current sort order, or listed as a separate group sorted by priority.
4. Each item shows its biggest score contributors (Priority §9).
5. The user can instead sort by any single parameter that has an order (for example deadline, effort, impact, or a
   custom parameter).
6. The user can search items by text and filter by state, tag, and parameter values. Filtering preserves the relative
   priority order of matching items.
7. The user can change an item's state directly in the list. Everything else is edited in the item.
8. The user can open any item in the edit view, and add a new item with a shortcut.

### Closed items

1. Shows done and won't-do items, sorted by when they were closed.
2. An item closed less than a week ago can be reopened from here.

### Add item

The fastest path in the app.

1. The user enters a title, picks how much the item is needed, and optionally adds a description, effort, impact, due
   time and tags. Everything except the title and the need is optional.
2. Typing a tag that doesn't exist yet creates it.
3. Adding a tag brings up the custom parameters linked to it.
4. Saving takes the user back to where they were, ready to add another item.

### Edit item

1. The user can change any field of an existing item, add or remove tags and parameters, and move it between states.
2. When setting an item to blocked, the user can pick the blocking item.

### Manage tags

A supporting view.

1. Any team member can create and rename tags and edit their descriptions. Tags can't be deleted, but a tag can be
   taken off any item.

### Manage parameters

A supporting view, for team admins only.

1. An admin can create and rename custom parameters, edit and reorder their options, and link them to tags.
2. Parameters and options are archived, never deleted (Nothing is deleted §5). Archiving an option requires a
   replacement that non-terminal items switch to. A parameter can be archived only when no non-terminal item uses it.

## Nothing is deleted

1. Items, tags and heaps can't be deleted.
2. An item that won't be done is closed with the **won't do** state instead.
3. Tags can be renamed and their descriptions edited, but not deleted. A tag can be taken off an item, which is
   recorded in the audit log like any other item edit.
4. Heaps are archived instead of removed.
5. Custom parameters and their options are archived instead of removed:
   - Archiving an option requires the admin to pick a replacement option. Items in a non-terminal state (open, in
     progress, blocked) switch to the replacement. Items in a terminal state (done, won't do) keep the archived
     option.
   - A whole parameter can be archived only when no non-terminal item uses it. Terminal items keep their archived
     values.

## Roles and permissions

| Action | Member | Team admin |
| --- | :---: | :---: |
| Register and create teams | ✓ | ✓ |
| List own teams and heaps | ✓ | ✓ |
| Leave a team | ✓ | ✓ (not the last admin) |
| Add and edit items | ✓ | ✓ |
| Add and edit tags | ✓ | ✓ |
| Rename the team | | ✓ |
| Add users to the team and remove them | | ✓ |
| Make a member an admin, demote an admin | | ✓ |
| Create, rename, archive and unarchive heaps | | ✓ |
| Configure a heap's ranking: its weights, parameters and options | | ✓ |
| Archive parameters and options | | ✓ |

## Audit log

1. Every action by a user or an admin is recorded in an audit log.
2. Each entry shows who made the change and what changed.
3. Entries can't be edited or deleted, and they are kept forever.
4. Users see audit entries on the page where the action happens, for every kind of action they are allowed to do. For
   example, every member sees item and tag changes on the heap page, and only admins see team membership changes and
   heap configuration changes.
