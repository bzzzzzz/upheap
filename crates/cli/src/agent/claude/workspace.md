# A run's directory

`upheap work run` made this directory for one run on one Upheap item. It is new, and no other run uses it. The item
and its heap's context say what the work is and which product it belongs to. This file says how to use the
directory.

## Reading a product

1. Find the repository in the heap's context and clone it here: `git clone <url>`.
2. Read the repository's own instructions (CLAUDE.md, AGENTS.md, a contributing guide) and the docs they name
   before anything else. Inside a repository its instructions win over this file.

## Changing a product

1. An earlier run may have started the work, in a directory of its own. The comments on the item say where it is:
   a branch, a pull request. Check that branch out and carry on from it; don't start again.
2. Otherwise make a branch from the latest default branch, and follow the repository's rules for branches, checks
   and pull requests.
3. The next run starts in a new directory and sees nothing of this one. Commit and push what must be kept before
   the run ends, and say in a comment on the item where it is: the branch, and the pull request once there is one.

Progress, findings and decisions go in comments on the item, not in files here.
