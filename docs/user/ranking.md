# How the ranking works

What puts an item where it is in the list, and what to change when you disagree with it. The list never shows a
score, only the position and the biggest reasons for it, so this page is what the screen can't tell you.

## What counts

1. **Need.** How much the item is needed: what happens if it never gets done. From least to most: an idea (nothing
   is lost), nice to have (only a gain is missed), expected (someone is counting on it), critical (skipping it does
   real damage). The more an item is needed, the higher it ranks. A critical item pulls about as hard as an exact
   deadline that has just passed, and an idea adds nothing.
2. **Age.** An item climbs as it waits, counted from when it was added, time in progress or blocked included. How
   fast depends on its need: an idea ages slowly, so old ideas don't climb past fresh work someone waits for, and
   a critical item ages fastest. A reminder's age counts from its date.
3. **Due.** The item rises as its due time comes near. Past an exact deadline it jumps, and keeps climbing the
   longer it is overdue. A loose due time ("next week") never becomes overdue: past its date the item keeps rising,
   slowly.
4. **Value for effort.** Impact and effort count together, so a small item with a high impact ranks above a large
   one with the same impact.
5. **Custom parameters.** A heap's admins can add their own, such as "Customer", with options in an order and a
   weight.
6. **Tag order.** A heap's admins can put its tags in an order. An item's highest-ranked tag lifts it or pushes it
   down. This counts for nothing until its weight is turned up from 0.

Effort, impact, a due time and a parameter left unset count as the lowest option. Need is always set.

## What else moves an item

1. **Blocking.** An item that blocks another takes on the priority of the most important item it blocks, with a
   small boost, through chains too. The blocker's reasons say "Blocks …".
2. **Ties** go to the item that blocks the longer chain, then to the older item.
3. **Placing by hand.** You can put an item right above or below another one without changing its fields:
   `upheap item place k3x9a --above p2m7q`, or `--reset` to undo it. It holds for everyone in the team and for
   agents, until the item is placed again or reset. The ranking stays computed, so a placed item can drift a place
   or two over days as others age or near their deadlines.

## When you disagree with it

1. **One item is in the wrong place:** fix the field that is wrong, usually its need or its due time. Place it by
   hand when the fields are right and you still want it elsewhere.
2. **A whole kind of item is in the wrong place:** a heap's admins change the weights, which are the heap's whole
   ranking policy and apply to everyone in the team. `upheap heap show` prints them and `upheap heap weights` sets
   them.
3. **Several heaps compare wrongly:** each person gives their own heaps a priority from 1 to 10, theirs alone,
   which orders the heaps against each other wherever several are shown together: `upheap heap priority 3 work`.

## What stays still

The ranking is computed when you look. Once a list is on screen it doesn't reorder by itself during that session;
an item you edit moves to its new place right away.

A sort by a single field (due, need, effort, impact or a parameter) and the filters are views of the same list.
Filtering keeps the items in their ranked order.
