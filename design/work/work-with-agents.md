# Work with agents: design

Upheap as the place where a person's projects keep their ideas, tasks and progress, worked on together with AI
agents. It replaces the pitch in [product.md](../product.md). In progress, by the steps of the maintainer's design
method. Step agreed: none.

## Constraints

1. One person working alone comes first. Sharing a heap with other people keeps working as it does today and gets
   no new design here. (Owner.)
2. Agents use Upheap through the command-line client only. (Owner.)
3. Agents are set up outside Upheap. Upheap decides what's next and calls an agent; it isn't one. (Owner.)
4. What's built stays: the ranking and its factors, need, states, tags, the audit log, nothing deleted, the stack.
   (Repository.)

## 1. Value

### Who it's for and the problem

Upheap is for a person who runs several projects and hands part of the work to AI agents. Today the ideas, plans
and progress of such a project sit in text files inside it. Every agent and every branch edits the same files, so
parallel work collides, and the person can't see or steer any of it away from their desk. An agent that needs an
answer has nowhere to ask, and the person has no single place that says what is waiting for them.

Upheap holds that moving part. A heap is a project: its context, and everything to do in it, ranked. Ideas go in
as notes. A note grows into a task, or into a project of its own. Agents pick up what is marked for them, do it or
come back with questions, and the person answers from wherever they are. The project's own files keep only what it
is, not how far along it is.

### The questions it answers

1. What should I work on now? (As today.)
2. What is waiting for me: which questions and requests only I can deal with?
3. What should the agent pick up next? (Answered the same way every time, without asking a model.)
4. Where has this idea got to: still a note, a task under way, or a project?
5. Why is this here, and why isn't it higher? (As today.)

### What it is not

1. **Not an agent runner.** It doesn't host models, hold prompts or manage their tools. It hands an agent an item
   with its context and takes back the result.
2. **Not a wiki or a document store.** A heap's context is short and points at where the knowledge lives: a
   repository, a wiki, a drive.
3. **Not a team project tracker.** No assignees, sprints, boards or reports. One ranked list per heap, as today.
4. **Not a chat.** A question is attached to an item, is answered once, and stays with the item.
5. **Not a calendar.** A reminder comes into the list on its date. It doesn't notify.

### The shape of the offering

Free, open source and self-hosted, as today. The web app is for the person, phone included. The command-line
client is for agents and for scripts, and can do everything an agent needs.

## 2. Golden path

### The path

1. I make a heap for a project and write its context: what it is and where its repository and docs are.
2. An idea comes up. I add a note, with how much it's needed, in a few seconds. It's ranked with everything else.
3. I mark the note for the agent.
4. `upheap work` runs on my machine. It asks Upheap what's next for an agent, gets the same answer any time it
   asks, and calls the agent only when there is something, with the heap's context and the item's.
5. The agent reads the note and turns it into a task with a checklist. It isn't sure about one thing, so it
   attaches a question. The task now waits for me.
6. On my phone I see one thing waiting for me, answer it, and the task goes back to the agent.
7. The agent works through the checklist, ticking each step and adding steps it finds on the way. One step needs
   me (a credential, a purchase), so it attaches a request. I do it and say so.
8. The task is done. The repository got the code and nothing about progress. I can see what happened and when.

Two side paths: a note too big for a task becomes a new heap with its own context, notes and tasks. And a
reminder ("renew the certificate in March") stays out of the list until its date, then ranks with the rest, and
is resolved or turned into a task.

### Requirements, in outline

Not yet numbered commitments. Those follow once this reads right.

**Heaps**

- A heap has a context: free text that can point at repositories, wikis and anything else the person and their
  agents can reach.

**Items**

- An item is a note, a task or a reminder. All three are ranked together in one list.
- Every item has its own context, beside the heap's.
- A **note** is today's item.
- A **task** is a note with a checklist. Each step of the checklist has its own context and is ticked when done.
- A **reminder** has a date. It is hidden until then, and from then on is ranked like any item.

**Questions**

- A question is attached to an item. It is either a question to answer or a request to do something.
- An item with an open question waits for the person, and the person can list everything that waits for them,
  across heaps.
- Answering sends the item back to whoever asked.

**Handing work to an agent**

- The person marks an item for agents. Nothing unmarked is ever picked up by one.
- At any moment an item is with the person or with an agent, and the list shows which.

**What can become what**

- Note: gains questions; becomes a task; becomes a new heap with tasks and notes.
- Task: gains questions; gains steps; is done.
- Reminder: is resolved; becomes a task, and from there behaves as one.
- A person can make every one of these moves by hand too.

**Command-line client**

- It gives the next item for an agent, by rules alone: the same state gives the same answer.
- It reads and writes everything an agent needs: contexts, checklists, questions, moves between kinds.
- `upheap work` runs the loop: find what's next, call the agent, wait when there is nothing or when everything
  waits for the person.

**People**

- Everything above works for one person with no teammates.
- In a shared heap any member can answer a question or mark an item. Nothing more is designed for teams yet.

### Out

See Ideas.

## 3. Workflows

## 4. Behaviours

## 5. Interface

## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- Whether "marked for agents" is a tag or a field of its own.
- What an agent's actions look like in the audit log, and which account it signs in as.
- Whether a checklist step can hold a question of its own.
- How a note that became a heap points at it, and what state the note ends in.
- Two agents asking for "next" at once must not get the same item.

## Ideas

Outside the scope. Not commitments.

- Notifications when something starts waiting for the person.
- Several kinds of agent, with items marked for one of them.
- A person in a shared heap picking who answers a question.
