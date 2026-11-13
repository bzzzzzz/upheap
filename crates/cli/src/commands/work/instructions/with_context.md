Goal: the item has its context.

You can run `upheap`, read files in this directory, and fetch and search the web. You can't edit files or run
other commands.

The item is a thought as the person wrote it. Work out what it means in this project and write that down as its
context: what it is about and why it matters, what is known, what is still open, and where the material lives
(files, links, related items). Look at the heap's context and at what it points to, in this directory or on the
web.

Write the context first. Then decide whether the item is already defined enough to be a task. It is only when all
of these hold: it is one piece of work; you can write what done looks like, under a line starting "Done when:"; the
steps are plain from what is known; and nothing is left to ask the person. If so, make it a defined task in this run:
`upheap item to-task <key>`, the "Done when:" line in the context, a step for each part of the work, in order
(`upheap step add <key> "<title>"`), and say `upheap work reached --stage defined`. Say in the result that it went
straight to defined.

If the item is already a task (the person asked for changes after an earlier turn made it one), work from what
they said: fix its context and steps, and say `upheap work reached --stage defined` once it fits.

If in any doubt, or if it is several pieces of work, or going on would need a question answered (anything under
"Still open" counts), stop at with context: don't make it a task, don't add steps, and ask nothing for defined.
The person decides whether there is a next turn.

Questions: this turn is light, and every question holds the note and puts a row in front of the person. So this
narrows "ask everything you need in one go": ask only what you can't write the context without. That is:
- the note can't be understood: it has two readings that would lead to different items, and nothing here says
  which; or
- it points at something only the person has: a missing link, name, file or account.
Anything about scope, details, choices or how to do it can wait for the turn that defines the task. Don't ask it:
put it in the context under "Still open", one line per point, written so that turn can ask it as it is. When
nothing can't wait, ask nothing.

Reached needs: a context on the item, and no open question. With `--stage defined`: a task with a context, at least
one step, and no open question.
