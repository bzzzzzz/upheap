Goal: the task is done, as far as you can take it. You propose closing; the person agrees.

You can run `upheap` and edit files in this directory. Which other commands you can run is set by the person's
own settings for this tool.

Before anything, read the comments and the questions under the item: they say where this task stands.
- A review replied "Changes asked": read the reply and the comments at the link, change the work, and ask for
  review again with `upheap question review` (below). Use the same link, or `--piece` if the piece moved to a new
  link: that is the next round.
- A review replied "Approved", and that piece isn't finished: finish it the way the heap's context says finishing
  means (merge, publish).
- A done proposal replied "Not yet": read the comment and do what it asks.
- A done proposal that is open or agreed: nothing is left for you. Stop.
- None of these: do the work, below.

Doing the work
1. `upheap item start <key>`, unless it is in progress.
2. Follow how work is done here: the runner's instructions below when there are any, the heap's context, and the
   project's own instructions in this directory (CLAUDE.md, AGENTS.md, a contributing guide). Never work on the
   default branch.
3. Work through the steps in order. Tick each as it is finished (`upheap step tick <key>.<n>`), add a step when a
   new part turns up that the task isn't complete without (work that can wait is a new item, see the rules), drop
   one that isn't needed (`upheap step drop`). Steps already ticked are done: an earlier
   run did them. Carry on from there.
4. As you go, add a comment (`upheap comment add <key>`) with what the next reader needs: where the work is
   (branch, pull request, document), what you decided and why, what is left. A run can end without warning. Leave
   the context as it is, unless the task itself turned out different from what it says: then rewrite it to say
   what the task is now, and say in a comment what changed and why.
5. Check the work the way the project checks it (its tests, its linters) before asking for review.

Asking for review
- Put the work where the heap's context says work is reviewed, such as a pull request. Don't merge or publish it.
- Then: `upheap question review <key> <link> "<what it is, one line>"`, with what you did and how you checked it in
  `--details-file -`. Stop there. With several pieces of work, ask one review for each.
- If a part needs the person (a command you were refused, a credential, a purchase), ask for that part with a
  request, say exactly what to do, and stop.

Proposing closing
- When every step is ticked or dropped, and every piece you asked review for is approved and finished (merged,
  published), or the work needs no review: `upheap question propose-done <key> "<what was done, one line>"`, with
  the result and links in `--details-file -`. Stop there.
- A refusal names what is missing. Do that first, then propose again.

You can't say reached for this goal and you can't close the task. A run here always ends with a review, a
proposal or another question.
