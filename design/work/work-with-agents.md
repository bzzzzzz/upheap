# Work with agents

Upheap as a ranked todo list that also carries a project's context and hands part of the work to AI agents:
notes, tasks with steps and reminders, questions that wait for the person, and an agent loop run from the
command line. It serves Product Notes, tasks and reminders, Questions, Working with agents, Review, The agent
loop, Heaps §5 and Distribution §5, where Product is [product.md](../product.md).

The design is agreed and is in [work-with-agents-design.md](work-with-agents-design.md).

## Progress

Two stages, a pull request per piece. Commitments, rules, scenarios and interface entries are cited as the
[design](work-with-agents-design.md) numbers them: commitments by their heading under Commitments, rules and
scenarios as in Behaviours, W and C entries as in the Interface inventory.

### Stage 1: the golden path, thin

Every step of the golden path works on the real stack, for one person, in one heap. Each piece is one pull
request and leaves `main` working in both builds, with its migration, its API and its tests. Pieces are in
order.

- [x] 1. **Spike: an agent tool run without a person.** Start Claude's command-line tool with a prompt, wait for it
   to exit, read its result and its cost. Throwaway, outside the repository.
   Done: the answer is written under Unproven mechanisms, and pieces 9 and 10 are adjusted if it's no.
- [x] 2. **Kinds.** An item has a kind and a stage; a note becomes a task in place. `item to-task`, `item show` and
   the web item show them; the person sets the stage by hand; icons mark kind and stage on the row and in the item.
   Done: a note made a task keeps its key, age and history. Covers Items §1, §3, §6.
- [ ] 3. **Steps.** A task's steps: add, tick, drop, edit, in the API, `step` commands and the web item, with the
   count on the row.
   Done: a task can be worked through by hand from the web app and from the command line. Covers Items §4,
   Review §7.
- [ ] 4. **Contexts.** A heap gets a context, edited in its settings and read with `heap show`. An item's
   description is its context, written from a file with `item edit --context-file`. Shown as written; rendering
   is stage 2.
   Done: a heap's context and an item's can be read and written from both clients. Covers Heaps §1, §2,
   Items §2 without rendering.
- [ ] 5. **Questions.** A question with its short version, details and choices, of kind question or request, asked
   with `question ask` and `question request`, shown and replied to in the web item. An item with an open
   question waits for the person.
   Done: a question asked from the command line is answered in the web app and reads back answered in
   `item show`. Covers Questions §1, §2, §4, §5, §6.
- [ ] 6. **Waiting for me.** The page across heaps with replies in the row, its place in the navigation with the
   count, the new phone tabs, and "waits for you" on list rows.
   Done: on a phone, an open question is found and answered without opening an item. Covers Questions §3,
   Waiting §1.
- [ ] 7. **Turns.** The go-ahead with its reach and line, the heap's prepare setting, who has an item, take back,
   and what the person's own changes do to a turn. In the web item and on list rows.
   Done: an item can be sent on and taken back, a new note in a heap that prepares gets a turn, and nothing
   else does. Covers Stable states §1 to §4, Handing work §2, §3, §4, §7.
- [ ] 8. **The queue and runs.** `work next`; the claim; a run recorded with how it ended; writes made inside a run
   and what they're refused; `work reached` and its check; failed runs, and a turn that gets stuck and shows
   in Waiting for me.
   Done: a script that plays the agent takes a note to "with context" through the command line alone, and the
   scenarios 4, 5, 6 and 12 pass as tests. Covers Command-line client §1, §2, Handing work §8, Cost §1.
- [ ] 9. **`work run`.** The loop: asks for next, starts the agent tool with the instructions and the brief, records
   the cost, waits when there's nothing. With a stand-in agent tool for the tests.
   Done: with `work run` going, a note added in the web app gets its context and a question, and carries on
   after the answer. Covers Command-line client §3 to §6, Waiting §3, Handing work §5.
- [ ] 10. **Splitting.** `item add --from` with need, age and what blocks what; `item became`; origins shown as From
    and Became in the item; the became state in Closed; a blocked item reopening when its blocker is done.
    Done: a note sent on becomes two tasks, the second blocked by the first, and freed when the first is done.
    Covers What can become what §1 (tasks), §2, Handing work §6, Where things came from §1.
- [ ] 11. **Review.** `question review` with rounds, the rule for an agent's done, and the review reply in the web
    app.
    Done: an agent can't close a task without an approval, and scenario 7 passes as a test. Covers Review §1
    to §6.
- [ ] 12. **All the way.** A turn with that reach opens the next one, and the tasks made from the item get it.
    Done: the second task of piece 10 runs from defined to its review without a go-ahead between. Covers
    Handing work §1.
- [ ] 13. **Runs on the item.** The item's runs with result, links and cost, and the total, in the web app and in
    `item show`.
    Done: after the golden path, each run and what it cost can be read on the item. Covers Cost §2 for an
    item.
- [ ] 14. **The golden path as a check.** One end-to-end test walks all ten steps with the stand-in agent tool, in
    CI, and the README says how to start `work run`.
    Done: the test is required to merge. Covers One person and teams §1.

Left out of stage 1 on purpose: reminders, more than one heap, Markdown rendering, the Agents page, a note
becoming a step or a heap, changing a reply, "changes asked" at a stable state, and the boost for started work.
Until the Agents page exists, a prepared note is found by its mark in the list.

### Stage 2: the rest of the first set

1. **The Agents page:** the history of results and Your decision (W10).
2. **Markdown:** contexts, details and comments rendered, and edited as Markdown (W4).
3. **Putting it right:** changing a reply, "changes asked" at a stable state, closed questions on the item,
   reopening an item closed as became (W7, W16).
4. **Reminders:** the kind and its date, Add, reminders to come and search, a due reminder made a task (W12,
   W15, W17).
5. **Several heaps:** heap weights, the queue across heaps, the boost for started work, and the heap's total
   cost (W14, W22).
6. **The other moves:** a note becoming a step, and a new heap that joins the queue (C10, C11).
7. **Told apart:** an agent's changes marked in item history and Activity (W21).
8. **The local build:** steps, kinds, reminders and Markdown checked there, with everything about agents
   absent.

## Open questions

None yet.
