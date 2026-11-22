# Placing an item by hand

A person drags an item above or below another one in a heap's list (or uses the keyboard, the CLI or the API). It
lands there, stays there after a reload, and none of its fields change. The ranking stays computed: the move is a
multiplier stored on that one item and applied to its score.

Why: today the only way to move an item is to change a field that isn't wrong, such as calling an expected item
critical. Then the fields lie to the next reader.

## Rules

1. **Where:** in one heap's list, sorted by priority, in the ranked group, by anyone who may edit items in a heap
   that isn't archived. Not in All items, not in another sort, not among pinned items or reminders to come.
2. **For whom:** everyone. The multiplier is part of the heap's one ranking, so teammates and agents follow it.
3. **How long:** until the item is placed again or reset. It doesn't fade. Field edits, state changes and weight
   changes keep it; moving the item to another heap drops it.
4. **Drift is accepted:** the item can slip a place or two over days as others age or near their deadlines. Placing
   it again fixes that.

## How the number is set

- The item stores `placement`, a positive multiplier, or nothing. With nothing, it ranks exactly as today.
- Ranking multiplies the item's score (its own, or what it inherits from what it blocks, whichever is greater) by
  the placement, then applies the blocker rule again: a blocker never ranks below what it blocks. A score of 0 counts
  as a small floor for a placed item, so there is something to scale.
- On a drop the client sends the anchor, the side (above or below) and the list's `as_of`. The server ranks the heap
  at that `as_of` without the item, takes the anchor's score and its neighbour's on that side, and sets the
  placement so the item lands halfway between them (a tenth past the anchor when there is no neighbour). Placing
  again replaces the number; it doesn't multiply on top.
- The formula's version goes up. With no item placed, every order is the same as before.

## Surfaces

- **API:** `POST /items/{item}/place` with `anchor`, `side` (above or below) and an optional `as_of`; `DELETE
  /items/{item}/place` resets. The anchor must be an item that ranks (open, in progress or blocked, not a reminder to
  come) in the same heap, and is refused otherwise, as is the item itself. The API doesn't know the client's list
  view, so the web offers drag only in the priority list's ranked group. Runs can't place. The item's response says
  whether it is placed and in which direction; the number is never shown, as scores aren't.
- **Contributors:** a placed item's first contributor is **Moved up** or **Moved down** (against where its fields
  would put it).
- **History:** each placement and reset is an audit entry: "Ana moved it up in the list", "Ana reset its place".
- **CLI:** `upheap item place <ITEM> --above|--below <ITEM>` and `upheap item place <ITEM> --reset`, ranked at the
  current time, printing where it landed. `item move` already means moving to another heap.
- **Web, wide screens:** a drag handle at the row's end on hover or focus; a line shows where it will land. Alt+↑ and
  Alt+↓ move the focused row one place.
- **Web, phone:** long press lifts the row, then drag up or down. A quick left or right swipe stays as it is.
- **Undo:** the toast after a move offers Undo, which puts the previous placement back (a reset when there was none).
- **The item:** shows "Moved up in the list" with **Reset**.

## Not doing

- A personal order, an exact pinned position, or placing across heaps.
- Storing who placed it on the item: the history shows that.
