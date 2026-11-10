# Work with agents: design

Upheap as a ranked todo list that also carries a project's context and hands part of the work to AI agents. It
replaces the pitch in [product.md](../product.md). In progress, by the steps of the maintainer's design method.
Step agreed: 3 (workflows).

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
   step needs me (a credential, a purchase), so it attaches a request; I do it and say so.
8. The agent opens a pull request and asks me to review it. I comment there and reply "changes asked". It fixes
   them and asks again; I approve. The task is done, and the second task is no longer blocked.
9. For the second I say "all the way" up front. The agent runs it through without stopping at stable states,
   and still waits for my review before done.
10. The repository got the code and nothing about progress. I can see what happened, when, and what each run cost.

Two side paths: a note too big for a task becomes a new heap with its own context, notes and tasks. And a
reminder ("renew the certificate in March") stays out of the list until its date, then ranks with the rest, and
is resolved or turned into a task.

### Commitments

**Core** marks a promise that would go to product.md. The rest guide the design and stay here.

**Heaps**

1. **Core.** A heap has a context: text that says what the project is, points at where its knowledge lives,
   such as a repository or a wiki, and says where its work is reviewed and what finishing a piece means (merged,
   published). Upheap stores the pointers and doesn't fetch what they point at.
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
4. **Core.** Answering a question, or saying a request is done or can't be done, closes it. When an item's last
   open question closes, the item goes back to whoever asked.
5. Questions and their answers stay with the item for good.

**Review**

1. **Core.** An agent never marks its own work done. It asks for a review: a request on the task with a link to
   where the work is, such as a pull request or a document.
2. **Core.** The person reviews where the work lives, and replies in Upheap with one of two outcomes: approved,
   or changes asked, with an optional line. Their comments stay where the work is.
3. **Core.** Changes asked sends the task back to the agent, which reads the comments at the link, changes the
   work and asks again. There is no limit on rounds.
4. **Core.** A task can need several reviews, as when one change lands as several pull requests. It is done when
   the agent has nothing left to do on it and every review it asked for is approved.
5. A task in review is in the middle of the agent's turn, like any item with an open request. "All the way"
   still stops for each review.
6. Work the person does themselves needs no review.
7. Ticking a step needs no review. Steps track progress inside a task, and an agent or a person adds, rewords
   and ticks them freely while the task is in progress. Only the task's done is reviewed.

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

1. **Core.** The person sends an item on by telling the agent to go one stable state further, or all the way.
   "All the way" covers the item and the tasks made from it, up to done. A new heap made on the way is left at
   its defined state, for the person to send on. How far it reaches when work spawns more work is settled while
   building it, not here.
2. **Core.** A heap can be set so agents prepare what's added without being asked: each new item is taken one
   stable state forward, and stops there. Preparing never includes doing a defined task; that always needs the
   person's word.
3. **Core.** Outside those two cases an agent never picks an item up.
4. **Core.** At any moment an item is with the person or with an agent, and the list shows which.
5. **Core.** An agent refines and does: it fills in context, turns a note into tasks, asks, adds steps, and also
   does the work, ticking the steps as it goes.
6. **Core.** An agent that makes several tasks says which block which. A blocked task is never given to an agent,
   and a blocking task takes on the priority of what it blocks, as today.
7. The person can take an item back from an agent at any time.
8. What an agent does is in the audit log, told apart from what the person did.

**What can become what**

1. **Core.** A note can gain questions, become a task, become a step of a task that already exists, or become a
   new heap with its own context, tasks and notes.
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

**Agents' progress**

1. **Core.** The person can see what agents have done across their heaps as one short history of results: what
   was reached, on which item, and the main things produced. Details stay one step away, on the item.
2. An agent's run says, when it ends, what it reached and what it produced. The history is built from that, not
   from the audit log.

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

Two actors: the person, and an agent started by `upheap work`. Each flow is marked loop (many times a day or a
week), setup (once or rarely) or recovery.

### The person

**P1. Capture.** Loop.

1. Open Upheap, in the web app or the command-line client.
2. Add a note with a title and how much it's needed, or a reminder with a title and a date.
3. It's in the list, ranked. Nothing else is asked.

**P2. See what's important.** Loop.

1. Open a heap. The list is in priority order: the top is what matters now, further down is the horizon.
2. Each item shows its kind, whether it's with me or with an agent, and whether it waits for me.
3. Pick something and work on it (P5), or leave.

