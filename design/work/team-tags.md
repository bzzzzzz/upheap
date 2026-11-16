# Team tags: design

Tags made once for a team and usable in every heap of it, beside each heap's own. In progress, by the steps of the
maintainer's design method. Step agreed: 5. Step 6 is drafted, for review.

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

### Concepts

- **Tag.** A name, unique within its owner ignoring case, and an optional description. It has an owner, which never
  changes: a heap (a **heap tag**) or a team (a **team tag**). A tag is never deleted (Constraint 2).
- **The tags of a heap.** The heap's own tags and its team's tags, together. Everything in a heap that names a tag
  (an item, the filter, a saved view, a parameter's link) names one of these.
- **A tag on an item.** An item carries a tag of its heap's tags, at most once. Putting it on and taking it off are
  in the item's history, as today.
- **States of a tag.**
  - A team tag is **in use**, its only state.
  - A heap tag is **in use** or **merged into** a team tag. The one move is in use → merged, by a merge (rule 7).
    It never moves back (demoting is out). A merged heap tag isn't one of the heap's tags any more: it isn't
    offered, filtered on, or put on items. It stays only so the history from before the merge reads as it was.
- **A merge.** One act that makes every heap tag of the team with a given name, in every heap, merged into one team
  tag. It is shown before it happens and confirmed (rule 9).
- **The team tag's history.** Its own entries: made, renamed, described, and each merge, naming the heaps and how
  many items.

### Rules and invariants

Names

1. In a heap, one name is one tag: no heap tag in use has the name of a team tag of its team, ignoring case. This
   is commitment 6 as an invariant; every rule below keeps it.
2. Two heaps of a team may each have a heap tag in use of the same name, as today.
3. Making or renaming a heap tag to the name of a team tag of its team is refused, as a clash between two heap tags
   in one heap is today. The refusal names the team tag, so the member can use it instead.
4. A tag's name and description are its own: wherever a team tag shows, in any heap, it shows its current name and
   description.

Making, promoting and renaming a team tag

5. Making a team tag with a name no heap tag in use of the team has makes it, and nothing else changes.
6. Promoting a heap tag is making a team tag with its name (rule 7).
7. **A team tag that takes a name** heap tags in use of the team have, by being made or promoted (commitment 7) or
   by being renamed (flow E), merges them all into it, in every heap of the team, archived heaps included:
   - every item that carries one of them, open or closed, carries the team tag instead, and its history says so
     and keeps the entries that named the heap tag;
   - every saved view whose filter names one names the team tag;
   - every custom parameter linked to one is linked to the team tag;
   - in each heap whose tag order ranks one, the team tag takes its place in that order (the highest, if the order
     ranks several of them).
8. The team tag a merge makes keeps the description given when it is made. A promotion carries the promoted heap
   tag's description. The other heap tags' descriptions stay in their history and the merge's entry.
9. A write that would merge is first shown what it reaches, the heaps and how many items in each, and happens only
   when the caller confirms that reach. If the reach has changed between the two, including from nothing to
   something for a make or a rename that merged nothing when it started, the write is refused and shows the new
   reach. This holds for the web app, the CLI and the API alike (flows C.3, D.2, I).
10. A rename that takes no name heap tags have changes only the team tag's name, in every heap at once
    (commitment 2). It changes nothing on items, views or parameters, which name the tag, not its name.
11. A name a team tag gave up by a rename is free. A heap may then make a heap tag of it; renaming back merges that
    tag in, by rule 7 (flow E′).

Using a team tag in a heap

12. A heap's tag filter, its saved views, the tag picker and a parameter's links take any of the heap's tags, and
    treat a team tag and a heap tag alike (commitment 8). A heap's parameter linked to a team tag is suggested when
    that tag is put on an item in that heap only, since the parameter is the heap's. A heap's tag order (product.md,
    Priority §10) ranks any of the heap's tags too: each heap orders a team tag on its own, since the ranking
    policy is the heap's, and a team tag joins each heap's order unranked until that heap's admin places it.
13. Where tags are listed or offered, each shows which kind it is.
14. Typing a name that matches none of the heap's tags makes a heap tag (commitment 9). In the CLI and the API a name
    with no tag is refused, as today, with the hint to make it (flow H).
15. An archived heap is read-only, but a team tag's rename shows in it, and a merge reaches it (rule 7): both are
    changes to the team's tag, not edits in the heap.

Moving an item

16. An item moved to another heap of its team keeps every team tag it carries (commitment 10).
17. Its heap tags carry over by name to the target heap's heap tag in use of that name, or are dropped, and the move
    says which. By rule 1 no heap tag shares a name with a team tag, so a heap tag never lands on a team tag.

Who may do what

18. Any member of the team may make, promote, rename and describe a team tag, and put one on or take it off an item
    in any heap of the team (the roles table).
19. An agent acts as its person, except in the agent loop, where it touches no heap the person didn't give it
    (product.md, The agent loop §4). There it may make a team tag that merges nothing, and put team tags on items in
    its heaps. A rename or a description reaches every heap of the team, and a merge reaches the heaps it names: it
    may do these only when every heap they reach is one of its heaps, and is refused otherwise.

History and what is kept

20. The team tag's history is shown to every member of the team, who all see every heap (Teams §5). It is kept
    forever, like every audit entry.
21. An item's history names each tag by its current name, as today. A merged heap tag keeps the name it had when it
    merged, so entries from before a merge name the heap tag and the merge entry names both.

### Open for now

Left open in the first version, with what in use would show it needs closing.

- **Admin-only team tags.** Any member manages them (the owner's call at step 2). Close it if renames or merges
  that a team didn't want start showing up in team tags' histories.
- **A size limit on a merge.** A merge may reach any number of heaps and items, with the confirm as the only guard.
  Close it if mistaken merges show up that the confirm didn't stop.
- **Many team tags.** No limit on how many a team makes. Close it if the offered list grows too long to pick from.

### Scenarios

Each pulls two rules against each other. Each is a test case for later.

1. **Rename back after a heap took the name.** The team tag `security` is renamed `security-review`. Heap C makes a
   heap tag `security` on two items (rule 11). Someone renames the team tag back to `security`: they're shown that
   C's tag and its two items merge (rule 9), confirm, and C's items carry the team tag (rule 7). Flow E′ still works,
   and rule 1 holds.
2. **Heap tag renamed into a team tag's name.** The team has `bug`; heap A still has `bugs`. Renaming A's tag to
   `bug` is refused, naming the team tag (rule 3). Merging two tags of different names is out, so the member puts
   `bug` on A's items and takes `bugs` off. Flow D′ applies only before the team tag exists. Kept as is: the Idea
   to merge tags of different names would cover it.
3. **A merge reaches an archived heap.** Heaps A and an archived heap Z both have `billing`. A member promotes A's
   `billing`. Z's items carry the team tag too (rules 7 and 15). Unarchived later, Z has no clash.
4. **A closed item and a moved one.** An item in A, done, carries A's `billing` when it merges: it carries the team
   tag (rule 7). Another item carrying the team tag moves from A to B and keeps it (rule 16), and its heap tag
   `infra` goes to B's `infra`, or is dropped and the move says so (rule 17).
5. **A saved view and a parameter across a promotion.** Heap A's saved view filters on A's `billing`, and A's
   parameter "Billing area" is linked to it. After the promotion the view filters on the team tag and finds the
   same items, and the parameter is suggested when the team tag goes on an item in A, not in B (rules 7, 12).
6. **The reach changed under the confirm.** A member is shown that promoting `billing` reaches heaps A and B. Before
   they confirm, someone makes `billing` in heap C. The confirm is refused and shows A, B and C (rule 9).
7. **An agent in the loop renames.** An agent given heap A alone is told to rename the team tag `ops`, used in A and
   B. It's refused (rule 19); it asks its person instead. Putting `ops` on an item in A works.
8. **Two at once.** One member makes the team tag `docs` while another makes a heap tag `docs` in heap B. If the
   heap tag comes second, it is refused (rule 3). If the team tag comes second, its confirm showed no reach, so it
   is refused and shows B's tag (rule 9); confirmed again, it merges it. Rule 1 holds either way.
9. **A personal team.** One person with heaps Home and Work makes the team tag `errand` and uses it in both
   (commitment 4). Nothing changes from a shared team except that one person is every member.

### Choices

- **A team tag renamed into a name heap tags have merges them** (rule 7), rather than being refused. One rule for
  every way a team tag gets a name, and renaming back always works (scenario 1). Refusing is simpler, but strands
  E′ once a heap reuses the old name.
- **A heap tag renamed into a team tag's name is refused** (rule 3), rather than merged. Merging it would merge two
  tags of different names, which is out.
- **A merge reaches archived heaps and closed items** (rules 7, 15), rather than skipping them. Skipping them breaks
  rule 1 in an archived heap the day it's unarchived, and leaves closed items on a tag nobody can pick.
- **A merged heap tag stays, as merged,** rather than going. Nothing is deleted, and old history names it.
- **The confirm states its reach and is refused if the reach changed** (rule 9), rather than a plain yes. A merge
  can't be undone (flow D″), so what was confirmed must be what happens. **Decision entry:** no; it is behaviour
  one change can undo.
- **An agent in the loop may rename or merge only within its heaps** (rule 19), rather than as its person. The agent
  loop's promise that it touches no other heap is above this design.
- **Team tags as a team-owned resource** (concept Tag): the first resource a team owns beside its heaps and members.
  **Decision entry:** yes, at the end of the design; it changes "every resource resolves to one team through its
  heap" and is costly to reverse.

### Unproven mechanisms

None. Each rule uses what the product already does: tag names unique ignoring case, a move carrying tags by name,
a history per entity, and refusals with a hint.

## 5. Interface

What each surface holds, in what order, and its states. The look, the final words and the components come with the
mockup, before the web app is built. Most of it is today's tag surfaces with team tags beside heap tags; the new
surfaces are the team's tags, the merge confirm and a team tag's history.

### Inventory

Rated by how often it's used (frequency) and what happens without it (necessity). A flow step names where it's
used. Necessity is the same for the web app and the CLI unless the row says otherwise.

| # | Entry | Flows | Frequency | Necessity |
| --- | --- | --- | --- | --- |
| W1 | Tag picker, on Add item and on the item's Tags field | A.1–4, B.1 | Constant | Core |
| W2 | The list's tag filter and a row's tag chip | B.1–3 | Daily | Core |
| W3 | A heap's Tags tab in Settings, with the team's tags beside the heap's | D.1, D′.1, E.1 | Sometimes | Core |
| W4 | The team's tags, on the Team page | C.1–4, E.1, G.1 | Sometimes | Core |
| W5 | The merge confirm | C.3, D.2, D″.1, E (rule 7) | Sometimes | Core |
| W6 | A team tag's edit form, saying it changes in every heap | E.1–3 | Sometimes | Enabler |
| W7 | A team tag's history | G.1, E′.1 | Sometimes | Enabler |
| W8 | A refused heap tag name that is a team tag's (rule 3) | D′, scenario 2 | Sometimes | Enabler |
| W9 | The item's history and the heap's Activity naming a merge | A′, rule 7 | Sometimes | Enabler |
| W10 | The move, keeping team tags and naming dropped heap tags | F.2 | Sometimes | Nice to have |
| W11 | The kind shown on a list row's tag chip | A, B | Constant | Polish |
| C1 | `upheap tag list`, with the team's tags beside the heap's | H.3 | Daily (agents) | Core |
| C2 | `--tag` and `--untag` naming a team tag on `item add`, `edit`, `list` | H.1–2 | Constant (agents) | Core |
| C3 | `upheap tag add --team`, `tag promote`, `tag edit` on a team tag | I (C, D, E) | Sometimes | Core |
| C4 | The confirm of a merge in the CLI | I (C.3, D.2) | Sometimes | Core |
| C5 | The agent-loop refusal of a rename or merge beyond its heaps | rule 19, scenario 7 | Sometimes | Core |
| C6 | `upheap tag history` | G for an agent | Sometimes | Nice to have |
| A1 | API: the team's tags (list, make, rename, describe) | C, E, I | Sometimes | Core |
| A2 | API: a heap's tags list and item tag writes accept team tags | A, B, H | Constant | Core |
| A3 | API: promote a heap tag, and the reach of a merge before it's confirmed | C.3, D, I | Sometimes | Core |
| A4 | API: a team tag's history | G | Sometimes | Enabler |

Why each Core and Enabler:

- W1, C2, A2: tagging an item is the loop (A, H); without team tags here nothing else matters.
- W2: B is a loop, and the golden path's step 4 filters by a team tag.
- W3: promoting starts from a heap's tags (D.1), and the heap's list of tags must show the team's, or a member
  can't tell why a name is taken (rule 3).
- W4, A1: the only place a team tag is made from nothing (C.1), and where its rename reaches every heap.
- W5, C4, A3: a merge can't be undone (D″), and rule 9 requires the reach shown and confirmed first.
- W6: a member is told a rename reaches every heap (user story 7); without it, E still works, but blind.
- W7, A4: E′ is undone from the history; without it the member must guess who changed what.
- W8: without the refusal naming the team tag, D′ dead-ends on an error that doesn't say what to do.
- W9: rule 7 says an item's history keeps the merge; without it, a closed item's tag looks changed by nobody.
- C1: an agent learns which tags a heap has from this list (H.3); without the team's, it refuses names that exist.
- C3: flow I. C5: the agent loop's promise (product.md, The agent loop §4).

W10 needs no change: the move already says which heap tags were dropped, and team tags never are (rule 16). W11
and C6 go to Ideas.

### The first set

W1 to W9, C1 to C5, A1 to A4.

Layout by frequency: the picker and the filter (constant, daily) are where they are today, with the team's tags in
them. Everything else is Sometimes and sits one or two steps away: a heap's Tags tab (Settings, then Tags) and the
team's tags (the team in the sidebar, then Tags).

### Web app

```
W1 · TAG PICKER (Add item, item's Tags field)            constant · core
┌──────────────────────────────┐
│ [sec_                      ] │  typing filters
│ security          team       │  heap's and team's tags together,
│ secrets                      │  one list in name order, each team tag marked
│ + Create heap tag "sec"      │  only when no tag has the name (rule 14)
└──────────────────────────────┘
States: no tags yet (a line, and Create), a name that only differs in case picks the tag that has it, saving,
failed (the pick is undone, and why).
Next likely action: pick, then close or pick another.
```

The mark says which kind a tag is (rule 13). Nothing else differs: a team tag picked in heap A is picked and
saved as a heap tag is. "Create" says it makes a heap tag, because the team's tags are made on the team.

```
W2 · LIST: TAG FILTER, ROW CHIP                           daily · core
Filter ▸ Tags:  [ ] billing   team
                [ ] infra
                [x] security  team
Row:  Rotate the certs        [security] [infra]
States: a saved view names the tag, not its name, so after a rename it shows the new name (rule 10).
Next likely action: pick another tag, or clear.
```

The filter lists the heap's tags as W1 does, with the mark. A row's chip shows the name only, as today, and filters
by the tag as today (W11 left out).

```
W3 · HEAP SETTINGS ▸ TAGS                                 sometimes · core
┌───────────────────────────────────────────────┐
│ New tag: Name · Description · [Add tag]        │  makes a heap tag
│ 6 tags of this heap                            │
│  [billing]  Invoices and payments   [Edit] [⋯] │  ⋯: Make a team tag, History
│  …                                             │
│ 3 tags of the team Acme  (used in every heap)  │
│  [security] Anything with access    [Edit] [⋯] │  ⋯: History; link to the team's tags
│  …                                             │
└───────────────────────────────────────────────┘
States: no heap tags, no team tags (a line saying where team tags are made); an archived heap (values only, as
today; the team's tags still show their current names).
Next likely action: Edit a tag, or Make a team tag.
```

- Two cards, the heap's first, then the team's, each with its count. A team tag is edited in place here, as on the
  team's tags (flow E.1), with W6's line.
- **Make a team tag** on a heap tag's ⋯ opens W5 for its name. It's under ⋯ because it's rare and can't be undone.
- Adding or renaming a heap tag to a team tag's name is refused under the field (W8).

```
W4 · TEAM PAGE ▸ TAGS                                     sometimes · core
┌───────────────────────────────────────────────┐
│ Acme                                       [⋯] │
│ Heaps …                                        │
│ Tags   Used in every heap of Acme              │
│   New team tag: Name · Description · [Add]     │
│   [security] Anything with access    [Edit] [⋯]│  ⋯: History
│   [billing]  Invoices and payments   [Edit] [⋯]│
│ Members …                                      │
└───────────────────────────────────────────────┘
States: none yet (a line: a team tag can be used in every heap; heap tags can be made team tags from a heap's
Tags), adding, a name that merges (W5 opens instead of adding), a failed add (under the field).
Next likely action: Add, then go to a heap and use it.
```

- A Tags section on the Team page, between Heaps and Members, in the settings layout (ux.md Rules §3). Every
  member sees it and may add and edit (the roles table), personal teams too.
- Add with a name no heap tag has makes the tag at once (rule 5). With a name heap tags have, W5 opens first.
- On a phone the rows are 52 px and Edit and History are in a row's ⋯ sheet, as heap rows are.

```
W5 · MERGE CONFIRM (dialog; sheet on a phone)             sometimes · core
┌──────────────────────────────────────────────┐
│ Make "billing" a team tag?                    │
│ These heap tags become it, with their items:  │
│   Launch     billing   12 items               │
│   Support    Billing    3 items               │
│   Old site   billing    5 items  (archived)   │
│ Their items keep their history. This can't be │
│ undone.                                       │
│ Description: (from Launch's tag)              │
│                  [Cancel] [Make team tag]     │
└──────────────────────────────────────────────┘
States: loading the reach; the reach changed (refused: the new list shows, marked as changed, and the button
confirms that one); failed (why, and the dialog stays).
Next likely action: confirm.
```

- Opened by W4's Add, W3's Make a team tag, and a team tag's rename to a name heap tags have (W6). The heading
  names what's being done: making, promoting, or renaming into the name.
- One row per heap the merge reaches, with that heap tag's name as written and how many items carry it, open and
  closed. Archived heaps are marked, since rule 15 reaches them.
- The description shown is the one the team tag will have (rule 8).
- After confirming, a toast says how many items in how many heaps now carry the team tag.

```
W6 · EDIT A TEAM TAG (in W3 or W4)                        sometimes · enabler
  Name [security-review]   Description [ … ]
  Changes in every heap of Acme.
  [Save] [Cancel]  History
States: saving; a name heap tags have (W5 opens before anything saves); a version conflict, as for any tag;
a name another team tag has (refused under the field).
```

```
W7 · A TEAM TAG'S HISTORY (collapsed, under the tag's row)   sometimes · enabler
  Made by Ana · 3 Nov
  Merged Launch's and Support's "billing" (15 items) · by Ben · 4 Nov
  Renamed "security" → "security-review" · by Sam · today
```

From W3's or W4's ⋯, like a heap tag's History. Each entry says who, when and what; a merge names the heaps and
counts (concept "the team tag's history").

```
W8 · REFUSED NAME (heap tag Add or Edit)                  sometimes · enabler
  Name [bug]
  "bug" is a team tag of Acme. Use it on items here instead.
```

The refusal sits under the field and names the team tag (rule 3). Scenario 2's way out, putting the team tag on
the items, is the member's.

W9: the item's history says its heap tag became the team tag, who merged it and when ("billing, Launch's, became
the team tag billing · by Ben"). It's a line the server writes, in the item's History as today.

### Command line

Tag commands work in a heap, as today (`--heap`, or the default). `--team <team>` works on the team's tags instead.

```
$ upheap tag list
billing    Invoices and payments
infra      Servers and deploys
security   team   Anything with access
```

C1: the heap's tags and the team's, in one list in name order, the team's marked. `--quiet` prints names, `--json`
the server's body, which says each tag's kind. `upheap tag list --team acme` lists the team's tags alone.

C2: `--tag security` on `item add`, `item edit`, `item list` resolves to the heap's or the team's tag of that name,
whichever there is (rule 1). Nothing else changes, and a name with no tag is refused, as today, with the hint to
make a heap tag.

```
$ upheap tag add security --team acme --description "Anything with access"
Made team tag security in Acme.

$ upheap tag add billing --team acme
Making team tag billing in Acme merges these heap tags into it:
  launch     billing   12 items
  support    Billing    3 items
  old-site   billing    5 items  (archived)
This can't be undone. To go ahead, run:
  upheap tag add billing --team acme --confirm <reach>
(exit 3)

$ upheap tag promote billing          # a heap tag of the heap in use
… the same reach and the line to confirm it

$ upheap tag edit security --name security-review
Renamed team tag security to security-review in every heap of Acme.
```

- C3: `tag add --team` makes a team tag; `tag promote <tag>` makes a heap tag a team tag; `tag edit` takes a team
  tag by name in a heap, or with `--team`, as it takes a heap tag today.
- C4: a write that would merge is refused (exit 3) with the reach and the exact line that confirms it. That line
  names the reach shown, so if the reach changed it's refused again with the new one (rule 9). How the reach is
  named is the API's (Parked). With `--json` the refusal is the problem body, with the reach in it.
- A heap tag added or renamed to a team tag's name is refused (exit 3), naming the team tag (W8).

```
$ upheap tag edit ops --name operations
Refused: renaming team tag ops reaches support, which this agent wasn't given.
Hint: ask the person: upheap question request <key> "Rename the team tag ops to operations"
(exit 3)
```

C5: in the agent loop, a rename, description or merge reaching a heap the agent wasn't given is refused (rule 19),
naming those heaps, with the hint to ask its person.

### API

At the level of what each group offers; paths, shapes and how a reach is confirmed are below this step.

- **A1, the team's tags:** list them, make one, rename and describe one, as the heap's tags work today.
- **A2, a heap's tags:** the list returns the heap's and the team's, each with its kind. An item's tag writes and
  the list's tag filter take a team tag's ID in any heap of its team. A saved view and a parameter's links take it
  too (rule 12).
- **A3, merges:** promoting a heap tag, and any make or rename that would merge, is refused with the reach until the
  request confirms that reach (rule 9). A heap tag's `allowed_actions` says whether the caller may promote it.
- **A4, history:** a team tag's history, read by every member of the team (rule 20).

### Choices

- **The team's tags live on the Team page,** as a section between Heaps and Members, rather than a page of their
  own or a tab in every heap's Settings. A team's other shared things are there, and it's used sometimes. A page of
  its own is the alternative if the section grows long (Open for now, "Many team tags").
- **A heap's Tags tab lists the team's tags too, editable in place,** rather than read-only with a link. Flow E
  starts "where a heap shows it", and a member who sees a name refused (W8) finds the tag on the same page.
- **Promote is under a heap tag's ⋯,** not a button on the row: it's rare, and it can't be undone.
- **A row's chip doesn't show the kind** (W11, Polish). The row is the most used layout, and a member needn't know
  the kind to use a tag (commitment 8). The picker, the filter and the Tags tab show it.
- **The CLI confirms a merge by running the printed line again,** not by a prompt: the CLI has no prompts (cli.md,
  Goals 4), and the line names the reach the person saw.

## 6. Stages

### Stage 1: the golden path, end to end

Every step of the path works in the web app, the CLI and the API, on SQLite, MySQL and Postgres, and ships in a
release as any change does. Six pieces, one pull request each, in this order. Piece 2 and piece 4 wait for piece 1,
piece 3 for piece 2, piece 5 for pieces 2 and 4, and piece 6 for the rest.

1. **Team tags in a heap (server and API).** A team has tags, and a heap's tags are its own and its team's
   together. The team's tags can be listed, made, renamed and described (A1). A heap's tags list says each tag's
   kind, and an item's tags, the list's filter, saved views and parameter links take a team tag in any heap of its
   team (A2). A heap tag can't take a team tag's name, and the refusal names it (rule 3). A move keeps team tags
   (rule 16). Making, renaming and describing a team tag are recorded in its history (concept "the team tag's
   history"); reading it is stage 2. In the agent loop, a rename or description reaching a heap the agent wasn't
   given is refused (rule 19). Done when an API test makes a team tag, puts it on items in two heaps, renames it once, and reads the new name
   from both, and the migration passes on all three databases. Covers commitments 1 to 6, 8, 10 and 11 in part.
2. **Merges (server and API).** Making, promoting or renaming a team tag into a name heap tags have merges them,
   in every heap, archived ones and closed items included, with saved views and parameter links following (rules 6
   to 9, 11, 15). The write is refused with its reach until the request confirms that reach, and refused again if
   it changed. The merge is in the team tag's history and in each item's (rule 21, W9). Done when scenarios 1, 3,
   4, 5, 6 and 8 pass as tests. Covers commitment 7.
3. **CLI.** `tag list` with both kinds, `--tag` naming a team tag, `tag add --team`, `tag edit` on a team tag,
   `tag promote`, the merge refused with its reach and the line that confirms it, and the agent-loop refusal (C1
   to C5). Done when the CLI's golden-path test runs steps 1 to 7 of the path, and scenario 7 passes.
4. **Web: using team tags.** The tag picker and the list's filter offer the team's tags with their mark (W1, W2),
   the Team page has its Tags section to make, rename and describe them (W4, W6), and a heap's Tags tab shows the
   team's card, editable in place (W3). A heap tag refused for a team tag's name says so under the field (W8).
   Done when an end-to-end test makes a team tag on the Team page, picks it on items in two heaps, filters by it,
   renames it once, and sees the new name in both. Covers golden path steps 1 to 5.
5. **Web: merges.** The merge confirm (W5), opened from the Team page's Add, a heap tag's Make a team tag, and a
   team tag's rename into a taken name, with the changed-reach state. Done when an end-to-end test promotes a heap
   tag that two heaps have and sees both heaps' items carry the team tag. Covers golden path step 7.
6. **The whole path, and the permanent docs.** One end-to-end test walks the golden path, steps 1 to 7, with a
   move between heaps carrying a team tag and a heap tag. The core commitments go into product.md (Heaps, Manage
   tags, the roles table, Custom parameters), the mechanisms into architecture.md (tag ownership, names, locking,
   how the API and CLI name a team tag), the decision entry into decisions.md, and this doc goes (CLAUDE.md, docs
   rule 3). Done when that test passes in CI and the docs merge.

Piece 4 and piece 5 start from a mockup of their screens, approved before they're built (CLAUDE.md, Interface
changes).

### Stage 2: the rest of the first set

- **A team tag's history, read.** The API's history of a team tag (A4) and its collapsed history under the tag's
  row in the heap's Tags tab and on the Team page (W7). The entries are recorded from piece 1 on, so nothing is
  lost before it ships.

### Choices

- **Server pieces come before any client,** rather than one feature slice through every layer at once. Every
  client names tags through the API (A2), and a slice per client would change the API three times.
- **Merges are a piece of their own,** rather than inside piece 1. They carry the hardest rules (7 to 9) and the
  scenarios, and piece 1 already proves the "Done when" line without them.
- **Reading a team tag's history waits for stage 2** (W7, A4 are Enablers), while recording it starts in piece 1.
  Flow E′ works in stage 1 without it: a rename is undone by renaming back, and the history is there to read once
  stage 2 ships.
- **The agent-loop refusal is in stage 1** (pieces 1 and 3), not later. Once an agent can rename a team tag, the
  agent loop's promise (product.md, The agent loop §4) must hold.

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- Storage: a tag owned by a team or by a heap; what becomes of a heap tag's row once merged into a team tag.
- Locking: a team tag write touches every heap of the team, where tag writes lock one heap row today.
- The API names tags by ID checked against the heap in the path; a team tag's ID must pass that check in each heap.
- The CLI's `--tag` resolves names against the heap; with §6 a name still resolves to one tag.
- A move today carries tags over by name (`domain::item_move::carry_over`); team tags need no lookup.
- Audit: where a team tag's entries are recorded, since the heap log reads only what is recorded against one heap.
- Rule 9's confirm over the API: how a request states the reach it confirms.
- An item's history naming a merged heap tag: what a read shows for an entry recorded against the old tag's ID.
- A heap's Activity: whether a merge shows there as a line per heap, beside the team tag's history.

## Ideas

Outside the scope. Not commitments.

- Archiving a tag, team or heap, so it's no longer offered but stays on the items that have it.
- Filtering by a team tag across every heap of the team (a view across heaps).
- Demoting a team tag to a heap tag.
- Merging two tags of different names into one.
- Custom parameters defined once for the team.
- Showing a tag's kind on a list row's chip.
- `upheap tag history`, a tag's history in the CLI.
