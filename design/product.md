# Product

What Upheap does and why: the core commitments the product makes, whether or not the code has caught up with them.
A commitment is a promise to a user, stated as an outcome, that only a product change could break. How it's built
is in [architecture.md](architecture.md), and how the web app behaves, screen by screen, in [ux.md](ux.md).

## Pitch

Upheap is a ranked todo list for a person, or a team, with more work than can ever be done. Everything goes in:
ideas, tasks, things to remember, each with how much it's needed and a rough size, impact and due date. Upheap ranks
it all by how much it's needed, how long it has waited, how close its deadline is and how much it's worth, and keeps
that ranking current as time passes. The top is what matters now, further down is what comes next, and the bottom
may never happen.

Part of that work can be handed to AI agents. A heap carries its project's context, the ranking decides what an
agent picks up, and an agent that needs the person leaves a question on the item and moves on. The project's own
files keep what it is, not how far along it is.

Upheap waits for the person. It shows what's important and what needs them when they come and look, and it doesn't
interrupt.

It answers questions like:

- What's important right now, and what should I work on?
- What comes next? (Scroll down: the list stays in priority order.)
- Why is this item here, and why isn't it higher?
- What is waiting for me: which questions and requests can only I deal with?
- What should an agent pick up next? (By rules, the same way every time, without asking a model.)

What it is not:

- **Not a plan that gets finished.** A heap holds more than can be done. It orders the work and doesn't promise all
  of it.
- **Not something that interrupts.** Information appears when the person asks for it.
- **Not a model host.** It runs no models. It drives an agent command-line tool the person already has, on their own
  machine.
- **Not a wiki.** A heap's context is short and points at where the knowledge lives.
- **Not a team project tracker.** No assignees, sprints, boards or reports.
- **Not a chat.** A question belongs to an item and stays with it.

## Distribution