**P3. Clear what waits for me.** Loop.

1. Open the list of what waits for me, across all my heaps, in priority order.
2. Open one: the question or request, with the item and its context beside it.
3. Answer the question, or do what was requested and say it's done, or say it can't be done.
4. When that was the item's last open question, the item goes back to the agent. Next one.

**P4. Decide at a stable state.** Loop.

1. See the items an agent has brought to a stable state and that wait for my decision, in the agents' progress
   (P10).
2. Open one and read what the agent made: the context it filled in, the tasks and their steps, what blocks what.
3. Decide: send it on one state, send it all the way, change it and then send it on, leave it for now, or close
   it as won't do.

**P5. Do the work myself.** Loop.

1. Start an item, as today.
2. For a task, tick steps as they're done, and add or reword steps.
3. Mark it done. What it blocked is free.

**P6. A reminder comes due.** Loop, weekly.

1. On its date the reminder is in the list, ranked with the rest. If I asked for it to be pushed, it is.
2. Resolve it, or turn it into a task and carry on as with any task.

**P7. Shape things by hand.** Loop, occasional.

1. Turn a note into a task and write its steps, into a step of a task that already exists, or into a new heap.
2. Say that one item blocks another.
3. Attach a question to an item for myself or a teammate to answer later.

**P8. Start a project.** Setup.

1. Create a heap.
2. Write its context: what the project is, and where its repository and docs are.
3. Choose whether agents prepare what's added.

**P9. Start the agent loop.** Setup.

1. Sign the command-line client in, as today.
2. Run `upheap work`, naming the agent tool to use and the heaps it may work in.
3. Leave it running, or run it for a while and stop it. Either way it picks up where things stand.

**P10. See what the agents did.** Loop, daily.

1. Open the agents' progress: one history of their work across my heaps, newest first.
2. Each entry is a result, not a log: what was reached (a note got its context, a task was defined, a task was
   done, a heap was made), on which item, with the main things it produced, such as a pull request or a document,
   as links.
3. Open an entry's item when I want more: its runs with what each cost, and what it was made from or turned
   into. A heap shows its total cost.
4. Items waiting for my decision at a stable state (P4) are marked here, so this is also where P4 starts.

**P11. The agent got it wrong.** Recovery, for P4.

1. At a stable state the result isn't what I wanted: the context it filled in, or the tasks it defined.
2. Either fix it myself (edit the context, the steps, the blocks) and send it on.
3. Or reply "changes asked" with what's wrong, as in a review (P14), and the agent redoes that turn. The earlier
   attempt stays in the history.

**P12. Take an item back.** Recovery, for P4 and A1.

1. An item is with an agent and I want it: I changed my mind, or the agent is going the wrong way.
2. Take it back. The agent's turn ends, and what it had already changed stays.
3. The item is mine: I do it, change it, or send it on again later.

**P13. I answered wrong.** Recovery, for P3.

1. While the item is still open, change my answer.
2. If the agent has already acted on it, the item goes back to it with the changed answer shown as new.

**P14. Review.** Loop.

1. A review request is in what waits for me (P3), with a link to the work.
2. Follow the link and review there: comment on the pull request, in the document.
3. Back in Upheap, reply approved, or changes asked with an optional line.
4. Changes asked: the request comes back later with the work changed, and I review again from step 2.

### The agent, through `upheap work`

**A1. Take a turn.** Loop.

1. `upheap work` asks for the next item. Nothing: it waits and asks again later, and no agent is started.
2. Something: it starts the agent with the heap's context, the item with its context, its questions and answers,
   what the item was made from, and which stable state to reach.
3. The agent does the move: fills in context, writes a checklist, makes tasks, does the work and ticks steps.
4. It reaches the stable state and says so. The run and its cost are recorded, and the item is the person's.
   For done, that takes a review first (A6).
5. Sent "all the way", the item is the next one again, until it's done.

**A2. Ask.** Loop.

1. In a turn the agent needs something only the person has or can do.
2. It attaches a question or a request to the item and ends the run. The run and its cost are recorded.
3. `upheap work` goes on to the next item.

**A3. Carry on after an answer.** Loop.

1. The last open question on an item closes. The item is in the agent's queue again, at its ranked place.
2. `upheap work` starts the agent on it with the answers, and the turn continues as A1 from step 3.

