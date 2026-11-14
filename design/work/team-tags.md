# Team tags: design

Tags made once for a team and usable in every heap of it, beside each heap's own. In progress, by the steps of the
maintainer's design method. Step agreed: none. Steps 1 and 2 are drafted, for review together.

## Constraints

1. The product as it stands is given: [product.md](../product.md), [ux.md](../ux.md),
   [architecture.md](../architecture.md) and [decisions.md](../decisions.md), except the parts this design sets out
   to change (Heaps §1, Manage tags, Add item §2, the roles table).
2. Nothing is deleted (product.md, Nothing is deleted): a tag is never deleted, only taken off items.
3. A heap belongs to exactly one team, and an item moves only between heaps of its team (Teams §5, Edit item §1).
4. The stack in use: the Rust server on SQLite, MySQL or Postgres, the web app and the `upheap` CLI over one API.
5. Done when a tag made once on the team can be put on items in two of its heaps, and renaming it changes it in
   both (the owner, in yacb1).

Assumption: no deadline and nothing else fixed. The owner wasn't asked in this run; the review can add to this list.

## 1. Value

For a team that runs several heaps, one set of tags that means the same thing in all of them, so the same word
isn't made again in each heap and doesn't drift apart in name and meaning. It extends the pitch's "tags, used to
group similar items" from one heap to the team.

It answers, in a member's words:

- "Do we already have a tag for this?" (The team's tags are offered in every heap.)
- "Is `security` here the same `security` as in our other project?" (A team tag is one tag, wherever it's used.)
- "We renamed it; is it renamed everywhere?" (Yes, once.)

What it is not:

- **Not a view across heaps.** Filtering every heap of the team by one tag is another question, not this one's. Team
  tags make that view more useful; they don't build it.
- **Not a taxonomy.** No tag groups, parents or required tags.
- **Not shared between teams.** A team tag belongs to one team, as a heap does.
- **Not team-level custom parameters.** Parameters stay per heap.

## 2. Golden path

### The path

1. A member opens the team's tags and makes `security`, with a description. It's listed as one of the team's tags.
2. In heap A, they add an item and start typing `sec`. The team tag is offered beside A's own tags; they pick it.
3. In heap B, they put the same tag on another item. B never made a tag of that name.
4. They filter B's list by `security`, then A's. Each shows its item, as with any tag.
5. A teammate renames it `security-review`, once, on the team. Both items, in both heaps, show the new name, and
   the tag's history shows the one rename.
6. Someone moves the item from A to B. It keeps the tag.
7. A already had a heap tag `billing`, and so did B. A member makes `billing` a team tag. The items in A and B that
   carried either keep it, now as the one team tag, and heap C can use it too.

After step 5 the value is there; step 7 is how a team that already drifted gets back to one tag.

### Commitments

Core ones, for product.md, are marked **core**.

Team tags

1. **core** A team has its own tags, beside each heap's own. A team tag can be put on items in any heap of the team.
2. **core** A team tag is one tag: renaming it or editing its description changes it in every heap at once.
3. **core** A heap keeps its own tags, which work as they do today, in that heap only.
4. Every team has team tags, personal teams too: one person with several heaps has the same drift.
5. A team tag is never deleted, like any tag. It can be taken off any item.

Names

6. **core** In a heap, a tag name means one tag: no heap tag can share a name with a team tag of its team, ignoring
   case. Two heaps can still each have a heap tag of the same name, as today.
7. **core** Making a team tag with a name that heap tags of the team already have makes those heap tags into the
   team tag: every item that carried one carries the team tag, and its history is kept. This is also how a heap tag
   is promoted to the team.

Using them in a heap

8. **core** In a heap, a team tag works wherever a heap tag does: picking tags on an item, typing to find one, the
   list's tag filter, saved views, and linking a custom parameter to a tag. A person needn't know which kind a tag
   is to use it, and can see which kind it is.
9. Typing a tag name that doesn't exist yet, on Add item or in the item, makes a heap tag, as today. A heap tag can
   be made a team tag later (§7).

Moving items

10. **core** An item moved to another heap of its team keeps its team tags. Its heap tags carry over by name as
    today: to the target heap's tag of the same name, or they're dropped and the move says so.

History

11. Making, renaming, describing and promoting a team tag is in the team tag's history, which every member of the
    team sees. Putting a team tag on an item or taking it off is in the item's history, as for any tag.

### Actors and roles

| Action | Member | Team admin | Agent |
| --- | :---: | :---: | :---: |
| Make, rename and describe a team tag | ✓ | ✓ | as the person it acts for |
| Make a heap tag a team tag | ✓ | ✓ | as the person it acts for |
| Put a team tag on an item, take it off | ✓ | ✓ | as the person it acts for |
| See a team tag and its history | ✓ | ✓ | as the person it acts for |

Any member manages team tags, as any member manages heap tags today (product.md, roles table). Admin-only would be a
limit with no harm named above it: a rename reaches the other heaps, but it is in the history and is undone by
renaming back. It goes under Open for now at step 4.

### Out

To Ideas, not commitments: archiving tags; a tag filter across heaps; demoting a team tag back to a heap
tag; merging two tags of different names; team-level custom parameters; tag colours.

## 3. Workflows
## 4. Behaviours
## 5. Interface
## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- Promotion (§7) with heap tags of different descriptions: which description the team tag keeps.
- Storage: a tag owned by a team or by a heap; what becomes of a heap tag's row once merged into a team tag.
- Locking: a team tag write touches every heap of the team, where tag writes lock one heap row today.
- The API names tags by ID checked against the heap in the path; a team tag's ID must pass that check in each heap.
- The CLI's `--tag` resolves names against the heap; with §6 a name still resolves to one tag.
- A move today carries tags over by name (`domain::item_move::carry_over`); team tags need no lookup.
- Audit: where a team tag's entries are recorded, since the heap log reads only what is recorded against one heap.

## Ideas

Outside the scope. Not commitments.

- Archiving a tag, team or heap, so it's no longer offered but stays on the items that have it.
- Filtering by a team tag across every heap of the team (a view across heaps).
- Demoting a team tag to a heap tag.
- Merging two tags of different names into one.
- Custom parameters defined once for the team.
