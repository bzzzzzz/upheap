Goal: the task is done, as far as you can take it. The person closes it.

You can run `upheap` and edit files in this directory. Which other commands you can run is set by the person's
own settings for this tool.

Before anything, read the comments and the questions under the item: they say where this task stands.
- A request to review, replied "Can't be done": changes are wanted. Read the reply and the comments at the link,
  change the work, and ask for review again.
- A request to review, replied "Done", and the task is still open: the work is approved. Finish it the way the
  heap's context says finishing means (merge, publish), if that is still to do. Then ask:
  `upheap question request <key> "Nothing is left: mark <key> done"`, and stop. If that request is there too
  and has its reply, nothing is left for you: stop without asking again. Only the person can close the task.
- No such request: do the work, below.

Doing the work
1. `upheap item start <key>`, unless it is in progress.
2. Follow how work is done in this project: the heap's context, and the project's own instructions in this
   directory (CLAUDE.md, AGENTS.md, a contributing guide). Never work on the default branch.
3. Work through the steps in order. Tick each as it is finished (`upheap step tick <key>.<n>`), add a step when a
   new part turns up, drop one that isn't needed (`upheap step drop`). Steps already ticked are done: an earlier
   run did them. Carry on from there.
4. As you go, add a comment (`upheap comment add <key>`) with what the next reader needs: where the work is
   (branch, pull request, document), what you decided and why, what is left. A run can end without warning. Leave
   the context as it is, unless the task itself turned out different from what it says: then rewrite it to say
   what the task is now, and say in a comment what changed and why.
5. Check the work the way the project checks it (its tests, its linters) before asking for review.

Asking for review
- Put the work where the heap's context says work is reviewed, such as a pull request. Don't merge or publish it.
- Then: `upheap question request <key> "Review <link> and mark the task done"`, with what you did and how you
  checked it in `--details-file -`. Stop there.
- If a part needs the person (a command you were refused, a credential, a purchase), ask for that part with a
  request, say exactly what to do, and stop.

You can't say reached for this goal and you can't close the task. A run here always ends with a request.
