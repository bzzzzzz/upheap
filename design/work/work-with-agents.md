# Work with agents

Upheap as a ranked todo list that also carries a project's context and hands part of the work to AI agents:
notes, tasks with steps and reminders, questions that wait for the person, and an agent loop run from the
command line. It serves Product Notes, tasks and reminders, Questions, Working with agents, Review, The agent
loop, Heaps §5 and Distribution §4. This is the design, agreed by the six steps of the maintainer's design method,
and the reference while it's built. It changes only when the design does.

The pitch and the core commitments are in [product.md](../product.md), which is their home; where this doc and
product.md differ, product.md wins. Product is product.md below.

| Here, under Commitments | In Product |
| --- | --- |
| Heaps §1 | Heaps §5 |
| Items §1 to §5, What can become what | Notes, tasks and reminders, and Items §6 |
| Questions §1 to §4 | Questions |
| Stable states, Handing work, Waiting, Agents' progress | Working with agents |
| Review §1 to §4 | Review |
| Command-line client §1 to §4 | The agent loop |
| One person and teams §1 | Distribution §4 |

Everything else stays here until it's built: the commitments that aren't core, the workflows, the concepts and
rules, the choices (those marked D go to decisions.md once built), the interface inventory and wireframes.

## Constraints

1. One person working alone comes first. Sharing a heap with other people keeps working as it does today and gets
   no new design here. (Owner.)
2. Agents use Upheap through the command-line client only. (Owner.)
3. Upheap hosts no models. `upheap work` drives an agent command-line tool the person already has, locally:
   it runs the loop, gives the agent the prompts and context for working with Upheap, and works only in the heaps
   it was given. (Owner.)
4. A note is today's item, unchanged. (Owner.)
5. An agent both refines work and does it. (Owner.)
6. One person uses agents by running a server for themselves. (Owner.)
7. This pitch replaces the one in product.md, and it is still a ranked todo list holding more work than can be
   done. Agents are how part of that work gets done, not what the product is. (Owner.)
8. Upheap waits for the person and doesn't interrupt. It pushes only what they asked for: a reminder, or
   what they have said is critical to act on fast. Only the person decides what is critical. (Owner.)
9. What's built stays: the ranking and its factors, need, states, tags, the audit log, nothing deleted, the stack.
   (Repository.)

## 1. Value

Agreed, and now Product's Pitch: who it's for and the problem, the questions it answers, and what it is not.
The shape of the offering is in Product, Distribution.

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
   them and asks again; I approve. The agent merges it and, with nothing left, proposes closing the task. I
   agree: the task is done, and the second task is no longer blocked.
9. For the second I say "all the way" up front. The agent runs it through without stopping at stable states,
   and still waits for my review, and my agreement, before done.
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
2. **Core.** Every item has its own context, beside the heap's. Contexts, questions and replies are written in
   Markdown, and the web app shows them rendered.
3. **Core.** A note is today's item, unchanged: it can be started, blocked and done on its own.
4. **Core.** A task is a note with a checklist. Each step has a title and its own context, and is ticked when
   done.
5. **Core.** A reminder has a date. It is out of the list until that date, and from then on is ranked like any
   item.
6. An item's kind changes only by the moves under "What can become what". Its history stays with it.

**Questions**

1. **Core.** A question is attached to an item, and is either a question to answer or a request to do something.
   An agent or a person can attach one. It is free text, can offer choices to pick from, and is replied to with
   the choices picked and a comment.
2. **Core.** An item with an open question waits for the person. An agent never picks it up.
3. **Core.** The person can see everything that waits for them, across their heaps, in one place.
4. **Core.** Answering a question, or saying a request is done or can't be done, closes it. When an item's last
   open question closes, the item goes back to whoever asked.
5. Questions and their answers stay with the item for good.
6. A question comes short and long. The short version is a line that someone who knows the work can answer
   at once. The long version adds what anyone else needs to know first, and the item's context and the heap's
   are one step away from it.

**Review**

1. **Core.** An agent never marks its own work done. It asks for a review: a request on the task with a link to
   where the work is, such as a pull request or a document. Approval doesn't close the task: the agent finishes
   the piece (merges, publishes).
2. **Core.** The person reviews where the work lives, and replies in Upheap with one of two outcomes: approved,
   or changes asked, with an optional line. Their comments stay where the work is.
3. **Core.** Changes asked sends the task back to the agent, which reads the comments at the link, changes the
   work and asks again. There is no limit on rounds.
4. **Core.** A task can need several reviews, as when one change lands as several pull requests. When the agent
   has nothing left to do on it and every review it asked for is approved, it proposes closing. The person's
   agreement closes the task as done; declining, with a line, sends it back to the agent.
8. **Core.** A task whose work needs no review, with no pull request or document to look at, is closed the same
   way: the agent proposes closing once it thinks the work is done, and the person agrees.
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
2. In a shared heap any member can send an item on, answer a question and edit a context. Nothing else is designed
   for teams here.

