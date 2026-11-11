---
name: add
description: Adds a note, a task or a reminder to Upheap, the ranked todo list, through the `upheap` command-line tool. Use ONLY when the user runs /upheap:add or names Upheap in the request ("add this to Upheap", "put a reminder in upheap", "upheap note: ..."). Do not use for a todo, note or reminder request that doesn't name Upheap, and do not use to work on, rank, list or close items.
---

# Add to Upheap

Put what the person asked for into Upheap as a note, a task or a reminder, in the right heap, with the fields that
are known. This skill only adds. It never starts an item, ticks a step, closes anything, or does the work the item
describes.

## 1. Check the tool and the sign-in

1. Run `upheap guide show`. It prints every command with its flags, and how the tool behaves. Read it: it is the
   source for every command below, and where it differs from this skill, the guide is right.
   - Command not found: stop. Tell the person Upheap's command-line tool isn't installed, and point them to
     https://github.com/bzzzzzz/upheap#install. Don't install it yourself and don't record the item anywhere else.
   - It runs but has no `guide` command: stop. The tool is too old; ask them to update it.
2. Run `upheap heap list`. It needs the sign-in, and prints the heaps you'll choose from.
   - Exit code 4, or a message that no server is set: stop. Tell the person to run `upheap auth login` themselves
     (with `--server <url>` the first time). It opens their browser, so you can't do it for them.
   - Exit code 5 (server unreachable) or 6 (the keychain or config is broken): stop and pass on the message.

Stopping means saying what's wrong and what the person should do, and keeping what they asked to add in your reply
so it isn't lost.

## 2. Pick the heap

1. One heap: use it.
2. Several: match what the item is about against each heap's name, and its context from `upheap heap show <heap>`
   when the name isn't enough. A heap the person named wins. The default heap is not a reason by itself.
3. Still unsure between two or more: ask before adding. This is the one question that comes before the item exists.

Pass the heap with `--heap team/heap` on every command, and say in your reply which heap the item went to.

## 3. Decide what it is

Take the person's word when they say note, task or reminder. Otherwise:

- **Reminder:** something to come back to at a time. "Remind me", "don't let me forget on Friday", "follow up in
  two weeks".
- **Task:** work with a result, which the person wants done. "We need to", "add a task to", a bug to fix, a thing to
  build.
- **Note:** everything else: a thought, an idea, an observation, something to look into one day.

Unsure between a note and a task: a note. It can become a task later and keeps its key.

## 4. Add it

Add the item first and ask afterwards, for all three. What the person said is then safe, and questions are cheapest
while the subject is fresh in their mind. Use `upheap item add ... -q` to get the item's key, and
`upheap item edit` for every change after that.

### A reminder

- Title: what to be reminded of, in the person's words, made concise. Don't reword it into something else.
- Due: always. A day they named becomes a date (`--due YYYY-MM-DD`, worked out from today's date); a rough time
  ("next week", "in a few weeks") becomes the matching rough choice.
- Description: only what they said that doesn't fit the title. Often nothing.
- If the guide lists a reminder kind or command, use it. If it doesn't, a reminder is an item with a due date.
- If the time or what it's about is unclear, add it with your best reading, then ask, then correct the item.

### A note

- Title and description: what the person said, and what this conversation already established about it. Short and
  to the point.
- Add nothing of your own: no assumed causes, no proposed solutions, no plan, no acceptance criteria, no research.
  A note is the person's thought, not your expansion of it.
- If something is unclear (what "it" refers to, which system, why it matters), ask after adding, and put the answers
  in the description.

### A task

A person who asks for a task usually has a definition in mind. Get it written down.

1. Add it with a title that names the result.
2. Collect its context from what the person said, this conversation, and the files or links at hand: what the
   problem is, what done looks like, what's already known or decided, and where the work lives. Write it to a file
   and attach it with `upheap item edit <key> --context-file <file>`. Facts and the person's decisions only; mark
   anything you inferred as an assumption.
3. Ask what's missing to do the work: done-when, scope, constraints. Update the context with the answers.
4. Add steps only when the parts are known, from the person or plainly from the context: `upheap item to-task`,
   then `upheap step add` for each. Don't invent a breakdown to have one.

### Asking

Ask in the conversation, briefly, the fewest questions that settle it, and update the item with each answer. If the
person can't answer now, or isn't there to answer, put the question on the item with `upheap question ask`, so the
item waits for them and the question isn't lost.

## 5. Fields

Set a field when the person said it or what's known matches it. Don't fill fields to have them filled.

### Effort

How much focused work the item takes for the person or team who will do it, start to finish, not counting waiting.

| Size | Work | Looks like |
| --- | --- | --- |
| `s` | Up to an hour | One sitting: a call, an email, a config change, a one-line fix |
| `m` | Up to a day | A few hours to a full day: a small feature, a bug that needs finding, a document |
| `l` | Up to a week | Several days: a feature with parts, a migration, anything needing review rounds |
| `xl` | More than a week | A project. Say so in your reply: it probably wants splitting |

Set effort whenever what's known roughly fits one row, on notes and reminders too, and pick the larger size when
it's between two. Leave it out only when there's nothing to judge by, such as an idea with no shape yet.

### Need, impact, due

- Need (`--need`): set it when the person says or plainly implies it. `idea`: kept so it isn't forgotten, nobody
  has decided it's wanted. `nice_to_have`: wanted, nobody is waiting. `expected`: someone is counting on it.
  `critical`: skipping it does real damage. Otherwise leave it to the default.
- Impact (`--impact`): only when the person says how much it matters.
- Due (`--due`): when the person gives a time. Always for a reminder.

### Tags and parameters

- Run `upheap tag list --heap <heap>`. Put on every tag whose name or description matches the item. No match: no
  tag.
- Parameters work the same way, if the guide shows a way to list and set them: use an existing parameter and option
  that match. If the guide shows none, leave parameters alone.
- Never create a tag, a parameter or an option, and never change one, unless the person asked for exactly that.

## 6. Report

One short reply: the item's key and title, the heap, what it is, and the fields you set (effort, due, tags, steps).
Name anything you assumed and any question still open. Then stop: the work itself is not part of this skill.
