# Work with agents: design

Upheap as a ranked todo list that also carries a project's context and hands part of the work to AI agents. It
replaces the pitch in [product.md](../product.md). In progress, by the steps of the maintainer's design method.
Step agreed: 1 (value).

## Constraints

1. One person working alone comes first. Sharing a heap with other people keeps working as it does today and gets
   no new design here. (Owner.)
2. Agents use Upheap through the command-line client only. (Owner.)
3. Upheap hosts no models. `upheap work` drives an agent command-line tool the person already has, locally:
   it runs the loop, gives the agent the prompts and context for working with Upheap, and works only in the heaps
   it was given. (Owner.)
4. A note is today's item, unchanged. (Owner.)
5. An agent both refines work and does it. (Owner.)
6. One person uses agents by running a server for themselves. The browser-only build stays as it is, without
   agents. (Owner.)
7. This pitch replaces the one in product.md, and it is still a ranked todo list holding more work than can be
   done. Agents are how part of that work gets done, not what the product is. (Owner.)
8. Upheap waits for the person and doesn't interrupt. It pushes only what they asked for: a reminder, or
   what they have said is critical to act on fast. Only the person decides what is critical. (Owner.)
9. What's built stays: the ranking and its factors, need, states, tags, the audit log, nothing deleted, the stack.
   (Repository.)

## 1. Value

### Who it's for and the problem

Upheap is a ranked todo list for a person, or a team's owner, with more work than can ever be done. Everything
goes in: ideas, tasks, things to remember. Upheap ranks it all and keeps the ranking current, so the owner always
knows what matters right now and what is on the horizon, and accepts that the bottom of the list may never
happen.

Part of that work can be handed to AI agents. Today doing so costs the owner time of its own: deciding what the
agent takes next, feeding it the background, tracking progress in text files inside the project where parallel
work collides, and doing the boilerplate between steps. Upheap takes that over. A heap carries the project's
context, the ranking decides what an agent picks up, and an agent that needs the owner leaves a question on the
item and moves on. The project's own files keep what it is, not how far along it is.

Upheap waits for the person. It shows what's important and what needs them when they come and look, and it
doesn't interrupt.

### The questions it answers

1. What's important right now, and what should I work on? (As today.)
2. What's on the horizon? (Whatever comes next in the list: scroll down, and it's still in priority order.)
3. Why is this here, and why isn't it higher? (As today.)
4. What is waiting for me: which questions and requests only I can deal with?
5. What should the agent pick up next? (Answered the same way every time, without asking a model.)

### What it is not

1. **Not a plan that gets finished.** The heap holds more than can be done. It orders the work; it doesn't
   promise all of it.
2. **Not something that interrupts.** Information appears when the person asks for it. Upheap pushes only what
   they asked to be pushed: a reminder, or what they have said is critical to act on fast.
3. **Not a model host.** It runs no models and manages no agent's tools. `upheap work` drives an agent
   command-line tool the person already has, such as Claude's or Copilot's, on their own machine: it runs the
   loop, tells the agent how to work with Upheap, and works only in the heaps it was given.
4. **Not a wiki or a document store.** A heap's context is short and points at where the knowledge lives: a
   repository, a wiki, a drive.
5. **Not a team project tracker.** No assignees, sprints, boards or reports. One ranked list per heap, as today.
6. **Not a chat.** A question is attached to an item, is answered once, and stays with the item.

### The shape of the offering

Free, open source and self-hosted, as today. The web app is for the person, phone included. The command-line
client is for agents and for scripts, and can do everything an agent needs.

One person runs the server for themselves, on their own machine with SQLite or hosted. The browser-only build
stays, without agents.

## 2. Golden path

### The path

1. I make a heap for a project and write its context: what it is and where its repository and docs are. I set
   the heap so agents prepare what I add.
2. An idea comes up. I add a note, with how much it's needed, in a few seconds. It's ranked with everything else.
3. `upheap work` runs on my machine. It asks Upheap what's next for an agent, gets the same answer any time it
   asks, and starts the agent only when there is something, with the heap's context and the item's.
4. The agent takes my note without being asked and fills in its context. It isn't sure about one thing, so it
   attaches a question and goes on to other work.
5. On my phone I see one thing waiting for me and answer it. The agent finishes: the note now has its context.
   That is a stable state, so the agent stops there and the note is mine again.
6. I read it, agree, and tell the agent to go on. It turns the note into two tasks, each with a checklist, the
   second blocked by the first. Stable again, and mine again.
7. I tell it to do the first task. It works through the checklist, ticking steps and adding ones it finds. One
   step needs me (a credential, a purchase), so it attaches a request; I do it and say so. The task is done, and
   the second task is no longer blocked.
8. For the second I say "all the way" up front, and the agent runs it to done without stopping in between.
9. The repository got the code and nothing about progress. I can see what happened, when, and what each run cost.

Two side paths: a note too big for a task becomes a new heap with its own context, notes and tasks. And a
reminder ("renew the certificate in March") stays out of the list until its date, then ranks with the rest, and
is resolved or turned into a task.

### Commitments

**Core** marks a promise that would go to product.md. The rest guide the design and stay here.

**Heaps**

1. **Core.** A heap has a context: text that says what the project is and points at where its knowledge lives,
   such as a repository or a wiki. Upheap stores the pointers and doesn't fetch what they point at.
2. Any member of the heap's team can edit its context.

**Items**

1. **Core.** An item is a note, a task or a reminder. All three are ranked together in one list, by the same
   factors.
2. **Core.** Every item has its own context, beside the heap's.
3. **Core.** A note is today's item, unchanged: it can be started, blocked and done on its own.
4. **Core.** A task is a note with a checklist. Each step has a title and its own context, and is ticked when
   done.