### Actors

| Actor | May |
| --- | --- |
| The person | Everything a member may do today, and: edit contexts, set a heap so agents prepare, send items on and take them back, answer questions, make every move between kinds |
| An agent, through the CLI, as the person | Read contexts; on items it may pick up only: attach questions, change the item's context, add and tick steps, set what blocks what, make the moves between kinds, change its state |

### Out

How a pushed reminder or a critical case reaches the person, how the person says what is critical, more than one
kind of agent, and who in a team answers a question. Each is an idea in Upheap.

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
5. Approved: the agent finishes the piece (merges, publishes). With nothing left, a done proposal is in what
   waits for me. I agree, and the task is done; or I say not yet, with a line, and it goes back to the agent.

### The agent, through `upheap work`

**A1. Take a turn.** Loop.

1. `upheap work` asks for the next item. Nothing: it waits and asks again later, and no agent is started.
2. Something: it starts the agent with the heap's context, the item with its context, its questions and answers,
   what the item was made from, and which stable state to reach.
3. The agent does the move: fills in context, writes a checklist, makes tasks, does the work and ticks steps.
4. It reaches the stable state and says so. The run and its cost are recorded, and the item is the person's.
   For done, the agent proposes closing, after a review where there is work to look at (A6), and the person's
   agreement closes the task.
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
   left and every review approved, it proposes closing the task. Work with nothing to review goes straight to the
   proposal.
4. Changes asked: on its next run it reads the comments at the link, changes the work, and asks again.

**A5. A run dies.** Recovery, for A1.

1. The agent tool crashes, is killed, hangs (showing no output for 30 minutes, when the loop stops it), or
   `upheap work` is stopped in the middle of a turn.
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
- Anything about teammates beyond what a member can do today: who answers, who reviews.
- Being told about any of this without looking.

## 4. Behaviours

### Concepts

**Heap.** As today, and: a **context** (text); a **prepare** setting, on or off; and, when it was made from an
item, its **origin**. A heap is *defined* once the turn that made it ends, and *complete* while it has no open
item. Both are read off its items; neither is stored.

**Heap weight.** A number each person gives each of their heaps, one by default. It says how much that heap
matters to them against their other heaps, and is theirs alone: teammates in a shared heap each have their own.

**Item.** As today, and: a **kind** (note, task or reminder), a **context** (the description it has today, as
Markdown), and an **origin** when it was made from another item. Its work state is today's, with one more closed state:

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

**Reminder.** An item of kind reminder with a date. Before the date it is out of the ranked list and the queue,
and still found by search. From the date on it is an ordinary open item.

**Question.** Belongs to an item, and may name one of its steps. Every question has one shape:

- **What is asked,** short: a line with the question or the action, enough for someone who is up to date.
- **The details,** optional: the long version, with the background needed to answer, summed up from the
  item's context and the heap's. An agent writes it as part of asking.
- **Choices,** optional: a list of options to pick from, either one of them or any number.
- **The reply:** the options picked, if there are choices, and a free-text comment. With no choices the comment
  is the answer; with choices it is optional.

Whoever asks writes the text and the choices. Two kinds come with their choices fixed, because rules read them:

| Kind | Asked with | Choices |
| --- | --- | --- |
| Question | Text, and any choices the asker likes | The asker's, or none |
| Request | What to do | Done, or can't be done |
| Review | A link to the work | Approved, or changes asked |
| Done proposal | That nothing is left | Close as done, or not yet |

A question is open, closed by a reply, or withdrawn (the asker no longer needs it). Its reply can be changed
while its item is open.

**Text.** Contexts, a question's details and the comment in a reply are Markdown. The web app shows them
rendered and edits them as Markdown; the command-line client reads and writes the Markdown itself.

**Turn.** One piece of an agent's work on one item: from the stable state it's at to the next. It has a
**reach** (one state, or all the way) and is opened by the person's go-ahead or by the heap's prepare setting. It
is open, then ends as reached, handed back or taken back. It also ends as reached when the person sets the item's
stage to the turn's goal or past it. A reached turn can later be marked **redone**. An open turn can be
**stuck**. An item has at most one open turn. A go-ahead can carry a line from the person, shown to the turn's
runs.

**Run.** One start of the agent tool on one item, inside a turn. It records when it started and ended, how it
ended (reached, asked, failed, handed back), the state it started from and reached, a **result** (a line or two and links to
what it produced), and its cost as reported. A turn has one or more runs. Runs are never changed or deleted.

**Claim.** While a run is live it holds its item, and no other run is given it. A claim lapses when its run
hasn't been heard from for a set time. The run is heard by the loop's heartbeat (the loop is up) or any write in
the run. Whether the agent itself is still working is the loop's to watch, from the tool's output, not the
server's.

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
3. Before its date a reminder is out of the ranked list, the queue and the counts. Search finds it, shown as a
   reminder with its date, and the person can see all reminders to come in a list of their own.