**A4. Split.** Loop.

1. A note is more than one piece of work.
2. The agent makes several tasks from it and says which block which, or makes a new heap with its context,
   tasks and notes. Each keeps a link to the note.
3. Or the note belongs to a task that already exists: the agent adds it there as a step, with a link to the
   note, and the note is closed.
4. The note has reached its stable state. The new items are at theirs, and wait for the person.

**A6. Ask for review.** Loop.

1. The agent has a piece of the task's work ready where the heap's context says work is reviewed.
2. It attaches a review request with the link and ends the run. A task whose work lands in several pieces gets
   a request for each, at once or one after another.
3. Approved: on its next run the agent finishes that piece as the context says (merges, publishes). With no work
   left and every review approved, it marks the task done.
4. Changes asked: on its next run it reads the comments at the link, changes the work, and asks again.

**A5. A run dies.** Recovery, for A1.

1. The agent tool crashes, is killed, or `upheap work` is stopped in the middle of a turn.
2. What the agent had already written stays: ticked steps, context, questions.
3. The item is still with the agent and still not at a stable state, so it's the next item again, and the next
   run carries on from what's written.
4. A turn that dies repeatedly stops being retried and waits for the person, with what went wrong.

### User stories

From the steps above that ask something of the product.

| # | Who | Does | Gets | From |
| --- | --- | --- | --- | --- |
| 1 | Person | Adds a note or a reminder with a title and one more field | A ranked item, at once | P1 |
| 2 | Person | Opens a heap | One list in priority order, each item showing its kind, who has it, and whether it waits for them | P2 |
| 3 | Person | Opens what waits for them | Every open question and request across their heaps, in priority order | P3 |
| 4 | Person | Answers a question, or says a request is done or can't be done | The question closes, and the item returns to the agent when it was the last | P3 |
| 5 | Person | Looks for what reached a stable state | In the agents' progress, the items that wait for their decision | P4 |
| 6 | Person | Reads an item at a stable state | What the agent made: context, steps, new tasks, blocks | P4 |
| 7 | Person | Sends an item on, one state or all the way | The item is in the agent's queue | P4 |
| 8 | Person | Ticks, adds and rewords a task's steps | A checklist that shows how far the task is | P5 |
| 9 | Person | Resolves a due reminder or turns it into a task | The reminder closed, or a task in its place | P6 |
| 10 | Person | Turns a note into a task or a heap by hand, and sets blocks | The same result an agent would produce | P7 |
| 11 | Person | Writes a heap's context and sets whether agents prepare | A heap an agent can work in without being briefed | P8 |
| 12 | Person | Runs `upheap work` with an agent tool and heaps | A loop that works those heaps and nothing else | P9 |
| 13 | Person | Opens the agents' progress | One short history of results across their heaps, each with its item and the main things produced | P10 |
| 25 | Person | Opens an item's runs | Each run with its states and cost, and totals per item and heap | P10 |
| 15 | Person | Takes an item back | The agent's turn ends, and the item is theirs with what was changed so far | P12 |
| 16 | Person | Changes an answer | The agent sees the new answer on its next run | P13 |
| 23 | Person | Replies to a review request: approved, or changes asked | The agent finishes the piece, or changes it and asks again | P14, P11 |
| 24 | Agent | Asks for review with a link to the work | The task waits for the person, and can't be done without their approval | A6 |
| 17 | `upheap work` | Asks for the next item | One item or none, the same for the same state | A1 |
| 18 | Agent | Reads an item | Both contexts, questions with answers, where it came from, and the state to reach | A1 |
| 19 | Agent | Writes context, steps, tasks, heaps, blocks, and ticks steps | The changes kept as it goes, under its own name in the history | A1, A4 |
| 20 | Agent | Says it reached the stable state | The item is the person's, and the run is recorded with its cost | A1 |
| 21 | Agent | Attaches a question or a request | The item waits for the person and leaves its queue | A2 |
| 22 | `upheap work` | Finds a turn that died | The same item again, until it has failed too often, then it waits for the person | A5 |

### Not core

- Commenting on the work inside Upheap: comments stay where the work lives, and Upheap holds the link and the
  outcome.
- Upheap watching a pull request or a document for new comments. The person's reply in Upheap is what sends the
  work back.
