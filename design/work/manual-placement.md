# Placing an item by hand: design

A person drags an item above or below another one in a heap's list, or does the same from the keyboard, the CLI or
the API. The item lands there and stays there after a reload, and none of its fields change. The ranking stays
computed: the move is a multiplier stored on that one item and applied to its score.

## Why

An item's place comes only from its fields (need, due, effort, impact, tags, parameters), its age and the heap's
weights (product.md, Priority). To move an item today, a person has to change a field that isn't wrong: call an
expected item critical, or give it a due date it doesn't have. The fields then lie, and the next person to read them
is misled. Placing by hand lets the order be fixed without the fields.

## What it promises

1. In a heap's own list, sorted by priority, a person who may edit items can put an item directly above or below
   another one. It lands there at once and is still there after a reload.
2. None of the item's fields change: not its need, due, sizes, tags or parameters, and not its age.
3. A placement counts for everyone in the heap. It is part of the heap's one ranking (product.md, Priority), so
   teammates see the same order, and agents pick work by it.
4. A placement lasts until the item is placed again or the placement is reset. It doesn't fade with time, and
   editing a field doesn't clear it.
5. The row says the item was moved, and the item says who moved it and when, with a way to reset it.
6. A move from the list can be undone.
7. The CLI and the API can place and reset, as the web app does.

## Out of this design

- No placing across heaps, or in All items, where each person's heap priority sets the order between heaps.
- No placing in a list sorted by anything but priority, in the pinned group, among reminders to come, or in select
  mode.
- No exact, permanent order. The item can drift a little from where it was put, as other items age or near their
  deadlines (below). Holding it exactly in place would need a stored constraint ("just below X"), which is what was
  turned down (Choices).
- No personal order: a placement is the heap's, not one person's.

## How it works

### The multiplier

An item that has been placed holds a **placement**: a positive number its score is multiplied by. An item that has
never been placed, or was reset, holds none, and ranks exactly as it does today.

1. Ranking takes the item's placement as one more input (`ItemForRanking`). The score stays a deterministic
   function of item data and time (decisions.md §1): the placement is item data, so a session's frozen `as_of`
   keeps the order still, and a placed item moves at once, as an edited one does.
2. Where it applies: the item's score before placement is its own score, or the boosted score it inherits from what
   it blocks, whichever is greater, as now. The placement multiplies that. Then the blocker rule holds again: a
   blocker's score is never below the boosted score of what it blocks. So a blocker can be put lower only as far
   as just above the items that wait for it, and an item placed above its own blocker pulls the blocker up with it,
   the way blocking already works (product.md, Priority §5).
3. An item whose score before placement is 0 (a new idea in a heap with low weights) can't be scaled up from 0. For
   a placed item, a score below a small floor counts as the floor, so the multiplier always has something to
   scale. Items that aren't placed never see the floor.