**Opening a turn**

1. The person's go-ahead opens a turn on an open note or task, with a reach of one state or all the way.
2. In a heap with prepare on, a note or task the person adds gets a turn of one state at once: a task goes to
   defined, never to done. Items an agent makes, reminders, and items that were there before prepare was turned on
   get none.
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
5. A run ends in one of four ways: **reached**, with its result; **asked**, having attached at least one
   question; **handed back**, with a reason, when there is nothing to do and the goal needn't be reached,
   such as work done elsewhere (the turn goes back to the person, and it isn't a failure); or **failed**. An item
   that already has what the goal needs is reached, not handed back. A claim that lapses counts as failed.
   So does a run whose tool showed no activity for 30 minutes: the loop stops it and fails it with its own reason.
6. Reached is checked, not believed: the turn ends, and the stage moves, only if the item fits the stage's row
   in the table and has no open question. Otherwise the run counts as failed.
7. After a set number of failed runs in a row, the turn is stuck: the item waits for the person, with what went
   wrong. Both kinds of failure, a lapsed claim and a silent tool, count toward stuck. The person retries it or takes
   it back.
8. A turn whose item is already at its goal stage or past it never starts a run: `next` skips it, and a claim
   ends it as reached, credited to the person running `upheap work`, and takes the next item. A goal of done is
   never met this way.
9. An agent sets a task in progress when it starts doing it. Turns that prepare don't change the work state.

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
2. An agent never closes a task; it proposes closing, with a done proposal. A proposal is asked only when every
   step is ticked or dropped, no question on the task is open, and for each piece of work that had reviews the
   latest round is approved. A task with no reviews may propose closing. The person's "close as done" closes the
   task as done and ends the turn as reached; "not yet" sends it back to the agent with what they said.
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
4. Never: close anything, as done or won't do (it proposes closing a task, Review §2), answer a question,
   change a heap's context or settings once the turn that made the heap has ended, or touch tags' and
   parameters' definitions.

**What the person does to a turn**

1. Take back: the turn ends as taken back, the claim is dropped, and the agent's open questions on the item are
   withdrawn. What the agent wrote stays. A run still going has its next write refused.
2. Starting, closing or reopening an item by hand takes it back first.
3. Editing an item an agent holds is allowed and doesn't take it back, except that setting its stage to the turn's
   goal or past it ends the turn as reached by the person (All the way §1 then applies). The agent's write on the
   stale version is refused, and it reads the item again.
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

1. No rule here pushes anything. A reminder's push and the person's critical cases wait for a channel, an idea in Upheap.

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
   §2: each gets a turn. Queue §1: only A is in the queue. A's review is approved, the agent merges and proposes closing, the person agrees, A is done, Blocks §2 frees B,
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
   Approved: the agent merges, ticks the last steps and proposes closing; every piece's latest round is
   approved, so the proposal is allowed. The person agrees: done.
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
   is the correction. This is also the mechanism for the view across heaps (a note in the Upheap heap).
7. **One blocker per item,** as today, not several. Chains cover the cases an agent produces, and several
   blockers change the ranking's inheritance.
8. **Reached is checked against the table,** not taken from the agent. A rule decides whether a turn ended.
9. **Agents never set need or close as won't do.** Both say what matters, which is the person's.
10. **Prepare covers what the person adds, once,** not everything in the heap. An agent's own output never
    feeds itself work.
11. **A reminder's age starts at its date,** not at creation. Otherwise a reminder set a year ahead arrives as a
    year-old item.
12. **An agent never closes a task; the person agrees to its proposal** (owner), not an agent closing once its
    reviews are approved. Approving a review and closing a task are different calls: a pull request approved
    still has to be merged, and a task with nothing to review still needs someone to say it's done.

### Unproven mechanisms

1. **Driving an agent tool without a person at it, and getting its cost back.** Proven for Claude's
   command-line tool (2.1.322) in stage 1's first piece; not checked for other tools. `claude -p --output-format
   json` reads the prompt on stdin, runs to the end with no one at it and exits. It prints one JSON object with
   `result` (the agent's last message), `subtype` and `is_error` (how it ended) and `total_cost_usd`.
   `--permission-mode dontAsk` with `--allowedTools "Bash(upheap:*)"` lets the agent run only `upheap`: any
   other command is refused without a prompt, the run carries on, and the refusal is listed in
   `permission_denials`. `work run` gives the tool two sets of flags. To prepare an item (`with_context`,
   `defined`): `--permission-mode dontAsk --allowedTools <upheap rules> "Read(./**)" "Grep(./**)" "Glob(./**)" WebFetch
   WebSearch`, where the upheap rules name the commands a run uses (`Bash(upheap item:*)`, `step`, `question`, `comment`,
   `work reached`, `guide`, and the read-only `heap`, `tag` and `parameter` ones) and leave out `auth`, so it
   can run `upheap`, read files under the directory `work run` was started in and use the web, and nothing else. To
   do a task (`done`): `--permission-mode acceptEdits --allowedTools <upheap rules>`, so edits are accepted,
   those `upheap` commands are always allowed, and every other command follows the allow rules in the person's own and the
   project's Claude settings; what they don't allow is refused without a prompt and the run carries on.
   `--max-budget-usd` stops a run that costs more, exiting 1 with `subtype`
   `error_max_budget_usd`. `--no-session-persistence` keeps runs out of the person's session list. Checked
   with 2.1.322: `Read(./**)` allows a file under the start directory and refuses `/etc/hosts`, where a bare `Read`
   allows any path; a heredoc on a command's stdin passes a prefix rule; that version has no Grep tool. Exit 0 with
   `is_error` false is a run that ended; anything else is a failed run, with its cost still reported.
2. **Nothing else.** Claims that lapse, checked moves and in-place kind changes are ordinary.

## 5. Interface

Two surfaces: the web app, for the person, and the command-line client, for agents, `upheap work` and scripts.
Everything here adds to [ux.md](../ux.md) and to the commands that exist. What isn't mentioned stays as it is.

An item's context is the description it has today, shown as Markdown and called Context. There is one text
field on an item, not two. So an agent that fills in the context rewrites what the person wrote, and the text
as it was before stays in the item's history.

### Inventory

Frequency is Constant (several times a session), Daily, Weekly, Sometimes or Once. Necessity is Core (no
workaround), Enabler (done painfully without it), Nice to have or Polish.

**Web app**

| # | Entry | Flows | Frequency | Necessity | Why, for Core and Enabler |
| --- | --- | --- | --- | --- | --- |
| W1 | List row: step count, reminder mark, who has it | P2 | Constant | Core | Without the marks an item an agent holds looks free to take |
| W2 | Waiting for me, across heaps | P3, P14, A5 | Constant | Core | Every agent turn that asks stops until the person finds the question |
| W3 | Replying: a question, a request, a review, a done proposal | P3, P14 | Constant | Core | The only way an item goes back to the agent, and the only way an agent's task is done |
| W4 | Item: context as Markdown | P4, P5 | Daily | Core | It is what the agent reads and what it writes |
| W5 | Item: steps | P4, P5 | Constant | Core | A task is its steps |
| W6 | Item: who has it, send on, take back, changes asked, retry | P4, P11, P12 | Constant | Core | Nothing reaches an agent without the go-ahead |
| W7 | Item: closed questions with their replies, change a reply | P13 | Weekly | Enabler | A wrong answer has no other fix |
| W8 | Item: what it came from and what it became | P4, P10 | Weekly | Enabler | After a split it's the only way from the note to its tasks |
| W9 | Item: runs with result, links and cost, and the total | P10, A5 | Weekly | Enabler | Asked and failed runs, and why a turn is stuck, are nowhere else |
| W10 | Agents: progress across heaps, and what waits for a decision | P4, P10 | Daily | Core | P4 starts here; a prepared note is otherwise found only by scrolling |
| W11 | Navigation: Waiting for me and Agents | all | Constant | Core | Both are across heaps and have no place today |
| W12 | Add: a reminder with its date | P1 | Weekly | Enabler | A reminder can't be made otherwise |
| W13 | Heap settings: context, agents prepare | P8 | Once | Core | Golden path step 1 |
| W14 | Heap settings: my weight for this heap | Queue §2 | Sometimes | Enabler | With two heaps it's the only way to say which comes first |
| W15 | The list: reminders to come, and search finding them | P6 | Sometimes | Enabler | A reminder with a wrong date can't be found to fix |
| W16 | Closed: items closed as became | P11 | Sometimes | Enabler | Reopening one within its week starts here |
| W17 | Item: make a note or a reminder a task | P6, P7 | Weekly | Enabler | P6 dead-ends: a due reminder can't become a task |
| W18 | Item: attach a question by hand | P7 | Sometimes | Polish | |
| W19 | Item: split a note by hand into tasks, a step or a heap | P7 | Sometimes | Nice to have | |
| W20 | Steps: reorder by dragging | P5 | Sometimes | Polish | |
| W21 | Activity and history: a mark on what an agent changed | P10, story 19 | Sometimes | Enabler | Otherwise an agent's wrong edit reads as the person's own |
| W22 | Heap: its total cost | P10, story 25 | Sometimes | Enabler | The only place the cost of a whole project is seen |

**Command-line client**

| # | Entry | Flows | Frequency | Necessity | Why, for Core and Enabler |
| --- | --- | --- | --- | --- | --- |
| C1 | `work run`: the loop | P9, A1 to A5 | Constant | Core | It is how an agent gets work |
| C2 | `work next`: what's next, without taking it | A1 | Weekly | Enabler | The promise that "next" is a rule can be checked, and a person's own loop needs it |
| C3 | `item show` and `heap show` with everything an agent reads | A1 | Constant | Core | The agent starts every run by reading |
| C4 | `item edit --context-file`, `heap edit --context-file` | A1, A4 | Constant | Core | Filling in context is the first turn of every note |
| C5 | `step add`, `tick`, `drop`, `edit` | A1 | Constant | Core | Doing a task is ticking its steps |
| C6 | `question ask`, `request`, `review`, `propose-done` | A2, A6 | Constant | Core | The agent's only way to reach the person |
| C7 | `work reached`: the run says what it reached and made | A1 | Constant | Core | An agent ends a turn no other way, bar a hand-back (C7a) |
| C7a | `work hand-back`: the run found nothing to do and gives the turn back | A1 | Weekly | Enabler | Without it a run that finds the work done must say reached, or fail |
| C8 | `item to-task` | A1 | Daily | Core | Golden path step 6 |
| C9 | Splitting: `item add --from`, `item became` | A4 | Weekly | Core | Golden path step 6 |
| C10 | `item to-step` | A4 | Sometimes | Enabler | A core move with no other way for an agent |
| C11 | `heap add --from` | A4 | Sometimes | Enabler | A core move with no other way for an agent |
| C12 | What a run is refused, and what it's told | A5, P12 | Weekly | Core | An agent that isn't told why repeats the same write |
| C13 | The person's side: `item send`, `item take-back`, `question list`, `question reply` | P3, P4 | Weekly | Nice to have | |
| C14 | `item list` showing kind and who has it | P2 | Weekly | Nice to have | |
| C15 | `item add --reminder` | P1 | Sometimes | Nice to have | |
| C16 | `work queue`: the whole queue, with why each is where it is | A1 | Sometimes | Polish | |
| C17 | `work log`: the agents' progress | P10 | Sometimes | Polish | |
| C18 | `step reopen`, `step move` | A1 | Sometimes | Polish | |

**The first set** is W1 to W17, W21, W22 and C1 to C12. W18 to W20 and C13 to C18 are ideas.

### Layout follows frequency

1. **Waiting for me and Agents are above the heaps,** since both are across heaps. Waiting carries its count. On
   a phone they take two of the four tabs, and Closed and Settings, used weekly and rarely, move into More.
2. **A reply is given in the row,** in Waiting and in the item alike, without opening anything. The row
   shows the short question and its reply, so someone up to date answers at a glance. Opening it shows the
   details, with the item's context and the heap's folded under them.
3. **In the item, what the person is likeliest to act on comes first:** an open question, then who has the item
   with the one action that fits, then the steps, then the context. What is read now and then (where it came
   from, closed questions, runs) is folded at the bottom, one step away.
4. **Rare things are two steps away:** a heap's context, prepare and weight in its Settings, and changing an
   item's kind or stage under the item's ⋯.
5. **An agent's common commands are one short line** with the item's key, and read as a sentence.

### Wireframes: web app

One wireframe per screen; entries that share a screen share a wireframe.

```
ALL WIDTHS · Navigation (W11)                         constant · core
Wide sidebar, tablet rail              Phone tabs
┌────────────────────────┐             ┌──────────────────────────────────────┐
│ Waiting for me       3 │             │ Open   Waiting 3   [+]  Agents  More │
│ Agents                 │             └──────────────────────────────────────┘
│ ────────────────────── │             More: Closed, Settings, Activity, Team,
│ [Work · Acme ▾]        │                   Teams, Account
│ Open 14                │
│ Closed                 │
│ Activity               │
│ Settings               │
│ team, Teams, Account   │
└────────────────────────┘
States: no count at zero. The count is fetched again every 30 s on every page, as the list is, so it
never lags a row that shows a question.
```

```
ALL WIDTHS · List row (W1)                            constant · core
│ ○ 3 ☑ Rotate the certs ▤ 2/5  [due] [tags]              with an agent │
│      Due this week · small effort                                     │
│ ○ 4   Paint the fence         [tags]                    waits for you │
│ ○ 5   Renew the domain  reminder                                      │
Marks: a task shows a checklist icon before the title (a note shows none, and the slot stays so titles line up),
       and after the title the stage: with context a page, defined a clipboard with a check, as written none.
       No words on the row; a screen reader hears them. The step count joins after the stage icon.
       A task shows steps ticked of steps; a reminder shows its mark; a note shows nothing.
       The end of the row says "with an agent" or "waits for you", and nothing when it's the person's.
States: the row's play and done buttons and the swipes work as today. On an item an agent holds they ask first, since
        changing its state takes it back and that can't be undone.
Next likely action: open the item.
```

```
ALL WIDTHS · Waiting for me (W2, W3)                  constant · core
┌────────────────────────────────────────────────────────────────────────┐
│ Waiting for me · 5                 all heaps, most important first     │
│                                                                        │
│ Review, round 2 · Rotate the certs · Work                              │
│   "The script and the runbook"   ↗ link to the work                    │
│   [Approved] [Changes asked]     comment, optional                     │
│                                                                        │
│ Question · Paint the fence · House                                     │
│   "Which colour?"                                          Details ▾   │
│     "The fence is cedar, last stained in 2022. The shed beside it      │
│     is white. Green needs a primer coat first."                        │
│     Context ▸  the item's · the heap's                                 │
│   ( ) White  ( ) Green                                                 │
│   comment, optional                                    [Reply]         │
│                                                                        │
│ Request · Rotate the certs · Work · step "Run in staging"              │
│   "Create a staging token and store it in the vault."                  │
│   [Done] [Can't be done]         comment, optional                     │
│                                                                        │
│ Stuck · Fix the footer · Site                                          │
│   3 runs failed: <what went wrong>           [Retry] [Take back]       │
│                                                                        │
│ Blocker closed · Ship the feed · Work                                  │
│   Waits for "Pick a host", closed as won't do     [Unblock] [Open]     │
└────────────────────────────────────────────────────────────────────────┘
One row per open question, so an item with two has two rows. The item's title opens the item.
A row shows the short question and its reply. Details, shown open on the second row, unfolds the long
version in place, rendered, and under it Context, which unfolds the item's context and the heap's. A
question asked without details has only Context there.
A question with no choices: the comment is the answer, and Reply stays off until it has text.
A question with choices: pick one, or any number when the asker allowed it.
States: empty ("Nothing waits for you"); loading; a failed refresh keeps the rows.
        After a reply the row leaves, and a toast offers Change, which opens the item at that reply.
Next likely action: the next row.
```

```
WIDE PANE, TABLET PANEL, PHONE SHEET · Item (W4 to W9, W17)   constant · core
┌────────────────────────────────────────────────────────────────────────┐
│ Rotate the certs                                    k3x9a  link   [✕]  │
│ ☑ Task · ▤ Defined                    the icons with their words       │
│ state buttons, as today                                                │
│                                                                        │
│ WAITS FOR YOU   only with an open question, a stuck turn or a closed   │
│   blocker: the same rows as in Waiting for me, with their replies      │
│                                                                        │
│ WHO HAS IT      one line and the action that fits (states below)       │
│                                                                        │
│ Why it's #3                                              as today      │
│                                                                        │
│ Steps 2/5                                                task only     │
│   [x] Inventory the certs                                              │
│   [ ] Run in staging                  opens to its context             │
│   [-] Email ops                       dropped, shown struck            │
│   + Add a step                                                         │
│                                                                        │
│ Context                               rendered Markdown     [Edit]     │
│                                                                        │
│ need, effort, impact, due, tags, parameters              as today      │
│                                                                        │
│ From: "Certs are a mess", a note                         when made     │
│ Became: 3 tasks, listed with their states                on a note     │
│                                                          closed so     │
│ Questions 3   folded: each with its reply and Change reply             │
│ Runs 4 · $1.20   folded: when, from → to, how it ended, result,        │
│                  links, cost                                           │
│ History              as today; an agent's change is marked as one      │
│                                                                        │
│ ⋯  More: Won't do · Make it a task… (asks first: a task can't go back  │
│    to a note) · Stage: as written, with context, defined (Defined      │
│    only on a task). A stage change saves silently.                     │
└────────────────────────────────────────────────────────────────────────┘
Who has it, by state:
  Yours, open:           "With you."  [Send to an agent ▾]  to <the next state>, or all the way,
                         with an optional line. A defined task has one choice: do it.
  An agent brought it:   "An agent brought this to: with context, 2 h ago."
                         [Send on ▾] [Changes asked…]  Changes asked takes a line saying what's wrong.
  With an agent:         "With an agent since 14:02, going to: defined."  [Take back]
  Stuck:                 shown under Waits for you, with [Retry] [Take back].
  Closed, any reminder, an archived heap: no line. A reminder becomes a task first.
Steps: a tap ticks or unticks; the title is edited in place; Drop is under the step's ⋯.
Context: Edit turns it into a Markdown text field; it saves as it closes, like today's description.
A reminder shows its date where a task shows its steps, and Make it a task is its main action once due.
The runs' total is shown per unit when the runs reported more than one ("$1.20 and 40k tokens").
Confirmations: Send on acts at once, with Undo while no run has started. Take back asks first.
States: loading, saved, conflict (shows the other version, as today), read-only in an archived heap.
Next likely action: reply when something waits; otherwise send on, or tick a step.
```

```
ALL WIDTHS · Agents (W10)                             daily · core
┌────────────────────────────────────────────────────────────────────────┐
│ Agents                                                                 │
│                                                                        │
│ Your decision · 2                                                      │
│   Rotate the certs     Work    note, with context         2 h ago      │
│   Move the blog        Work    made the heap "Blog move"  yesterday    │
│                                                                        │
│ Today                                                                  │
│   14:02  Rotate the certs · Work               reached: with context   │
│          Filled the context from the infra repo's runbook.             │
│   11:40  Fix the footer · Site                 reached: done           │
│          Footer links fixed.   ↗ pull request                          │
│ Yesterday                                                              │
│   …                                                                    │
│ Load more                                                              │
└────────────────────────────────────────────────────────────────────────┘
Your decision: items with the person whose latest turn ended as reached, with no turn opened and no
         change of state or stage since; and for a note closed as became in such a turn, one row
         standing for what it became, until any of those is sent on, started or closed. Most important
         first, ordered as Waiting for me is.
History: runs that ended as reached, newest first: the item, its heap, the state reached, the run's
         result in a line or two, and its links. No cost here; it's on the item.
A row opens its item.
States: empty (says no agent has finished anything yet, and shows the command that starts one);
        loading; a failed refresh keeps the rows.
Next likely action: open the first item under Your decision.
```

```
DIALOG, PHONE SHEET · Add (W12)                       constant · core; reminder: weekly · enabler
│ Add to Work ▾                                                          │
│ [Note] [Reminder]                    Note is picked                    │
│ Title                                                                  │
│ Need                                 as today, required                │
│ Date                                 reminder only: exact, required    │
│ context, effort, impact, tags, parameters                as today      │
A reminder has no due field: its date is its deadline.
States: for a reminder, Add stays off until a date is picked. The toast says when it joins the list
        instead of where it landed.
```

```
ALL WIDTHS · Heap settings, General (W13, W14)        sometimes · core
│ Name, link name                                          as today      │
│ Context                              rendered Markdown     [Edit]      │
│ Agents prepare new notes             off / on                          │
│ Your weight for this heap            1                                 │
│ Agents' cost in this heap            $14.20                            │
│ History, Archive heap                                    as today      │
Context, empty: one line saying what belongs there: what the project is, where its repository and docs
         are, where its work is reviewed and what finishing a piece means.
Prepare: an admin's control; everyone else reads the value. One line says what it does.
Weight:  each person's own, with a line saying it's against their other heaps and nobody else sees it.
Cost:    the total of the runs on the heap's items, per unit when there is more than one.
Activity, the heap's and the team's, marks an agent's change the same way an item's history does.
```

```
ALL WIDTHS · The list, reminders to come (W15)        sometimes · enabler
Filters › Show gains "Reminders to come": Hidden (the default) or Shown.
Shown: a group of their own under the list, by date, soonest first. A row has the reminder mark, the
       title and the date, and no position. It opens the item, where the date can be changed.
Search matches reminders to come whether the group is shown or not, and lists them in that group.
No note counts the hidden ones: they aren't part of the list yet.
```

```
ALL WIDTHS · Closed (W16)                             sometimes · enabler
│ Certs are a mess       Became 3 tasks · today by an agent    [Reopen]  │
A third outcome beside Done and Won't do. The title opens the item, which lists what it became.
Reopen works within the week, as for any closed item, and what it became stays.
```

### Wireframes: command-line client

The new commands follow the shape of the ones that exist: a category, a verb, the required input, then
options. Every one takes `--json` and `--quiet`.

**Names.** A step and a question are named by their item's key and a number given once and never reused:
`k3x9a.3` is a step, `k3x9a.q2` a question. Reordering steps doesn't rename them.

**Markdown** comes from a file or from standard input: `--context-file notes.md`, or `--context-file -`.

**A run** is known to the commands by the `UPHEAP_RUN` environment variable, which `work run` sets when it
starts the agent tool. A command with it set writes as an agent, inside that run. Without it the same command
is the person's own.

```
C1 · upheap work run --tool <tool> [--heap <heap>]... [--once] [--workers <N>] [--root <dir>]     constant · core

$ upheap work run --tool claude --heap work --heap house
Working in Work (weight 2) and House (weight 1) with claude. Ctrl-C stops after the current run.
14:02  k3x9a  Rotate the certs    note → with context    reached   2m10s   $0.31
14:05  p2m4q  Paint the fence     note → with context    asked     0m40s   $0.08
14:06  Nothing to do. 1 item waits for you. Asking again in a minute.

--heap    repeatable; with none, the default heap. Heaps agents make from these are included.
--once    do one run, or nothing when there is nothing, and exit.
--workers keep up to N runs going at once (1 to 8, default 1), each on its own item. Above 1, needs --root.
--root    each item runs in <dir>/<team>/<heap>/<key>, kept and reused for its next run; removed once the item
          is closed or can't be found, or 7 days after a run last started or ended in it (swept at start, then hourly).
The agent tool gets: the instructions for working with Upheap, the heap's context, what `item show`
prints, the state to reach, and the person's line.
How a run ended is read off it: it said `work reached` and the check passed; or `work hand-back`; or it left an open
question; otherwise it failed. Cost is what the tool reported, or blank.
Ctrl-C: once, the current run (every current run, with --workers) finishes and the loop stops; twice, the runs are
stopped and count as failed.
Exits:  0 stopped by the person, or --once done; 2 no heap named and no default; 4 not signed in;
        5 server unreachable with --once (without it the loop waits and tries again); 6 the tool isn't
        installed or wouldn't start.
```

```
C2 · upheap work next [--heap <heap>]...                           weekly · enabler

$ upheap work next --heap work --heap house
k3x9a  Rotate the certs   Work   task → done   started, ready to go on

$ upheap work next
Nothing to do. 1 item waits for you.

It takes nothing: no claim, no run. --quiet prints the key, or nothing. Exit 0 either way.
```

```
C3 · upheap item show <item>, upheap heap show [<heap>]            constant · core

$ upheap item show k3x9a
k3x9a  Rotate the certs
task · defined · in progress · with an agent                      Work #3
need expected · effort M · due in a few weeks
Turn: to done, all the way. "Use the new CA."
From: n8c2d "Certs are a mess" (note, became)

Context
  <the Markdown, as written>

Steps 2/4
  [x] k3x9a.1  Inventory the certs
  [x] k3x9a.2  Write the script
  [ ] k3x9a.3  Run in staging
        <its context>
  [-] k3x9a.4  Email ops                                   dropped

Questions
  k3x9a.q1  Question · closed · changed since your last run
            "Which CA?"  →  Internal. "Use the 2026 root."
              <its details, when it has them>
  k3x9a.q2  Review, round 1 · changes asked
            https://example.com/pull/12  →  "See the comments there."
  k3x9a.q3  Done proposal · open
            "Merged; nothing left"

Runs 3 · $1.20

$ upheap heap show work
Work · Acme · agents prepare new notes: on · your weight: 2

Context
  <the Markdown, as written>
```

```
C4 to C11 · What an agent writes                                   constant · core

upheap item edit k3x9a --context-file context.md
upheap item to-task k3x9a                                  a note or a reminder, in place
upheap step add k3x9a "Run in staging" [--context-file f] [--after k3x9a.2]
upheap step tick k3x9a.3
upheap step drop k3x9a.4
upheap step edit k3x9a.3 [--title "..."] [--context-file f]
upheap item start k3x9a                                    as today

upheap question ask k3x9a "Which CA?" [--details-file f] [--choice Internal --choice "Let's Encrypt"]
                                      [--many] [--step k3x9a.3]
upheap question request k3x9a "Put a staging token in the vault." [--details-file f] [--step k3x9a.3]
upheap question review k3x9a https://example.com/pull/12 "The script and the runbook" [--details-file f]
                                      [--piece "the script"]

upheap item add "Write the script" --task --from n8c2d --need expected --age keep [--blocked-by k3x9a]
upheap item to-step n8c2d --task k3x9a                     closes the note as became
upheap heap add "Blog move" --from n8c2d --context-file context.md
upheap item add "Pick a host" --heap blog-move --from n8c2d --need expected --age new
upheap item became n8c2d                                   after making things from it

upheap question propose-done k3x9a "Merged; nothing left" [--details-file f]
                                                           when steps and reviews are done
upheap work reached --result "Rotation script merged." [--link https://example.com/pull/12]...
upheap work hand-back --reason "Already fixed in #123; nothing left to prepare."

--from     what the new thing was made from. With it, --need and --age are required: nothing is
           inherited silently. --age keep counts from the note's creation, --age new from now.
--piece    names the piece of work a review is for. Without it the link is the name, so asking again
           with the same link is that piece's next round. It's needed only when a piece moves to a new
           link.
--many     the person may pick any number of the choices; without it, one.
The quoted text is the short version, one line. --details-file is the long one, in Markdown: `work run`'s
instructions tell the agent to sum up there what the person needs from the item's context and the heap's.
Each prints one line saying what changed, and --quiet prints the key of what was made.
`item became` is refused while nothing was made from the item.
`work reached` and `work hand-back` take no item: the run knows it.
```

```
C12 · What a run is refused                                        weekly · core

Each is exit 3 with the server's message and code, and a hint saying what to do next.

  The item changed since it was read          "Read it again: upheap item show k3x9a"
  The turn was taken back, or the claim lapsed  "This run is over. Stop."
  A write outside the run's own item          names the item the run holds
  A need above the note's                     names the highest it may give
  Closing a task, in a run                    "Propose closing: upheap question propose-done k3x9a ..."
  A proposal without what Review §2 asks      names everything missing: open steps, an open question or
                                              review, a piece whose latest round isn't approved
  Reached, but the item doesn't fit the stage names the condition it misses, and the run can fix it
                                              and say reached again
  Won't do, answering a question, changing    "Only the person does that."
  a heap it didn't just make
```

## 6. Stages

Agreed. Each piece is a task in Upheap, with when it's done and what it waits for, and the later stage's pieces
are notes there until they're defined. Ideas and thoughts parked for a later step are notes there too.
