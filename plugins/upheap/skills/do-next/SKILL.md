---
name: do-next
description: Starts work on the next item in Upheap, the ranked todo list, through the `upheap` command-line tool, in an interactive session with the user. Takes the top-ranked item that isn't started and moves it to in progress, in one heap or across all heaps; continues an item already in progress only when asked. Gives a note its context and decides whether it stays a note or becomes a defined task. For a task, does the work or a part of it using the skills and project instructions available, or tells the user how to do it when an agent can't. Records progress and state in Upheap. Use ONLY when the user runs /upheap:do-next or names Upheap in the request ("do the next thing in Upheap", "work on the next upheap item", "continue the upheap task"). Do not use for a request to work on something that doesn't name Upheap, and do not use to only add items or only report status.
argument-hint: "[team/heap | all]"
---

# Do the next item in Upheap

Take the next item and move it forward, with the person in the conversation. A note gets its context and a
decision about what it is. A task gets done, or partly done, or handed to the person with instructions when it's
theirs to do.

Talk to the person here, in the conversation: questions, choices, what you're about to do. Record in Upheap what
has to outlast the conversation: the item's state, its context, its steps, and anything still waiting for them.
After every session, the item alone should tell the next reader where things stand.

## How to write

Everything you write into Upheap is read by a person later, often on a phone and with the conversation forgotten:
contexts, comments, steps, questions. Your replies are read now. Write all of it this way.

- **Concise.** People skim. Say it once, and cut filler, recaps and hedging.
- **Main point first.** A title names the thing in a few words. A context opens with what it is and why it
  matters, and each part leads into the next.
- **Plain and friendly,** the way a helpful colleague would say it, in the person's own terms. Technical only when
  the subject is.
- **Stands alone.** No "as discussed" or "the above": the reader won't have this conversation.
- **No AI phrasing:** "X, not Y" contrasts, "it's not just X, it's Y", lists of three by habit, "delve",
  "leverage", "seamless", "robust", "crucial", chains of dashes, rhetorical questions, emoji bullets, and closing
  summaries that repeat the body.

## 1. Check the tool and the sign-in

1. Run `upheap guide show`. It prints every command with its flags, and how the tool behaves. Read it: it is the
   source for every command below, and where it differs from this skill, the guide is right.
   - Command not found: stop. Tell the person Upheap's command-line tool isn't installed, and point them to
     https://github.com/bzzzzzz/upheap#install. Don't install it yourself.
   - It runs but has no `guide` command: stop. The tool is too old; ask them to update it.
2. Run `upheap heap list`. It needs the sign-in, and prints the heaps you can use.
   - Exit code 4, or a message that no server is set: stop. Tell the person to run `upheap auth login` themselves
     (with `--server <url>` the first time). It opens their browser, so you can't do it for them.
   - Exit code 5 (server unreachable) or 6 (the keychain or config is broken): stop and pass on the message.

## 2. Which heaps

- **One heap:** the person named it, in the command's argument or in their words. Use it.
- **All heaps:** they said "all", "everything" or "across heaps".
- **Nothing said:** the heap this conversation has been working in. Failing that, the default heap, which
  `upheap heap show` prints when one is set. Failing that, all heaps.

Pass the heap with `--heap team/heap` on every command.

## 3. Pick the item

Take the top ranked item that isn't started yet. In one heap, that's the first open item in
`upheap item list --heap <heap>`. Across all heaps, it's the first open item in the ranked list across heaps,
which the guide shows the command for: one list, in one order, with each item's heap. If the guide has no such
command, the tool is too old: say so, and ask which heap to work in.

Items in progress are not picked by default: they may be someone's work under way, here or elsewhere. Pick one
up only when the person asks ("continue", "carry on with what's started", or an item they name). Then take the
one they mean, or the highest in progress, and read where it got to before doing anything.

Pass over an item, and say so in a line, when:

- It waits for the person's answer. Tell them the question. If they answer now, record the answer as a comment
  (`upheap comment add`) and go on with it; the question itself is closed in the Upheap app, unless the guide
  shows a command for replying.
- It's been sent on to an agent: `upheap item show` prints a `Turn:` line. An unattended run may take it, so
  leave it, unless the person says to do it here.
- It's blocked.

Don't use `upheap work claim`, `work reached` or `work finish`: they are for runs with no person present.

Nothing to pick: say so, and stop. If items are in progress, name them and say you can continue one.

Then read it: `upheap heap show <heap>` for what the project is and how work is done in it, and
`upheap item show <key> --heap <heap>` for the item's context, comments, steps, and questions with their answers.

Tell the person in a line or two what you picked and why. Move it to in progress with `upheap item start <key>`,
a note as well as a task, so the list shows it's being worked on. Then start. Don't wait for a go-ahead, except
where section 6 asks for one.

When you stop with no work under way on it (a note that stays a note, a task handed to the person, a task defined
but not begun), move it back to open with `upheap item reopen <key>`. In progress means work is under way.

## 4. A note

A note is a thought that hasn't been worked out. Work it out, then decide what it is.

