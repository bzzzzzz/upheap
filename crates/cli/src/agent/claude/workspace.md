# An item's directory

`upheap work run` keeps this directory for one Upheap item, and every run on that item starts here. The item and its
heap's context say what the work is and which product it belongs to. This file says how to use the directory; it is
written again at the start of each run.

## What may be here already

An earlier run on this item may have worked here and left its files: a clone, a branch, build caches, changes that
were never committed. It may have stopped half-way, or failed. Before anything else:

1. Read the item's comments: they say where the work is (a branch, a pull request) and what is left.
2. Look at what is here. In a clone, check `git status` and the branch it is on, and compare them with what the
   comments say. Carry on from that work; don't start again, and don't throw away changes you don't understand.

## Reading a product

1. Find the repository in the heap's context. If it isn't cloned here yet, clone it: `git clone <url>`. If it is,
   fetch before you use it.
2. Read the repository's own instructions (CLAUDE.md, AGENTS.md, a contributing guide) and the docs they name
   before anything else. Inside a repository its instructions win over this file.

## Changing a product

1. If the work has started (here, or on a branch the comments name), check that branch out and carry on from it.
2. Otherwise make a branch from the latest default branch, and follow the repository's rules for branches, checks
   and pull requests.
3. This directory is removed once the item is closed or moves to another heap, or after 7 days with no run on the item, and a run on another
   machine starts without it. Commit and push what must be kept before the run ends, and say in a comment on the
   item where it is: the branch, and the pull request once there is one.

Progress, findings and decisions go in comments on the item, not in files here.
