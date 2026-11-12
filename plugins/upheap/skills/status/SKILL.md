---
name: status
description: Shows where things stand in Upheap, the ranked todo list, through the `upheap` command-line tool: what's in progress, what waits for the user, and what's next and why, for one heap or across all heaps. Read-only. Use ONLY when the user runs /upheap:status or names Upheap in the request ("upheap status", "what's next in Upheap", "where are we in the launch heap in upheap"). Do not use for a status question that doesn't name Upheap, and do not use to add, change or work on items.
argument-hint: "[team/heap | all]"
---

# Upheap status

Tell the person where things stand: what's in progress, what waits for them, and what's next and why. One heap or
all of them. Quick to read, with enough of an overview to decide what to do.

This skill only reads. It changes nothing in Upheap and starts no work.

## How to write

Your reply is read at a glance, often on a phone.

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

## 3. Read

For one heap:

1. `upheap item list --heap <heap>`. In-progress items are pinned at the top, the rest are in ranked order, and
   each row ends with the main reason for its place. The footer says how many are hidden as blocked.
2. `upheap item show <key> --heap <heap>` for each in-progress item, for its steps and open questions.
3. `upheap item show` for the top one or two ranked items too, when the row alone doesn't say why it's next or
   whether it's ready to start.

Across all heaps, read the ranked list across heaps, which the guide shows the command for: one list, in one
order, with each item's heap and the reason for its place. Open the in-progress items as in step 2, and the top
one or two as in step 3. If the guide has no such command, the tool is too old: say so, and ask which heap to
report on.

If `upheap item list` can show what waits for the person, use that. Otherwise you'll see it only on the items you
opened: say so in a line when you may have missed some.

## 4. Report

Lead with one sentence that says the state of things: how much is under way, whether anything waits for them, and
what's next. Then, for one heap:

- **In progress:** each item with its key, title and how far it got ("3 of 5 steps", or the last thing its comments
  or context say). Say so if nothing is.
- **Waiting for you:** each open question or request, with the item's key and the question in a line. Leave the
  heading out when there are none.
- **Next:** the top three ranked items, each with its key, title and why it's there in a few plain words ("critical,
  small", "due Friday"). For the first one, add a line on what starting it takes, if you read it.
- **Also:** one line for what's left: how many more are open, how many are blocked.

For all heaps, the same lead sentence and the same headings, over the one list: each item with its heap's name
beside its key. Next is the top five across heaps. Close with a line per heap that has open items: its name and
its counts.

Keep it to a screen. Use the item's own title, don't rewrite it. Don't list everything, don't explain how ranking
works, and don't offer a plan. End by saying `/upheap:do-next` starts the next one, in one line.