- Setting up the agent tool, its sign-in and its permissions.
- Sending many items on at once.
- Anything about teammates beyond what a member can do today: who answers, who reviews.
- Being told about any of this without looking.

## 4. Behaviours

### Concepts

**Heap.** As today, and: a **context** (text); a **prepare** setting, on or off; and, when it was made from an
item, its **origin**. A heap is *defined* once the turn that made it ends, and *complete* while it has no open
item. Both are read off its items; neither is stored.

**Heap weight.** A number each person gives each of their heaps, one by default. It says how much that heap
matters to them against their other heaps, and is theirs alone: teammates in a shared heap each have their own.

**Item.** As today, and: a **kind** (note, task or reminder), a **context**, and an **origin** when it was made
from another item. Its work state is today's, with one more closed state:

- open, in progress, blocked, done, won't do: as today.
- **became**: closed because it turned into other things (several tasks, a step, a heap). It isn't done and
  isn't refused; the work carries on in what it became.

An open item also has a **stage**: how far it has been prepared. It is set when a turn reaches a stable
state, or by the person by hand, and never worked out from what the item happens to contain.

| Stage | Means | Starts here |
| --- | --- | --- |
| As written | Nobody has prepared it yet | Every new note; a task the person made by hand; a reminder turned into a task |
| With context | Its context says what it means in this project | |
| Defined | A task with a context and at least one step, ready to be done | |

An item is at a **stable state** when it has no open turn and no open question. Then its stable state is its
stage, or done or became once it's closed. The next stable state is the next row; for a task as written it is
defined, in one turn; after defined it is done. A note with context goes on by becoming one task, several, a
step or a heap (Kinds and moves).

**Who has it.** Read off the item, never set by hand:

- **Waits for the person:** it has an open question, its turn is stuck, or its blocker was closed as won't do
  or became.
- **With an agent:** it has an open turn and doesn't wait for the person.
- **With the person:** everything else.

**Step.** Belongs to a task. A title, a context, a place in the order, and an origin when it was made from a
note. It is open, ticked or dropped. A dropped step stays visible as dropped; steps are never deleted.

**Reminder.** An item of kind reminder with a date. Before the date it is in no list and no queue. From the date
on it is an ordinary open item.

**Question.** Belongs to an item, and may name one of its steps. Three kinds:

| Kind | Asked with | Closed by |
| --- | --- | --- |
| Question | Text | An answer in text |
| Request | What to do | "Done" or "can't be done", with an optional line |
| Review | A link to the work | "Approved" or "changes asked", with an optional line |

A question is open, closed, or withdrawn (the asker no longer needs it). Its answer can be changed while its
item is open.

**Turn.** One piece of an agent's work on one item: from the stable state it's at to the next. It has a
**reach** (one state, or all the way) and is opened by the person's go-ahead or by the heap's prepare setting. It
is open, then ends as reached or taken back. A reached turn can later be marked **redone**. An open turn can be
**stuck**. An item has at most one open turn. A go-ahead can carry a line from the person, shown to the turn's
runs.

**Run.** One start of the agent tool on one item, inside a turn. It records when it started and ended, how it
ended (reached, asked, failed), the state it started from and reached, a **result** (a line or two and links to
what it produced), and its cost as reported. A turn has one or more runs. Runs are never changed or deleted.

**Claim.** While a run is live it holds its item, and no other run is given it. A claim lapses when its run
hasn't been heard from for a set time.

**Agent.** Not an account. An agent acts as the person who runs `upheap work`, with their sign-in, and every
change it makes carries the run it was made in.

### Rules

**Kinds and moves**

1. A note becomes one task in place: the same item, with its key, need, age and history. A reminder becomes a
   task the same way.
2. A note that becomes several tasks, a step of an existing task, or a heap is closed as became. What it became
   keeps it as origin.
3. Need and age are decided at each split, for each thing made, the items of a new heap included: its need,
   and whether its age counts from the note's creation or from now. Nothing is inherited silently. A person
   splitting by hand picks both. An agent picks them when the note's context or the person's line settles it,
   and otherwise asks before its turn ends. An agent never gives a need above the note's.
4. A step can only be added to an open task in the same heap. A heap made from a note is in the note's team,
   with prepare off.
5. No other move changes a kind. A task doesn't go back to a note.
6. An item closed as became can be reopened within a week, like any closed item. What it became stays.

**Ranking**