1. **Collect.** Find what bears on it: the heap's context, related items (`upheap item list <text>`), the files,
   code or links it points at, this conversation. Ask the person what only they know: what they meant, why it
   matters, what they'd want from it. A few questions, asked together.
2. **Write the context.** What it's about, why it matters, what's known, what's still open, and where the material
   lives. Facts and the person's words; mark what you inferred as an assumption. Set it with
   `upheap item edit <key> --context-file <file>`, and set effort, need or due if they're now clear.
3. **Decide,** and say why in a line:
   - **It becomes a task** when someone wants a result, and you can say what done looks like. Run
     `upheap item to-task`, write "Done when" into the context, and add a step for each part that's known. If the
     work is really several results, say so and ask before adding the others.
   - **It stays a note** when it's an idea nobody has decided on, a question still open, or something to keep in
     mind. It goes back to open, with its context.
   - **It's closed** when collecting settled it: `upheap item done` if there's nothing left to do, `upheap item
     wontdo` if the person says it won't happen.
   If the choice is the person's, ask them.
4. If the guide shows a way to set an item's stage, set it to match: with context, or defined.

Stop there. Report what the note is now, and say you can go on with it if it became a task. Go on only if they
say so; otherwise it goes back to open.

## 5. A task

### Is it defined?

A task is ready when its context says what the problem is and what done looks like. If it doesn't, define it
first: collect as for a note, ask the person what's missing, write "Done when", add the steps that are known. Once no
question is open on it, set its stage to defined, if the guide shows how. Then go on.

### Can an agent do it?

Decide before starting, part by part.

- **Yes:** do it (below).
- **Partly:** do your part, and hand over the rest (next section).
- **No:** it needs the person's hands, presence, accounts, money or judgment: a phone call, a purchase, a
  signature, a conversation, a physical thing. Hand it over.

### Handing it to the person

Telling the person how to do it is this session's result for that item.

1. Work out how: the steps in order, with the links, numbers, names and wording they'll need. Do the preparation
   an agent can do: find the form, draft the message, look up the phone number.
2. Tell them in the conversation, briefly, in order.
3. Record it, so it's there when they get to it: each step as a step of the task, with its details in the step's
   context (`upheap step add <key> "<title>" --context-file <file>`), or the whole of it as a comment on the item
   (`upheap comment add <key> --file <file>`) when it's one action.
4. Move the item back to open. Open a request on it with `upheap question request`, saying what to do in a
   line, so it shows as waiting for them.
5. If they do it right now and tell you, tick the steps, and close the item with `upheap item done`.

### Doing it

1. It's in progress already, from when you picked it.
2. **Find how this kind of work is done here,** before doing it your own way:
   - The heap's context, and the project's own instructions (CLAUDE.md, AGENTS.md, a contributing guide).
   - The skills available in this session: the project's, the person's own, and those from installed plugins.
     Match by what the task is: building or fixing code, reviewing, writing for people, a document or a
     spreadsheet, research, a design. When one fits, use it for that part of the work.
   - In another agent tool, its own equivalents.
   Nothing fits: do it with care in the ordinary way. Don't stop for that.
3. **Work through the steps in order.** Tick each as it's finished (`upheap step tick`). Add a step when a new
   part turns up, and drop one that turns out not to be needed (`upheap step drop`). A task with no steps and
   more than one part gets steps first.
4. **Record as you go,** as comments on the item (`upheap comment add <key> --file <file>`): what's done, where it
   is (the branch, the pull request, the document, the link), what you decided and why, and what's left. A comment
   adds to the item without touching what the person wrote. Keep the context for what the item is and "Done when";
   change it only to correct or restate that. Comment at each stopping point, not only at the end: a session can
   end without warning.
5. **Ask in the conversation** when a call is the person's, and record the answer as a comment. If they can't
   answer now, put it on the item with `upheap question ask` and stop work on that part.

### Where to stop

- **Done:** "Done when" is met, and whatever the heap's context says finishing means has happened. Tick the last
  step and run `upheap item done`.
- **Your part is done and it waits for a person:** a review, an approval, a merge, a reply. Leave it in progress,
  comment where it stands and the link, and open a request with `upheap question request` for what they need to
  do.
- **A good part is done and more is left:** leave it in progress with the steps ticked and a comment saying
  what's next. Stop when the session has gone on long enough or the next part needs a fresh start.
- **It can't go on:** something outside it has to happen first. `upheap item block <key>`, with `--by` when it
  waits for another item in the heap, and the reason in a comment.
- **It shouldn't be done at all:** tell the person why. Close it with `upheap item wontdo` only if they agree.

## 6. Ask first

Get the person's go-ahead in the conversation before anything that reaches other people or can't be undone:
sending or publishing, merging, deleting, spending money, changing something live. The item being next in the
list is not that go-ahead.

Make new items only for work the person asked for or agreed to. Something you noticed on the way goes in your
report; they can add it with `/upheap:add`.

## 7. Report

A short reply: the item's key and title, what you did, where it is now in Upheap (state, steps ticked), links to
what you made, and what waits for them. Then name what's next in the list in one line, and stop. One item per
run, unless they ask for more.
