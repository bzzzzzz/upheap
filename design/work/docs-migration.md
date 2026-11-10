# Docs migration

Moving the docs to the layout in the maintainer's engineering base: a small tracker with a doc per track of work,
and [product.md](../product.md) and [decisions.md](../decisions.md) holding the core only (CLAUDE.md, rules 2 to
4). One pull request per step.

## Progress

- [x] 1. **Tracks.** `work/progress.md` becomes the tracker, `work/README.md`, without the table of what's built
  and the open-questions list. getting-in.md takes the shape of a track's doc. CLAUDE.md gets the new rules 2 to 4.
- [ ] 2. **decisions.md against its bar.** An entry stays when it had a real alternative someone would propose
  again, is costly to reverse, and has a reason the code can't show. For an entry that fails, the reason worth
  keeping becomes a "because" clause where architecture.md or ux.md describes the mechanism, and the entry goes.
  Its number isn't reused, and every citation of a removed entry is repointed or dropped. The pull request lists
  each entry removed, so the owner can keep one.
- [ ] 3. **product.md against its bar.** A point stays when it is a promise to a user, stated as an outcome, that
  only a product change could break, with about five to eight points an area. A point that says how a screen
  behaves moves to ux.md if it isn't there already. Code cites these sections by name and number, so every
  citation of a renumbered point is repointed in the same change. The pull request lists each point cut or merged.
  This doc and its tracker line go with it.

## Design

The rules are in CLAUDE.md, and the shapes of the docs are the base's. Nothing here to design.

## Open questions

## Ideas

- A sync of `engineering-principles.md` with the base, if the base's rules have changed since its commit.
