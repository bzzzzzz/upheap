# Notifications: design

How Upheap reaches a person who isn't looking at it: mail about a reminder on its date, and about what starts
waiting for them where they asked to be told. A short design, drafted whole and reviewed once. Design in progress.
Agreed so far: nothing.

## Constraints

1. Nothing is pushed that the person didn't ask for.
2. It works for one person on a server they run for themselves, with no outside service required.
3. A push says what waits and links to it. Acting on it happens in Upheap.
4. Mail is optional on a server, and goes out over plain SMTP ([product.md](../product.md#email) §1,
   [decisions.md](../decisions.md) §37).
5. The production server does no work between requests; timed work there is an outside scheduler calling an
   endpoint ([decisions.md](../decisions.md) §20).
6. Nothing stores a person's timezone ([decisions.md](../decisions.md) §2).
7. There are no assignees: what waits in a team's heap waits for every member who can see it.

## 1. Value

Upheap waits for the person and doesn't interrupt. This adds the two exceptions, both only because the person asked:

- Be told on the day about a reminder or a deadline I asked to be told about.
- Be told when something starts waiting for me in a place I said matters.

It is not a feed of what changed, a digest of the list, or a way to act from the inbox.

## 2. Golden path

1. The person adds a reminder, "Renew the certificate", for 3 March, and ticks Mail me. Nothing happens until then.
2. On the morning of 3 March a mail arrives: the reminder's title, its heap, and a link.
3. They open the link and deal with it in Upheap.

The second path, with the same mail:

1. The person turns on Mail me what waits for me for the heap their agents work in.
2. At night an agent asks a question on a task there. Within minutes a mail arrives with the question and a link.
3. They open the link and reply in Upheap.

Commitments. The first four are core:

1. **Core.** Upheap mails a person about items only where they asked: for an item, or for a heap.
2. **Core.** An item the person asked about is mailed to them on the morning of its date: a reminder's date, or an
   exact due date.
3. **Core.** In a heap the person asked about, they are mailed when something starts waiting for them.
4. **Core.** A mail says what waits and links to it. Nothing is done from the mail.
5. What merely changed is never mailed: an edit, a comment, a move in the ranking, an agent's progress.
6. On a server without mail nothing is pushed, and nothing offers to.

No new actor. A person asks for themselves only; nobody turns a push on for someone else.

Out: a browser or phone notification, choosing kinds of event, quiet hours.

## 3. Workflows

| Flow | Who | Kind |
| --- | --- | --- |
| Add a reminder and ask to be mailed on its day | person | loop, about weekly |
| Ask to be mailed about an item that exists, or stop | person | loop, now and then |
| Get a mail, open its link, act in Upheap | person | loop |
| Turn a heap's mail on or off | person | setup |
| Stop mail that turned out to be too much: from the mail, find where it was asked for and turn it off | person | recovery |
| Give a server that sleeps between requests a scheduler that calls the tick | operator | setup |
| Mail is down, or the server was: nothing to do, the push is sent late | operator | recovery |

Not core: a page listing everything a person asked to be mailed about, and turning mail off without signing in.

## 4. Behaviours

### Concepts

- **Mail me**, on an item: one person's wish to be mailed about that item. On or off, off at the start. In a team's
  heap each member has their own.
- **Mail me what waits for me**, on a heap: one person's wish to be mailed when something in that heap starts
  waiting for them. On or off, off at the start.
- **Push:** one thing to tell one person about one item, with its reason: its date came, a question or request was
  put on it, its agent's turn is stuck, or its blocker is gone. Pending, then sent or dropped.
- **Tick:** one run of the server's timed work. Sending pending pushes is its first job.

### Rules

1. **Date.** An open item with Mail me on gets a push for that person on the morning of its date: a reminder's
   date, or an exact due date. A loose due time has no day, so it never pushes.
2. **Morning** is nine o'clock on that day by the clock of the place where the date was set. A date is stored as
   the edge of its day there, so the hour is counted from it and no timezone is kept.
3. A date moved before its push moves the push. Moved to a later day after it, it pushes again on the new day. Mail
   me turned on after the morning has passed sends nothing for that date: the item is already in the list.
4. **Waiting.** When something starts waiting for the person on an item (a question or a request, which covers a
   review request and a proposal to close, a stuck turn, a gone blocker), a push is pending for every person who
   can see the item and has Mail me on it or Mail me what waits for me on its heap, except the person who caused
   it.
5. **One push per reason.** A question pushes once. Nothing repeats or nags.
6. **Sent by the tick, together.** The next tick sends every pending push whose time has come. All of one person's
   go in one mail.
7. **Dropped when moot.** A pending push is dropped when its reason is gone before it is sent: the question was
   answered, the item closed, the wish turned off, or the person can no longer see the item.
8. **Failure.** A send that fails stays pending and is tried again by later ticks, for a day. Then it is dropped
   and logged. The item waits in Upheap either way.
9. **Late.** A tick that comes late, after the server was down or asleep, sends what is still pending. A date's
   mail names the date.
10. **Once.** Two ticks at the same moment, on one instance or two, send each push once.
11. **Two sources, one tick.** The server runs the tick itself, every minute, unless the operator turns that off.
    An endpoint guarded by a secret runs the same tick for a scheduler to call. A server that sleeps between
    requests uses the second.
12. **No mail, no pushes.** On a server without mail the wishes can't be set, and nothing is recorded as pending.
13. **The mail** is plain text: for each push its reason, the item's title, its heap, a question's short text, and
    the link. It ends by saying why the person got it and where to turn it off.

### Open for now

- An agent acts as the person, so it can turn Mail me on. Close it if agents start doing so unasked.
- No limit on mails in a day. A busy heap with agents could send many; that would call for a least gap between
  mails.
- A push is sent even while the person has Upheap open.
- A person who travels gets the mail at nine by the clock of where they set the date.

### Scenarios

1. A reminder set in Berlin for 3 March, Mail me on. The first tick after 09:00 Berlin time on 3 March sends it.
   The server was down until 14:00: it goes at 14:00 and names 3 March (rule 9).
2. A heap with mail on. An agent puts three questions on two items inside a minute, and the person answers one
   before the tick. One mail, two questions (rules 6, 7).
3. A team's heap, mail on for A and B. B puts a request on an item. A is mailed, B isn't. A replies: no mail to B,
   since a reply is a change (rule 4, commitment 5).
4. Mail is down for two hours on a reminder's morning: sent when it is back. Down for two days: dropped and logged,
   and the reminder is in the list as it would have been (rule 8).
5. An exact due date with Mail me on, moved from the 5th to the 10th on the 4th: one mail, on the 10th. The item
   is done on the 9th: none (rules 3, 7).
6. A reminder added at noon for today, Mail me ticked: no mail, and it is in the list now (rule 3).

### Choices

- **Mail only.** A browser notification needs the browser makers' push services and a service worker; mail is
  already there, needs nothing outside the server, and reaches a phone.
- **Every push goes through the tick,** also the ones known at once. Sending inside the request would be faster by
  a minute, and would give two ways to send, no retry, no gathering into one mail, and a request that waits on
  SMTP.
- **The server's own timer and an endpoint.** The endpoint alone would make a self-run server set up cron; the
  timer alone doesn't run on a server that sleeps. Decision 20's consequence stands as written.
- **A wish of the person's, not the Critical need.** The need says skipping the item does damage, and ranks it
  for everyone. Whether to interrupt is each person's call.
- **An item and a heap, not kinds of event.** Two switches cover both paths. Kinds can come if a heap's mail
  proves too coarse.
- **Nine o'clock from the stored date,** not a stored timezone or a chosen hour: decision 2 stays.

No choice here is costly enough to reverse to need an entry in decisions.md.

## 5. Interface

| Entry | Used by | Frequency | Necessity |
| --- | --- | --- | --- |
| Add dialog: Mail me, for a reminder | adding a reminder | Weekly | Core: the reminder is where the push is asked for |
| Item: Mail me | asking later, stopping | Sometimes | Core: the only way to stop, and to ask for a due date |
| Heap settings: Mail me what waits for me | turning a heap on or off | Once | Core: the second path starts here |
| The mail | getting it | Daily at most | Core |
| CLI: the same three switches | adding from a terminal or a script | Weekly | Enabler: reminders are often added there |
| Operator: turning the timer off, the tick's endpoint and secret | setup of a sleeping server | Once | Core there, unused elsewhere |
| A page of everything I asked to be mailed about | recovery | Sometimes | Nice to have: left out |

On a server without mail none of the switches show, as the Email card doesn't.

```
ADD DIALOG · Reminder                     weekly · core
┌──────────────────────────────────────┐
│ [Note | Reminder]                    │
│ title                                │
│ Date: <exact date>                   │
│ [ ] Mail me on the day               │  only for Reminder, unticked
│ need, effort, impact, tags …         │
│                       [Cancel] [Add] │
└──────────────────────────────────────┘
States: hidden without mail. A date of today or earlier: the box is off, with a line saying it's in the list now.
```

```
ITEM · fields                             sometimes · core
│ Date / Due: 3 March                  │
│ Mail me: [off | on]                  │  a line under it says when: "on the day", "when something
│ Tags …                               │  waits for you", or both
States: hidden without mail, on a closed item, and for someone who can't see the heap's items.
```

```
HEAP SETTINGS · for me                    once · core
│ Mail me what waits for me: [off | on]│  beside the person's own priority for the heap
│ questions, requests, stuck agents, gone blockers
```

```
THE MAIL                                  core
Subject: Renew the certificate            one push: its title; several: "3 things wait for you in Upheap"

Reminder for 3 March: Renew the certificate
  Home · https://upheap.example.com/…

Question: Which region should the bucket be in?
  Move the backups · Ops · https://upheap.example.com/…

You get this because you asked to be mailed about these items or their heaps.
Turn it off on the item, or in the heap's settings.
```

```
CLI
upheap item add "Renew the certificate" --reminder 2027-03-03 --mail-me
upheap item edit <item> --mail-me | --no-mail-me
upheap heap mail <heap> on | off
```

The item is a layout used many times a session, so its switch gets an approved mockup before it is built.

## 6. Stages

**Stage 1: a reminder's mail, end to end.** One pull request each, in order.

1. **The tick.** The timed run in the server, its own timer, the endpoint and its secret, the settings, and the
   operator's pages. Done when a tick runs on the timer and through the endpoint, two at once do a job once, and
   the operator's docs say how to set up each kind of server. Covers rules 10 and 11.
2. **Mail me on an item, and the date push.** The wish, the pending push, the mail, the tick's job, in the API.
   Done when a reminder with Mail me on is mailed on its morning, and scenarios 1, 4, 5 and 6 pass as tests. Covers
   commitments 1, 2, 4 and 6, and rules 1 to 3, 5 to 9, 12 and 13.
3. **The web app:** the Add dialog's box and the item's switch, after the mockup. Done when the golden path works
   in a browser.
4. **The CLI:** the item's two flags, and the user's pages. Done when the golden path works from a terminal.

**Stage 2:** the heap's switch and the waiting pushes (commitment 3, rule 4, scenarios 2 and 3), then heap
settings in the web app, then the CLI's heap command.

When this is agreed, product.md changes in four places: the pitch's "doesn't interrupt" and "Not something that
interrupts", Email §2, Working with agents §8, and a line in Notes, tasks and reminders. Each gains the exception:
unless the person asked to be mailed.

## Left out

Not part of this design.

- A browser or phone notification as a second channel.
- Choosing which kinds of event a heap mails about.
- An hour of the person's choosing, and quiet hours.
- A page on Account listing every item and heap with mail on.
- A default that ticks Mail me on every new reminder.
- Other timed work for the tick, such as clearing expired sign-ins and links.