1. All kinds are ranked by the same factors. Who has an item doesn't change its place.
2. A reminder's date is its exact deadline, and its age counts from that date. So on its date it arrives ranked
   as due today, not as an old item.
3. Before its date a reminder is in no list, search or count. The person finds it in a list of reminders to come.

**Opening a turn**

1. The person's go-ahead opens a turn on an open note or task, with a reach of one state or all the way.
2. In a heap with prepare on, a note the person adds gets a turn of one state at once. Items an agent makes,
   reminders, tasks, and notes that were there before prepare was turned on get none.
3. Prepare never opens a second turn on the same item.
4. No turn opens on a closed item, a reminder before its date, or an item in an archived heap.

**The agent's queue**

1. An item is in the queue when it has an open turn, doesn't wait for the person, isn't blocked, isn't claimed,
   and is in a heap `upheap work` was given. A heap made from an item in a given heap counts as given, and starts
   with its source's weight.
2. The queue is one list over all the given heaps. An item's place is its priority score in its own heap,
   times the weight the person running `upheap work` gave that heap. So the top items of every heap get worked,
   and raising a heap's weight moves all of its items up.
3. **Started work comes first.** An item whose turn already has a run, and which is ready to go on, gets a large
   boost in the queue: large enough that only something much more important passes it. Half-done work is
   finished before new work of about the same importance is begun. The size of the boost is a number in the
   formula, like the others.
4. "Next" is the first item of the queue, and gives its run the claim. It depends on stored state and the time
   only.
5. A run ends in one of three ways: **reached**, with its result; **asked**, having attached at least one
   question; or **failed**. A claim that lapses counts as failed.
6. Reached is checked, not believed: the turn ends, and the stage moves, only if the item fits the stage's row
   in the table and has no open question. Otherwise the run counts as failed.
7. After a set number of failed runs in a row, the turn is stuck: the item waits for the person, with what went
   wrong. The person retries it or takes it back.
8. An agent sets a task in progress when it starts doing it. Turns that prepare don't change the work state.

**All the way**

1. When a turn with that reach ends short of done, the next turn on the item opens at once, with the same reach.
2. Tasks made from the item get it too. Notes and heaps made from it don't.
3. It never answers a question, a request or a review.

**Questions and waiting**

1. Any open question on an item makes it wait for the person, whoever asked it.
2. When an item's last open question closes and its turn is open, the item is back in the queue.
3. Changing an answer on an open item with an open turn puts the item back in the queue, with the change marked
   as new. On a closed item an answer can't be changed; the person reopens the item first.
4. A request closed as "can't be done" goes back to the agent like any answer. The agent finds another way, or
   asks what to do instead; it doesn't ask for the same thing again.
5. "What waits for me" is every item that waits for the person, across their heaps, in one list ordered as the
   queue is: the item's score times the person's weight for its heap.

**Review**

1. A review is asked only on a task, in a turn that is doing it.
2. An agent can close a task as done only when every step is ticked or dropped, at least one review on it is
   approved, and for each piece of work the latest round is approved.
3. Changes asked closes that review. The agent's next request for the same work is a new review, shown as the
   next round of that piece.
4. An agent can't withdraw a review. Reviews withdrawn by a take-back count for nothing: after a new go-ahead
   the agent asks again.
5. Reviews on one task are answered one by one.
6. At a stable state short of done there is no review request. The person's "changes asked" there marks the
   turn that reached it as redone, puts the item's stage back to where that turn started, and opens a new turn
   to the same state, carrying what they said. What the agent wrote stays for the new turn to change.

**Blocks**

1. An agent sets blocks only among items it made in the same turn, and on the item it is working on.
2. When an item's blocker is closed as done, the item goes back to open on its own.
3. When the blocker is closed as won't do or became, the item stays blocked and waits for the person.
4. An item has one blocker, as today. Work that depends on several things is ordered as a chain.

**What an agent may do**

1. Read everything in the heaps `upheap work` was given.
2. Write only inside a run that holds a claim: the item's context, steps and state, questions on it, and the
   moves in Kinds and moves. Adding a step to an existing task is the one write outside its own item.
3. Set effort, impact and due time on what it makes, and its need at a split (Kinds and moves §3). It never
   changes the need of an item that exists.
4. Never: close anything as won't do, close anything as done except a task under Review §2, answer a question,
   change a heap's context or settings once the turn that made the heap has ended, or touch tags' and
   parameters' definitions.

