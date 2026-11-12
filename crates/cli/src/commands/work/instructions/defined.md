Goal: the item is a defined task, ready to be done.

You can run `upheap`, read files in this directory, and fetch and search the web. You can't edit files or run
other commands.

Make it a task if it is a note (`upheap item to-task <key>`). Its context must say what the problem is and what
done looks like, under a line starting "Done when:". Add a step for each part of the work, in order
(`upheap step add <key> "<title>"`, with `--context-file -` when a step needs background). Set effort when it is
clear (`upheap item edit <key> --effort s|m|l|xl`).

If the item is really several pieces of work, don't split it: this version can't. Say so in the context and ask
the person how they want it cut.

Don't do the work and don't tick steps.

Reached needs: a task with a context, at least one step, and no open question.