1. Upheap is free and open source.
2. Teams self-host it, on Postgres, MySQL or SQLite. There is no hosted or paid tier.
3. One person can also use it without hosting anything: a static, single-user build runs entirely in the browser and
   keeps the data there ([architecture.md](architecture.md#local-build)). It has no accounts and no teammates, all
   its heaps belong to one team, and it is not a hosted tier, since no data leaves the browser.
4. The server exposes an API that any client can use. There are two clients, the web app and the `upheap`
   command-line client, and the API assumes neither.
5. Working with agents needs a server, which one person can run for themselves alone. The browser-only build has
   no agents.

## Accounts and sign-in

1. The app runs self-contained, with its own accounts: an email address, a username and a password.
2. Registration is open by default. The operator can close it, and then a new account is made only through a
   team's invite link (Invite links §4).
3. Users can also sign in through a third-party identity provider, for example Okta. Signing in through one for the
   first time creates the user record.
4. Non-web clients such as the CLI sign in through the browser: the client opens the app's sign-in page, the user
   signs in there, and the backend hands a token back to the client (OAuth authorization code flow with PKCE). The
   client never handles the user's password.
5. A signed-in user can change their password by giving the current one. That ends every other sign-in of theirs,
   in browsers and in command-line clients.
6. A forgotten password is reset through a link the operator makes with a command on the server. It works once and
   for one hour, and setting the new password ends all the user's sign-ins. The server sends no email.
7. A password change and a reset are in the audit log, without the password, and only the user sees them.

## Teams

1. Users form teams. A team has one or more admins.
2. Any user can create a team and becomes its admin.
3. Every new user gets a personal team, shown as "\<their name\>'s team", with a first heap called "Personal". The personal
   team is theirs alone: it holds as many heaps as they like, but it can't be shared. Anything shared lives in
   another team.
4. A team owns its heaps, and can have more than one.
5. A heap belongs to exactly one team, and only that team's members can access it. Everyone in the team sees the
   same items and ranking in its heaps, and can add and edit items.
6. Users join a team in two ways: an admin picks an existing user, who is added directly, or the person comes
   through one of the team's invite links (Invite links).
7. A team admin can make any other member an admin, remove admin rights from another admin, and remove another admin
   from the team.
8. A team always has at least one admin. The last admin can't leave, be demoted or be removed until another member
   has been made admin.

## Invite links

1. A team admin can make an invite link for a team they administer. A personal team has none.
2. A link works for one week, for any number of people. It can't be extended; the admin makes a new one.
3. Any admin of the team can revoke a link, and it stops working at once. People who already joined through it
   stay.
4. A person joins through a working link as a member, never as an admin: a signed-in user by confirming, and a
   person without an account by creating one, whether registration is open or closed.
5. A person an admin removed from the team can't come back through a link made before the removal.
6. Links are never deleted. Making one, revoking one and each join through one are in the audit log.

## Heaps

1. A heap is a collection of items with its own ranking, tags and custom parameters.
2. Every heap is owned by a team (Teams §4–5). A personal heap is one in the user's personal team.
3. A person can belong to several teams and heaps, and switches between them.
4. Items have no assignee or owner. A shared heap has one ranking for the whole team, and "my plate" means the heaps I
   belong to. This is deliberate.
5. A heap has a context: text that says what the project is, points at where its knowledge lives, such as a
   repository or a wiki, and says where its work is reviewed and what finishing a piece means. Upheap stores the
   pointers and doesn't fetch what they point at.

## Items

1. An item is a finishable piece of work with a title and an optional description.
2. An item can have an effort size and an impact, each on the scale S, M, L, XL.
   - Every item says how much it's **needed**, picked when it's added: what happens if it never gets done. Need is
     separate from impact, which is what doing the item gains; need is what skipping it costs, so an item nobody
     would miss can be valuable, and an urgent one can be small. The steps, from least to most needed:
     - **Idea:** kept so it isn't forgotten. Nobody has decided it's wanted, and nothing is lost if it never happens.
     - **Nice to have:** wanted, but nobody is waiting for it. Skipping it only misses a gain.
     - **Expected:** someone is counting on it. Skipping it lets them down.
     - **Critical:** it has to happen. Skipping it does real damage: an outage, a lost customer, a broken commitment.
   - Each step is told apart from the next by who is affected when the item is skipped: no one, no one yet, someone,
     or the team and the people it serves.
   - Need can be changed but not cleared.
3. An item can have a due time, given either loosely or as an exact deadline date.
   - A loose due time is picked from rough choices ("next week") rather than a date, and reads back in fuzzy terms
     such as "in a few days", "next week", "in a few weeks", "next month" or "in several months", with the term it
     was picked by.
   - A loose due time never becomes overdue. Once its date has passed, it keeps showing the nearest term, for example
     "in a few days".
   - An exact deadline can become overdue.
4. Effort, impact, due time and custom parameters can all be left unset; need can't. An unset value counts as the
   lowest-priority option for that parameter, and the interface shows it as unset.
5. An item can have any number of tags, used to group similar items. A tag has a name and an optional description.
6. An item is always in one state: open, in progress, blocked, done or won't do. Done and won't do are closed
   (terminal) states. Items are never deleted. Work that won't happen is closed as won't do. A note that turned into
   other things is closed too, as became (Notes, tasks and reminders §6).
7. A closed item can be reopened within one week of being closed. After that it can't be reopened.
8. A blocked item can point to another item as the reason it's blocked.

## Notes, tasks and reminders

1. An item is a note, a task or a reminder. All three are ranked together in one list, by the same factors.
2. A note is the item described above: it can be started, blocked and done on its own.
3. A task is a note with a checklist. Each step has a title and its own context, and is ticked when done.
4. A reminder has a date. It is out of the list until that date, and from then on is ranked like any item.
5. An item's description is its context, beside the heap's. Contexts, questions and replies are written in
   Markdown, and the web app shows them rendered.
6. A note can become a task, a step of a task that already exists, several tasks, or a new heap with its own
   context, tasks and notes. A reminder is resolved, or becomes a task. A task gains steps and is done like any
   item.

## Questions

1. A question is attached to an item, and is either a question to answer or a request to do something. An agent or
   a person can attach one. It can offer choices to pick from, and is replied to with the choices picked and a
   comment.
2. An item with an open question waits for the person. An agent never picks it up.
3. The person can see everything that waits for them, across their heaps, in one place.
4. Answering a question, or saying a request is done or can't be done, closes it. When an item's last open
   question closes, the item goes back to whoever asked.

## Working with agents

1. Work moves between stable states: points where an agent counts its part as finished and the item is the
   person's again. They are: a note as written; a note with its context filled in; a defined task, with its
   checklist and context, ready to be done; done. A note too big for a task becomes a defined heap instead.
2. An agent's turn takes one item from one stable state to the next and stops there. An item with an open question
   is in the middle of a turn, and the agent carries on once the person has answered.
3. The person sends an item on by telling the agent to go one stable state further, or all the way to done.
4. A heap can be set so agents prepare what's added without being asked: each new item is taken one stable state
   forward. Preparing never includes doing a defined task; that always needs the person's word.
5. Outside those two cases an agent never picks an item up. At any moment an item is with the person or with an
   agent, and the list shows which.
6. An agent refines and does: it fills in context, turns a note into tasks, asks, adds steps, and does the work,
   ticking the steps as it goes.
7. An agent that makes several tasks says which block which. A blocked task is never given to an agent.
8. Nothing reaches the person unless they look: a question, a finished task and an agent's progress all wait in
   Upheap. The person can see what agents have done across their heaps as one short history of results.

## Review

1. An agent never marks its own work done. It asks for a review: a request on the task with a link to where the
   work is, such as a pull request or a document.
2. The person reviews where the work lives, and replies in Upheap: approved, or changes asked. Their comments stay
   where the work is.
3. Changes asked sends the task back to the agent, which changes the work and asks again. There is no limit on
   rounds.
4. A task can need several reviews, as when one change lands as several pull requests. It is done when the agent
   has nothing left to do on it and every review it asked for is approved.

## The agent loop

1. The command-line client gives the next item for an agent by rules alone: the heaps' ranking, limited to items an
   agent may pick up that aren't blocked and don't wait for the person. The same state gives the same answer, and
   no model is asked.
2. It reads and writes everything an agent needs: both contexts, checklists, questions, and the moves between
   kinds.
3. It runs the loop on the person's machine: finds what's next, starts the agent tool the person named with the
   item and its contexts, and waits when there is nothing or everything waits for the person.
4. The person says which heaps it may work in, and it touches no other. A heap an agent makes out of an item in
   one of those heaps is included.

## Custom parameters

1. Team admins can define custom parameters for a heap. A parameter has a name and a list of options.
2. The order of a parameter's options determines their relative priority within that parameter. Each parameter also
   has a weight controlling its influence relative to the heap's other factors (Priority §1).
3. A custom parameter can be linked to one or more tags. When a user adds that tag to an item, the app suggests the
   linked parameters.
4. Effort and impact apply to every item. They share one weight: they count together as a single value-per-effort
   factor, so a small high-impact item ranks above a large item with the same impact. The exact combination is part
   of the formula.

## Priority

1. Each item has a combined priority score based on need, age, deadline proximity, effort and impact, and custom
   parameters. Each factor has a weight, set per heap by its admins, and the weights apply to everyone in the team.
   The weights are the heap's whole ranking policy; there is no separate policy setting.
2. **Age** counts from the item's creation, including any time spent in progress or blocked. Items are not expected
   to stay in those states long. A reopened item keeps its original creation time. How fast an item ages depends on
   its need: an idea ages slowly, so old ideas don't climb past fresh work someone is waiting for, and a critical
   item ages fastest.
   - **Need** also counts on its own: the more an item is needed, the higher it ranks. A critical item pulls about as
     hard as an exact deadline that has just passed; an idea adds nothing.
3. **Deadline proximity** rises as the due date approaches. Past an exact deadline the score jumps, and keeps
   climbing the longer the item is overdue. A loose due time never becomes overdue (Items §3): past its date, the
   score keeps rising, slowly.
4. **Unset values** count as the lowest-priority option (Items §4).
5. **Blockers** take on the priority of the most important item they block, with a small boost, because finishing
   both takes more effort than finishing one. This carries through chains: if A blocks B and B blocks C, A can
   inherit C's priority. Blocking cycles are not allowed; the app refuses a block that would create one.
6. The score is calculated live when someone views the list. Once a list has been sorted, it doesn't reorder on its
   own during that session. When the user edits an item, that item moves to its new position right away, and the
   list may re-sort as a whole at that point.
7. **Ties** go to the older item: earlier creation comes first.
8. Only the item's position is shown, never the score itself.
9. In the list view, each item shows the biggest contributors to its position. This is how the app answers "why is
   this here?" and "why isn't this higher?"; there is no separate item-by-item comparison.

The factors above are the commitment; the numbers aren't. The formula and its numbers live in the code, versioned
([decisions.md](decisions.md), decision 4).

## Views

### List items

The main view and the answer to "what should I work on?"

1. The app opens on this view, with items ordered by combined priority, highest first.
2. In-progress items stay pinned at the top in a compact form so they aren't forgotten. Blocked items are hidden by
   default. All other open items follow in priority order. Closed items appear only in the Closed items view.
3. The user can configure which non-closed states are pinned, shown, or hidden. Shown blocked items can be mixed in
   with the other items in the current sort order, or listed as a separate group sorted by priority.
4. Each item shows its biggest score contributors (Priority §9).
5. The user can instead sort by any single parameter that has an order (for example deadline, effort, impact, or a
   custom parameter).
6. The user can search items by text and filter by state, tag, and parameter values. Filtering preserves the relative
   priority order of matching items.
7. The user can change an item's state directly in the list. Everything else is edited in the item.

### Closed items

1. Shows done and won't-do items, sorted by when they were closed.
2. An item closed less than a week ago can be reopened from here.

### Add item

The fastest path in the app.

1. The user enters a title, picks how much the item is needed, and optionally adds a description, effort, impact, due
   time and tags. Everything except the title and the need is optional.
2. Typing a tag that doesn't exist yet creates it.

### Edit item

1. The user can change any field of an existing item, add or remove tags and parameters, and move it between states.
   They can move an item to another heap of its team, and it keeps its context, steps, questions and history.

### Manage tags

A supporting view.

1. Any team member can create and rename tags and edit their descriptions. Tags can't be deleted, but a tag can be
   taken off any item.

### Manage parameters

A supporting view, for team admins only.

1. An admin can create and rename custom parameters, edit and reorder their options, link them to tags, and
   archive them (Nothing is deleted §5).

## Nothing is deleted

1. Items, tags and heaps can't be deleted.
2. An item that won't be done is closed with the **won't do** state instead.
3. Tags can be renamed and their descriptions edited, but not deleted. A tag can be taken off an item, which is
   recorded in the audit log like any other item edit.
4. Heaps are archived instead of removed, by an admin. An archived heap is read-only and keeps its audit history,
   and an admin can unarchive it.
5. Custom parameters and their options are archived instead of removed:
   - Archiving an option requires the admin to pick a replacement option. Items in a non-terminal state (open, in
     progress, blocked) switch to the replacement. Items in a terminal state (done, won't do) keep the archived
     option.
   - A whole parameter can be archived only when no non-terminal item uses it. Terminal items keep their archived
     values.

## Roles and permissions

| Action | Member | Team admin |
| --- | :---: | :---: |
| Register and create teams | ✓ | ✓ |
| List own teams and heaps | ✓ | ✓ |
| Leave a team | ✓ | ✓ (not the last admin) |
| Add and edit items | ✓ | ✓ |
| Add and edit tags | ✓ | ✓ |
| Edit a heap's context | ✓ | ✓ |
| Send an item to an agent, take it back, answer a question | ✓ | ✓ |
| Set a heap so agents prepare what's added | | ✓ |
| Rename the team | | ✓ |
| Join a team through a working invite link | ✓ | ✓ |
| Add users to the team and remove them | | ✓ |
| Make, see and revoke the team's invite links | | ✓ |
| Make a member an admin, demote an admin | | ✓ |
| Create, rename, archive and unarchive heaps | | ✓ |
| Configure a heap's ranking: its weights, parameters and options | | ✓ |
| Archive parameters and options | | ✓ |

## Audit log

1. Every action by a user or an admin is recorded in an audit log.
2. Each entry shows who made the change and what changed.
3. Entries can't be edited or deleted, and they are kept forever.
4. Users see audit entries on the page where the action happens, for every kind of action they are allowed to do. For
   example, every member sees item and tag changes on the heap page, and only admins see team membership changes and
   heap configuration changes.