**What the person does to a turn**

1. Take back: the turn ends as taken back, the claim is dropped, and the agent's open questions on the item are
   withdrawn. What the agent wrote stays. A run still going has its next write refused.
2. Starting, closing or reopening an item by hand takes it back first.
3. Editing an item an agent holds is allowed and doesn't take it back. The agent's write on the stale version
   is refused, and it reads the item again.
4. Archiving a heap takes back every open turn in it.

**History**

1. The agents' progress is the runs that ended as reached, newest first, each with its item, the state reached
   and its result. Asked and failed runs are on the item only.
2. Costs are added up per item and per heap, by the unit the agent tool reported them in.
3. Every change an agent makes is in the audit log as the person's, marked as made by an agent, with its run.
4. Contexts are part of the audit log like any field: each change keeps the text before and after.

**People**

1. In a shared heap any member may edit a context, give a go-ahead, take back and answer. An agent acts as the
   member whose `upheap work` started it, on any open turn in the heaps it was given.
2. Only an admin sets a heap's prepare setting.

**Pushing**

1. No rule here pushes anything. A reminder's push and the person's critical cases wait for a channel (Ideas).

### Scenarios

Each walked by hand through the rules above.

1. **An old idea against a new critical task.** A three-week-old idea note and a critical task added today both
   have open turns, neither started. Queue §2: both are in one heap, so its weight changes nothing and the
   heap's ranking decides. Critical pulls far harder than three weeks of an idea's slow ageing, so the task is
   next. The idea is taken when nothing above it is in the queue.
2. **A half-done task against something new.** Task A is mid-turn after an answer; note B gets a go-ahead.
   Queue §3: A is started and ready, so it has the boost. B a little above A in the heap: A is next, and B
   waits one run. B critical against an A that is nice to have: B passes the boost and is next. Accepted: started
   work is finished first unless what arrives is much more important.
3. **Splitting.** A two-month-old "expected" note is sent all the way. The agent makes tasks A, B, C, with B
   blocked by A and C by B. Moves §2: the note is closed as became. Moves §3: the note's context says all three
   serve the same promise, so the agent gives each "expected" and the note's age, without asking. All the way
   §2: each gets a turn. Queue §1: only A is in the queue. A's review is approved, A is done, Blocks §2 frees B,
   and B is next.
4. **Taken back mid-run.** The agent has ticked two of five steps and asked a question. The person takes the
   task back. Person §1: the turn ends, the question is withdrawn, the two ticks stay. The run's next write is
   refused. The task is with the person, in progress, two of five.
5. **A run dies.** Three steps ticked, then the machine sleeps. Queue §5: the claim lapses, failed. The task is
   in the queue again; the next run reads three ticked steps and carries on. Dies twice more: Queue §7, stuck,
   and it shows in what waits for the person.
6. **Two loops at once.** `upheap work` runs on two machines over the same heap. Both ask for next. Queue §4: the
   first gets the top item and its claim; Queue §1 keeps it out of the second's queue, which gets the next one.
7. **Two reviews, one approved.** A task lands as two pull requests. The person approves the first and asks for
   changes on the second. Review §2: not done. Review §3: the agent reworks the second and asks again, round 2.
   Approved: every review is approved, steps are ticked, done.
8. **A second thought after done.** A day later the person wants to change the first approval. Questions §3: the
   task is closed, so no. They reopen it (within a week), which puts it with them, and give a go-ahead with a
   line saying what's wrong. The earlier approvals stand, and the agent asks for a review of what it changes.
9. **A reminder in a heap that prepares.** "Renew the certificate" comes due. Ranking §2: it arrives as due
   today. Opening §2: prepare doesn't touch reminders. The person turns it into a task (Moves §1, in place) and
   gives a go-ahead.
10. **The person closes what the agent is reviewing.** A task has an open review; the person marks it done by
    hand. Person §2: taken back first, so the review is withdrawn and the turn ends; then done. No approval was
    needed, because the person closed it (Review §2 binds agents only).
11. **The blocker is refused.** A blocks B; the person closes A as won't do. Blocks §3: B stays blocked and waits
    for the person, who unblocks it, closes it, or points it at something else.
12. **Editing under the agent.** While an agent prepares a note, the person rewrites its title. Person §3: fine.
    The agent's write of the context is refused as stale; it reads the note again and writes a context for the
    new title.