5. **Core.** A reminder has a date. It is out of the list until that date, and from then on is ranked like any
   item.
6. An item's kind changes only by the moves under "What can become what". Its history stays with it.

**Questions**

1. **Core.** A question is attached to an item, and is either a question to answer or a request to do something.
   An agent or a person can attach one.
2. **Core.** An item with an open question waits for the person. An agent never picks it up.
3. **Core.** The person can see everything that waits for them, across their heaps, in one place.
4. **Core.** Answering a question, or saying a request is done, closes it. When an item's last open question
   closes, the item goes back to whoever asked.
5. Questions and their answers stay with the item for good.

**Waiting, not interrupting**

1. **Core.** Nothing reaches the person unless they look: a question, a finished task and an agent's progress all
   wait in Upheap until then.
2. **Core.** Only what the person asked to be pushed is pushed: a reminder on its date, and whatever else they
   have said is critical to act on fast. Upheap and agents never decide that for them.
3. An agent that has to wait for the person goes on to the next item it can work on.

**Stable states**

1. **Core.** Work moves between stable states: points where an agent counts its part as finished and the item is
   the person's again. For an item they are: a note as written; a note with its context filled in; a defined
   task, with its checklist and context, ready to be done; done. A note too big for a task goes to a defined
   heap instead: a new heap with its context, tasks and notes. A heap whose tasks are all done is stable too.
2. **Core.** An agent's turn takes one item from one stable state to the next, and stops there. It never goes
   further on its own.
3. **Core.** An item with an open question or request is not at a stable state. The agent is in the middle of
   its turn, and carries on with it once the person has answered.
4. At a stable state the person decides: send it on, change it, leave it, or close it.

**Handing work to an agent**

1. **Core.** The person sends an item on by telling the agent to go one stable state further, or all the way to
   done.
2. **Core.** A heap can be set so agents prepare what's added without being asked: each new item is taken to its
   next stable state. Preparing never includes doing a defined task; that always needs the person's word.
3. **Core.** Outside those two cases an agent never picks an item up.
4. **Core.** At any moment an item is with the person or with an agent, and the list shows which.
5. **Core.** An agent refines and does: it fills in context, turns a note into tasks, asks, adds steps, and also
   does the work, ticking the steps as it goes.
6. **Core.** An agent that makes several tasks says which block which. A blocked task is never given to an agent,
   and a blocking task takes on the priority of what it blocks, as today.
7. The person can take an item back from an agent at any time.
8. What an agent does is in the audit log, told apart from what the person did.

**What can become what**

1. **Core.** A note can gain questions, become a task, or become a new heap with its own context, tasks and
   notes.
2. **Core.** A task can gain questions and gain steps, and is done like any item.
3. **Core.** A reminder is resolved, or becomes a task and from there behaves as one.
4. A person can make every one of these moves by hand, and an agent through the command-line client.

**Command-line client**

1. **Core.** It gives the next item for an agent by rules alone: the heaps' ranking, limited to items an agent
   may pick up (Handing work §1 to §3) that aren't blocked and don't wait for the person. The same state gives
   the same answer, and no model is asked.
2. **Core.** It reads and writes everything an agent needs: both contexts, checklists, questions, and the moves
   between kinds.
3. **Core.** `upheap work` runs the loop on the person's machine: it finds what's next, starts the agent tool
   the person named with the item and its contexts, and waits when there is nothing or everything waits for the
   person.
4. **Core.** The person says which heaps it may work in, and it touches no other. A heap an agent makes out of
   an item in one of those heaps is included.
5. `upheap work` gives the agent its instructions for working with Upheap: how to read context, ask, tick steps
   and make the moves. The person doesn't write them.
6. Upheap runs no models and doesn't manage the agent tool's own setup, sign-in or tools.

**Cost**

1. Every run of an agent on an item is recorded with the item: when, which stable state it started from and
   reached, and what it cost, as far as the agent tool reports it. An item touched five times shows five runs.
2. The person can see an item's runs, and the total for an item and for a heap.

**Where things came from**

1. An item, a heap or a checklist step made out of another item keeps a link back to it, and the source shows
   what came out of it.

**One person and teams**

1. **Core.** Everything above works for one person with no teammates, on a server they run for themselves.
2. The browser-only build stays as it is, without agents: it has no API for the command-line client to reach.
3. In a shared heap any member can send an item on, answer a question and edit a context. Nothing else is designed
   for teams here.

### Actors

| Actor | May |
| --- | --- |
| The person | Everything a member may do today, and: edit contexts, set a heap so agents prepare, send items on and take them back, answer questions, make every move between kinds |
| An agent, through the CLI, as the person | Read contexts; on items it may pick up only: attach questions, change the item's context, add and tick steps, set what blocks what, make the moves between kinds, change its state |

### Out

How a pushed reminder or a critical case reaches the person, how the person says what is critical, more than one
kind of agent, and who in a team answers a question. See Ideas.

## 3. Workflows

## 4. Behaviours

## 5. Interface

## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- How "go one further" and "all the way" are stored on an item.
- Whether a done task that an agent thinks needs follow-up makes a new note.
- What an agent's actions look like in the audit log, and which account it signs in as.
- Whether a checklist step can hold a question of its own.
- What state a note ends in once it has become a heap.
- What cost is counted in when the agent tool reports tokens, money or nothing.
- Two agents asking for "next" at once must not get the same item.

## Ideas

Outside the scope. Not commitments.

- **Spend and time limits** for `upheap work`.
- **Stories:** following a note through everything it turned into, with the cost of the whole added up.
- The channel for what is pushed, and how the person says what counts as critical.
- Several kinds of agent, with items sent to one of them.
- A person in a shared heap picking who answers a question.
