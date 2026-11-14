# Team tags: design

Tags made once for a team and usable in every heap of it, beside each heap's own. In progress, by the steps of the
maintainer's design method. Step agreed: 2. Step 3 is drafted, for review.

## Constraints

1. The product as it stands is given: [product.md](../product.md), [ux.md](../ux.md),
   [architecture.md](../architecture.md) and [decisions.md](../decisions.md), except the parts this design sets out
   to change (Heaps §1, Manage tags, Add item §2, the roles table).
2. Nothing is deleted (product.md, Nothing is deleted): a tag is never deleted, only taken off items.
3. A heap belongs to exactly one team, and an item moves only between heaps of its team (Teams §5, Edit item §1).
4. The stack in use: the Rust server on SQLite, MySQL or Postgres, the web app and the `upheap` CLI over one API.
5. Done when a tag made once on the team can be put on items in two of its heaps, and renaming it changes it in
   both (the owner).

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

Members and team admins do the same here (the roles table), so the flows are a member's. An agent does what its
person does, through the CLI or the API, and has the flows marked for it. A personal team's one person is its only
member and has every flow.

### A member

**A. Tag an item** (loop, many times a day; it changes from today only in what is offered)

1. In a heap, on Add item or in an item, they start typing a tag name.
2. The heap's tags and the team's tags are offered together, each showing which kind it is.
3. They pick one. The item carries it, and the item's history says so.
4. A name that matches no tag makes a heap tag, as today (§9).

**A′. Recovery: the wrong tag** (after A)

1. They take the tag off the item and put the right one on, as today. The item's history shows both.

**B. Narrow a heap's list by a team tag** (loop, daily)

1. In a heap's list, they pick a team tag in the tag filter, or click its chip on a row.
2. The list shows that heap's items that carry it, as with a heap tag.
3. A saved view that holds the filter keeps the team tag, and still finds it after a rename.

**C. Make a team tag** (setup, now and then, most often when the team starts using team tags)

1. They open the team's tags and give a name and a description.
2. If no heap tag of the team has that name, the team tag is made.
3. If heap tags have it, they're shown first which heaps and how many items carry one, and that those become the
   team tag (§7). They confirm, or step back.
4. The tag is listed among the team's tags and offered in every heap of the team.

**D. Promote a heap tag** (setup, a burst when a team that already drifted adopts team tags, rare after)

1. From a heap's tags, they choose to make one a team tag.
2. As C from step 3: they see which other heaps have a tag of that name and how many items, and confirm.
3. The items that carried any of those heap tags carry the team tag, and their history is kept.

**D′. Recovery: a name that drifted apart** (setup, before D, when heaps spell the same tag differently)

1. In a heap whose tag says `bugs` where the others say `bug`, they rename that heap tag to `bug`, as today. Two
   heaps may each have a heap tag of one name (§6), so this is allowed.
2. Then they promote `bug` (D), and every heap's `bug` becomes the team tag.

**D″. Recovery: a promotion made by mistake** (after C or D)

1. The confirm in C step 3 is the guard: it shows what will merge before anything does.
2. After it, there is no undo: demoting is out (Out). They rename the team tag if the name was the mistake, or take
   it off the items it shouldn't be on, one by one.

**E. Rename or describe a team tag** (sometimes)

1. From the team's tags, or from the tag where a heap shows it, they rename it or edit its description.
2. They're shown that it changes in every heap of the team.
3. Every heap shows the new name at once: rows, the filter, saved views, linked parameters.

**E′. Recovery: a rename that shouldn't have happened** (after E)

1. They see it in the tag's history, who and when, and rename it back. The history keeps both.

**F. Move an item to another heap** (sometimes; an existing flow, changed in what it keeps)

1. They move the item, as today.
2. Its team tags stay on it. Its heap tags carry over by name, or are dropped and the move says which (§10).

**G. Look at a team tag's history** (sometimes, often after E′ or on a surprise)

1. From the tag, they open its history: made, renamed, described, promoted, each with who and when.

### An agent, for its person

**H. Tag an item from the CLI or the API** (loop, many times a day)

1. It adds or edits an item in a heap and names tags, as today.
2. A name resolves to the heap's tag or the team's tag of that name, whichever there is: one name is one tag in a
   heap (§6). A name with no tag is refused as today, with the hint that offers to make it.
3. Listing a heap's tags shows the team's tags beside the heap's, with which kind each is.

**I. Manage team tags from the CLI** (sometimes): C, D, E and G as the person does them, with the same "what this
reaches" shown before a promotion, and confirmed before it happens.

### User stories

From the steps above that ask something of the product.

1. A member typing a tag in a heap is offered the heap's tags and the team's together, and sees which kind each is.
   (A.2)
2. A member typing a name no tag has gets a heap tag, as today. (A.4)
3. A member can filter a heap's list by a team tag, from the filter or a row's chip, and save it in a view that
   survives a rename. (B)
4. A member can make a team tag with a name and a description from the team's tags. (C.1)
5. Before a team tag takes over heap tags of its name, a member sees which heaps and how many items, and can step
   back. (C.3, D.2)
6. A member can make a heap tag a team tag from that heap's tags. (D.1)
7. A member can rename a team tag or edit its description once and see it change in every heap, and is told that
   it will. (E)
8. A member can read a team tag's history: what changed, who, when. (G, E′)
9. A member moving an item keeps its team tags without a choice to make. (F)
10. An agent names a team tag in a heap the way it names a heap tag, and sees team tags when it lists a heap's
    tags. (H)
11. An agent can make, promote, rename and describe team tags for its person. (I)

### Not core

Left out on purpose; each is under Out or Ideas.

- Undoing a promotion (demoting a team tag): recovery is renaming, or taking it off items (D″).
- Merging two tags of different names in one heap: D′ covers names that differ across heaps.
- Suggesting which heap tags look like the same tag across heaps: the member spots them.
- Filtering every heap of the team by a team tag at once.
- A team admin approving team tags, or managing them alone.
- Archiving a team tag that is no longer wanted.

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
- Renaming a team tag to a name some heap tags have (E): merge as in §7, or refuse. Renaming a heap tag to a team
  tag's name: refused by §6, as a clash in one heap is today.
- A saved view or a linked parameter holding a heap tag that is promoted (D): it should hold the team tag after.

## Ideas

Outside the scope. Not commitments.

- Archiving a tag, team or heap, so it's no longer offered but stays on the items that have it.
- Filtering by a team tag across every heap of the team (a view across heaps).
- Demoting a team tag to a heap tag.
- Merging two tags of different names into one.
- Custom parameters defined once for the team.