13. **Two heaps.** The person weights "Work" 2 and "House" 1. House's top item scores 50 and Work's third
    scores 30. Queue §2: 30 × 2 = 60 beats 50, so Work's third is taken before House's first. They raise House
    to 3: its top item is now 150 and leads the queue.
14. **A split with nothing to go on.** A note with context is sent on and the agent finds two unrelated pieces
    of work in it. Moves §3: nothing settles their need, so it asks one question listing both, and the turn ends
    only after the answer.

Two scenarios changed the rules: 10 added "closing by hand takes it back first", and 11 added Blocks §3. A
review of this step added six more: the stage as something set, not worked out; the latest round of each piece
deciding done; agents never closing a note as done; a refused blocker counting as waiting; heaps an agent made
joining the queue; and need being settled for everything made from a note.

### Choices

**D** marks one that passes the bar for a decision entry.

1. **D. One item type with a kind, changed in place,** not three types. A note that becomes a task keeps its key,
   age and history, and one ranking covers everything. The alternative, a new item per move, loses the age and
   breaks links to the note.
2. **D. A closed state "became",** not an open parent that waits for its children. A parent would bring a
   hierarchy of items, with its own ranking and progress questions. Origins give the trail without one.
3. **D. Agents act as the person,** not as accounts of their own. No sign-up, membership or permissions for
   agents, and one person can't lose track of what "their" agent may reach. The cost: in a shared heap an
   agent's change reads as its member's.
4. **D. Turns and runs are stored,** not derived from the audit log. The queue, the claim, the cost and the
   progress page all read them.
5. **Started work gets a boost in the ranking,** not a rule of its own. "Always finish first" would let a
   trivial half-done item hold up something critical; ranking alone leaves a pile of half-done things.
6. **D. Heaps are compared by score times a weight the person sets,** not taken in a fixed order or one loop
   per heap. The top of every heap gets attention, and one number moves a heap. It assumes scores from
   different heaps can be compared as they are, which holds while heaps keep similar factor weights; a heap
   with unusual weights or many custom parameters will sit higher or lower than it should, and its heap weight
   is the correction. This is also the mechanism for the view across heaps in the tracker's Next list.
7. **One blocker per item,** as today, not several. Chains cover the cases an agent produces, and several
   blockers change the ranking's inheritance.
8. **Reached is checked against the table,** not taken from the agent. A rule decides whether a turn ended.
9. **Agents never set need or close as won't do.** Both say what matters, which is the person's.
10. **Prepare covers notes the person adds, once,** not everything in the heap. An agent's own output never
    feeds itself work.
11. **A reminder's age starts at its date,** not at creation. Otherwise a reminder set a year ahead arrives as a
    year-old item.

### Unproven mechanisms

1. **Driving an agent tool without a person at it, and getting its cost back.** Claude's command-line tool has a
   non-interactive mode that returns a result with its cost. Not run for this design, and not checked for other
   tools. The rules need only "run a command with a prompt, wait for it to exit"; cost is recorded "as reported"
   and may be empty. To be tried in stage 1's first piece that starts an agent.
2. **Nothing else.** Claims that lapse, checked moves and in-place kind changes are ordinary.

## 5. Interface

## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- How "go one further" and "all the way" are stored on an item.
- The exact reach of "all the way" when a turn spawns tasks, notes and heaps. To be iterated on while building.
- Whether a done task that an agent thinks needs follow-up makes a new note.
- How many failed runs stop the retries, and how long before a claim lapses.
- Whether heap scores need normalising before the heap weight is applied.
- When the reached check fails a run, the agent is told which condition it missed.
- A heap's cost can come in more than one unit, and the interface has to show that.
- What cost is counted in when the agent tool reports tokens, money or nothing.

## Ideas

Outside the scope. Not commitments.

- **Watching the place of review** (a pull request, a document) and sending the task back to the agent when
  comments arrive.
- **A daily outline** of the agents' work, written by an agent: the day's results and artifacts in a few lines.
- **Spend and time limits** for `upheap work`.
- **Stories:** following a note through everything it turned into, with the cost of the whole added up.
- The channel for what is pushed, and how the person says what counts as critical.
- Several kinds of agent, with items sent to one of them.
- A person in a shared heap picking who answers a question.