4. The multiplier is kept within a range (1/1000 to 1000, to be tuned in the server's piece). A move that would need
   more lands as far as the range allows, and the response says where it landed.

The formula's version changes (`FormulaVersion`), since the same items can now rank differently. With no item
placed, every score is the one the current version gives, so the change moves nothing until someone places an item.

### Working out the number

A placement names one other item, the anchor, and a side: above it or below it. The server works the number out at
the `as_of` the client's list was ranked at, so the item lands where the person saw it go.

1. The server ranks the heap at that `as_of`, all of its ranked items, not only those the person's filters show,
   leaving out the item being placed.
2. **Above B:** the target score is halfway between B's score and the score of the item directly above B. With
   nothing above B, it is a tenth above B's.
3. **Below A:** halfway between A's score and the score of the item directly below A. With nothing below A, a tenth
   below A's.
4. The placement is the target divided by the item's score before placement (above, with the floor).
5. Placing an item again replaces its placement: the new number is worked out from the score before placement, not
   multiplied on top of the old one.

Halfway in the full ranking, not between the two rows the person dropped it between, matters when filters hide
items: dropped between two visible rows, the web app names the row below the drop as the anchor ("above B"), or the
row above when it was dropped last ("below A"), and the item lands in the gap the person saw, next to that row.

When two neighbours tie, there is no number between them. The item then ties with them too and the usual tie rule
places it among them (older first). This is rare, since ties need equal scores, and is accepted.

### Drift

The neighbours keep ageing, and their deadlines come nearer, at their own rates; the placed item's score grows by
its own factors, times its placement. So a day or a week later the item may sit a place or two from where it was
put. This was accepted when the mechanism was picked: the ranking stays computed, and a placement that has drifted
too far is fixed by placing it again.

### What changes it

- **Placing again** replaces it.
- **Reset** removes it, from the item, the CLI or the API. The item goes back to where its fields put it.
- **Moving the item to another heap** removes it, since its number only means something against that heap's weights
  and items.
- **Nothing else.** Editing a field, changing state, closing and reopening, a note becoming a task, a change to the
  heap's weights or tag order: the placement stays. After a field edit the item moves by that edit, from where the
  placement held it.

### Who can place, and what is recorded

- Placing and resetting need the right to edit the item (`EditItems`), in a heap that isn't archived, like any edit.
- A placement is not an edit of a field: it takes no item version and so never conflicts with an edit made meanwhile;
  the last placement wins. (Assumption, checked in the server's piece: if the web app's cache relies on the
  version changing, it bumps it without checking it.)
- The item stores the placement, who placed it and when. The schema is in the migration.
- Each placement and each reset is an audit entry in the heap's team, and shows in the item's history: "Ana moved
  it up in the list", "Ana reset its place in the list".
- An agent may place and reset too, with the same right. Agents pick work by the score, so they follow every
  placement whoever made it.

### The API

- `POST /items/{key}/place` with `above` or `below` (an item key in the same heap) and the list's `as_of`. It answers
  with the item's new position in the heap's ranked list. Refused when the anchor is the item itself, is in another
  heap, is closed, or isn't in the ranked group.
- `POST /items/{key}/place` with `reset` removes the placement.
- An item's response says whether it is placed, by whom and when. The number itself is never returned, as scores
  aren't (product.md, Priority §8).
- A placed item's contributors start with **Moved up** or **Moved down** (factor `Placed`), whatever its share, so
  the row always says it. Up and down are against where its fields would put it, the placement above or below 1.
  One more contributor follows by the usual rules. Its Held back by is empty, as a blocker's is, since changing its
  fields wouldn't move it where the person can see.

Endpoints and fields are settled in `openapi.json` once built; this names only what they must do.

### The CLI

`upheap item move` already moves an item to another heap, so placing has its own verb:

- `upheap item place <ITEM> --above <ITEM>` and `--below <ITEM>` put it there, at the current time (the CLI has no
  frozen list), and print where it landed ("Placed: now #3 in Planning").
- `upheap item place <ITEM> --reset` removes the placement.
- `upheap item show` says "Moved up in the list by Ana, 3 days ago" when the item is placed.

### The web app

What follows is the outline the mockup works out; nothing here is built before the mockup is approved.

- **Mouse and trackpad:** a drag handle shows at the row's end while it is hovered or focused, in the heap's list
  sorted by priority, in the ranked group. Dragging it shows a line where the row will land, and the other rows make
  room. The rest of the row still opens the item on a click.
- **Keyboard:** Alt+↑ and Alt+↓ move the focused row one place up or down, each press a placement against the row
  it passes. The list's own ↑ and ↓ keep moving the focus.
- **Phone and tablet:** a long press on a row lifts it, and then it drags up or down. A quick drag left or right is
  still the swipe (ux.md, Gestures): only a long press first starts a placement, so the two never meet.
- **The row** shows "Moved up" or "Moved down" as its first contributor.
- **Undo:** after a placement from the list, the toast "Moved “Rotate the certs” up." offers Undo. Undo puts the
  previous placement back: a reset when there was none, and otherwise a placement against the item's old neighbour,
  at the same `as_of`.
- **The item** shows "Moved up in the list by Ana, 3 days ago" with **Reset**. Placing it again is the drag.

## Checking it

- Ranking tests: a placed item lands between the anchor and its neighbour; with no placement every order is the
  previous formula's; a blocker can't be placed below what it blocks and follows an item placed above it; the floor
  lifts a zero score; the range clamps; placing again replaces rather than multiplies.
- Service tests: placing at an `as_of` puts the item at that position in the list ranked at the same `as_of`; reset
  restores the fields' position; moving to another heap drops it; a field edit keeps it; audit entries are written.
- API and CLI tests for both commands and their refusals; e2e tests for drag, the keyboard, undo and reset.

## Choices

- **A multiplier on the item, not an offset added to the score.** A multiplier scales with the item's own score, so
  as the whole heap's scores grow with age the item keeps roughly its place, and it reads as "this item matters this
  much more or less than its fields say". An offset would be a fixed amount that shrinks in meaning as scores grow.
  It needs the floor for zero scores, which an offset wouldn't.
- **Not a shifted age.** Moving the time the item ages from reuses a factor, but an idea ages slowly, so a big move
  needs a huge shift, and age labels would lie ("Waiting 3 years").
- **Not a stored constraint ("ranks just below X").** It gives an exact order, but chains of constraints, cycles and
  closing X all need rules, and the ranking stops being one score per item that merges across heaps.
- **It lasts until reset, and doesn't fade.** A fading placement would quietly undo itself, and the person would have
  to keep moving the same item. It stays, and Reset is one click.
- **A field edit keeps it.** The placement says how the item stands against its fields; an edit changes the fields,
  and the item moves by that edit from where it was held, which is what the person sees as "the edit moved it".
