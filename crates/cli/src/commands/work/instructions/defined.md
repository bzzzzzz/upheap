Goal: the item is a defined task, ready to be done.

You can run `upheap`, read files in this directory, and fetch and search the web. You can't edit files or run
other commands.

Start with the context's "Still open" section, when it has one: the turn that gave the note its context left
there what could wait. Settle what you can from the code, the docs and the web, and mark those as assumptions. Ask
only what is left, in one go.

Make it a task if it is a note (`upheap item to-task <key>`). Its context must say what the problem is and what
done looks like, under a line starting "Done when:". Add a step for each part of the work, in order
(`upheap step add <key> "<title>"`, with `--context-file -` when a step needs background). Set effort when it is
clear (`upheap item edit <key> --effort s|m|l|xl`).

If the item is really several pieces of work, split it instead. First look at "Became" under the item: an earlier
run or the person may have made some of the tasks already, so carry on from them and make only what is missing.
Give one with no step its steps (`upheap step add`). On those tasks you can only add steps: if one has no context,
ask on the note (`upheap question ask <key>`) and stop.
1. Make each task, one at a time and in order, with its context (a line starting "Done when:") and its steps:
   `upheap item add "<title>" --task --from <key> --need <need> --age keep|new --context-file - --step "<title>"`,
   with the context on stdin. `--age keep` ranks it from the note's age, `--age new` from now.
2. Give the need and the age when the item's context or the person's line settles them. Never give a need above the
   note's. When they don't, ask one question on the note that lists the pieces (`upheap question ask <key>`), and
   stop.
3. A task that has to wait for an earlier one gets `--blocked-by <key of that task>` when it is made. Block only on
   tasks made from this note in this turn.
4. Then `upheap item became <key>`. It closes the note as became and reaches the turn. There is nothing more to do
   after it. If the note was sent all the way, each open task split from it gets a turn of its own then; don't start
   them in this run.
If the note is a part of an open task in this heap, make it a step of that task instead:
`upheap item to-step <key> --task <task>`. It closes the note and reaches the turn; there is nothing more to do
after it.
If the note is a project of its own, too big for tasks in this heap, make it a heap instead: first
`upheap heap add "<name>" --from <key> --context-file -`, with the heap's context on stdin (what the project is, where
its work lives and what finishing it means). Then make each of its tasks, with the context and steps as above, and
its notes (with a context), in the new heap: `upheap item add "<title>" --task --heap <team/heap> --from <key>
--need <need> --age keep|new --context-file - --step "<title>"`. Then
`upheap item became <key>`, which reaches the turn. The heap needs at least one task or note made from this one; its
tasks are defined by then, and get no turn in this run.
A single piece of work stays one task, in place (`upheap item to-task <key>`).

Don't do the work and don't tick steps.

Reached needs: a task with a context, at least one step, and no open question. For a split: no open question on the
note, and every open task made from it with a context and at least one step. For a heap made from it: a context on
the heap, at least one open task or note made from this one in it, and a context on each of those notes.
