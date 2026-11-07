# Upheap

> See also [offering.md](offering.md).

## Pitch

Upheap is a todo list for people with more work than they can keep track of. You add items as they come up, with a rough size, impact, and due date. The app ranks everything by how long it has waited, how close its deadline is, and how much it matters, and keeps that ranking current as time passes. A heap can be personal or shared by a team.

It answers questions like:

- What should I focus on today?
- What's the most important thing on my plate right now?
- What's next after I finish this?
- Why isn't this item higher on the list?

## Golden path requirements

### Heaps

1. A heap is a collection of items with its own ranking, tags, and custom parameters.
2. Every heap is owned by a team (see [offering.md](offering.md)). A personal heap is a heap in a one-person team. Everyone in a team sees the same items and ranking in its heaps and can add and edit items.
3. A person can belong to several teams and heaps and switch between them.
4. A heap can have named, saved, shareable views defined by filters, such as work ready to pick up, a service or component, recurring toil, or blocked work. These views reference the same underlying items rather than creating separate backlogs.
5. Items have no assignee or owner. A shared heap has one ranking for the whole team, and "my plate" means the heaps I belong to. This is deliberate.

### Items

1. An item is a finishable piece of work with a title and an optional description.
2. An item can have an effort size and an impact, each on the scale S, M, L, XL.
3. An item can have a due time, given either loosely or as an exact deadline date.
   - A loose due time is stored as a date, marked as not firm. It is shown in fuzzy terms such as "in a few days", "next week", "in a few weeks", "next month" or "in several months".
   - A loose due time never becomes overdue. Once its date has passed, it keeps showing the nearest term, for example "in a few days".
   - An exact deadline can become overdue.
   - A loose due time is picked from rough choices ("next week") rather than a date, and reads back with the term it was picked by.
4. Effort, impact, due time and custom parameters can all be left unset. An unset value counts as the lowest-priority option for that parameter, and the interface shows it as unset.
5. An item can have any number of tags, used to group similar items. A tag has a name and an optional description.
6. An item is always in one state: open, in progress, blocked, done, or won't do. Done and won't do are closed (terminal) states. Items are never deleted. Work that won't happen is closed as won't do.
7. A closed item can be reopened within one week of being closed. After that it can't be reopened.
8. A blocked item can point to another item as the reason it's blocked.

### Custom parameters

1. Team admins can define custom parameters for a heap. A parameter has a name and a list of options.
2. The order of a parameter's options determines their relative priority within that parameter. Each parameter also has a heap-level weight controlling its influence relative to other parameters. These weights are the heap's ranking policy; there is no separate policy setting.
3. A custom parameter can be linked to one or more tags. When a user adds that tag to an item, the app suggests the linked parameters.
4. Effort and impact apply to every item. They share one weight: they count together as a single value-per-effort factor, so a small high-impact item ranks above a large item with the same impact. The exact combination is part of the formula.

### Priority

1. Each item has a combined priority score based on age, deadline proximity, effort and impact, and custom parameters. The heap's weights (Custom parameters §2) set their relative influence, and they apply to everyone in the team.
2. **Age** counts from the item's creation, including any time spent in progress or blocked. Items are not expected to stay in those states long. A reopened item keeps its original creation time.
3. **Deadline proximity** rises as the due date approaches. Past an exact deadline, the score keeps climbing, probably with an overdue multiplier (details to be decided with the formula). Loose due times never become overdue (Items §3): past their date, the score keeps rising slowly, without the overdue multiplier.
4. **Unset values** count as the lowest-priority option (Items §4).
5. **Blockers** take on the priority of the most important item they block, possibly with a small extra boost, because finishing both takes more effort. This carries through chains: if A blocks B and B blocks C, A can inherit C's priority. Blocking cycles are not allowed; the app refuses a block that would create one.
6. The score is calculated live when someone views the list. Once a list has been sorted, it doesn't reorder on its own during that session. When the user edits an item, that item moves to its new position right away, and the list may re-sort as a whole at that point.
7. **Ties** go to the older item: earlier creation comes first.
8. Only the item's position is shown, never the score itself.
9. In the list view, each item shows the biggest contributors to its position. This is how the app answers "why is this here?" and "why isn't this higher?"; there is no separate item-by-item comparison.
10. The exact formula is open and will be designed separately.

### Views

**List items.** The main view and the answer to "what should I work on?"

1. The app opens on this view, with items ordered by combined priority, highest first.
2. In-progress items stay pinned at the top in a compact form so they aren't forgotten. Blocked items are hidden by default. All other open items follow in priority order. Closed items appear only in the Closed items view.
3. The user can configure which non-closed states are pinned, shown, or hidden. Shown blocked items can be mixed in with the other items in the current sort order, or listed as a separate group sorted by priority.
4. Each item shows its biggest score contributors (Priority §9).
5. The user can instead sort by any single parameter that has an order (for example deadline, effort, impact, or a custom parameter).
6. The user can search items by text and filter by state, tag, and parameter values. Filtering preserves the relative priority order of matching items.
7. The user can edit an item's title, effort, impact, due time, and state directly in the list.
8. The user can open any item in the edit view, and add a new item with a shortcut.

**Closed items.**

1. Shows done and won't-do items, sorted by when they were closed.
2. An item closed less than a week ago can be reopened from here.

**Add item.** The fastest path in the app.

1. The user enters a title and optionally a description, picks effort, impact, and due time, and adds tags. Everything except the title is optional.
2. Typing a tag that doesn't exist yet creates it.
3. Adding a tag brings up the custom parameters linked to it.
4. Saving takes the user back to where they were, ready to add another item.

**Edit item.**

1. The user can change any field of an existing item, add or remove tags and parameters, and move it between states.
2. When setting an item to blocked, the user can pick the blocking item.

**Manage tags.** A supporting view.

1. Any team member can create and rename tags and edit their descriptions. Tags can't be deleted, but a tag can be taken off any item.

**Manage parameters.** A supporting view, for team admins only.

1. An admin can create and rename custom parameters, edit and reorder their options, and link them to tags.
2. Parameters and options are archived, never deleted. Archiving an option requires a replacement that non-terminal items switch to. A parameter can be archived only when no non-terminal item uses it. See [offering.md](offering.md).
