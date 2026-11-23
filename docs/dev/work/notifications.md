# Notifications: design

How Upheap reaches a person who isn't looking at it: a notification about a reminder on its date, and about what
starts waiting for them on an item they marked. Mail is the first channel. The design is agreed. The promises it
makes to a user are in [product.md](../product.md), under Notifications.

## Constraints

1. Nothing is sent that the person didn't ask for.
2. It works for one person on a server they run for themselves, with no outside service required.
3. A notification says what waits and links to it. Acting on it happens in Upheap.
4. Mail is optional on a server, and goes out over plain SMTP ([product.md](../product.md#email) §1,
   [decisions.md](../decisions.md) §37).
5. The production server does no work between requests; timed work there is an outside scheduler calling an
   endpoint ([decisions.md](../decisions.md) §20).
6. Nothing stores a person's timezone ([decisions.md](../decisions.md) §2).
7. There are no assignees: what waits in a team's heap waits for every member who can see it.

## 1. Value

Upheap waits for the person and doesn't interrupt. This adds the two exceptions, both only because the person asked:

- Be told on the day about a reminder I asked to be told about.
- Be told when something starts waiting for me on an item I said matters.

It is not a feed of what changed, a digest of the list, or a way to act from the inbox.

## 2. Golden path

1. The person adds a reminder, "Renew the certificate", for 3 March, and ticks Notify me. Nothing happens until
   then.
2. On the morning of 3 March a mail arrives: the reminder's title, its heap, and a link.
3. They open the link and deal with it in Upheap.

The second path, with the same mail:

1. The person sends a task to an agent and turns on Notify me on it.
2. At night the agent asks a question on the task. Within minutes a mail arrives with the question and a link.
3. They open the link and reply in Upheap.

Commitments. The four core ones are in [product.md](../product.md), under Notifications: a person is notified
about an item only if they asked for that item, a reminder notifies on the morning of its date, a marked item
notifies when something starts waiting for the person, and a notification says what waits and links to it. The
rest:

1. What merely changed is never notified: an edit, a comment, a move in the ranking, an agent's progress.
2. On a server with no channel set up nothing is sent, and nothing offers to.
3. The person asks to be notified, not to be mailed. Which channels carry it is the server's, so one added later
   needs nothing asked again.

No new actor. A person asks for themselves only; nobody turns a notification on for someone else.

## 3. Workflows

| Flow | Who | Kind |
| --- | --- | --- |
| Add a reminder and ask to be notified on its day | person | loop, about weekly |
| Ask to be notified about an item that exists, or stop | person | loop, now and then |
| Get a notification, open its link, act in Upheap | person | loop |
| Stop one that turned out to be too much: follow its link to the item and turn it off | person | recovery |
| Give a server that sleeps between requests a scheduler that calls the tick | operator | setup |
| Mail is down, or the server was: nothing to do, the notification is sent late | operator | recovery |

Not core: a page listing every item a person asked about, and turning one off without signing in.

## 4. Behaviours

### Concepts

- **Notify me**, on an item: one person's wish to be notified about that item. On or off, off at the start. In a
  team's heap each member has their own.
- **Notification:** one thing to tell one person about one item, with its reason: its date came, a question or
  request was put on it, its agent's turn is stuck, or its blocker is gone. Pending, then sent or dropped.
- **Channel:** a way a notification reaches the person. Mail is the only one. A notification goes out on every
  channel the server has.
- **Tick:** one run of the server's timed work. Sending pending notifications is its first job.

### Rules

1. **Date.** An open reminder with Notify me on gets a notification for that person on the morning of its date. A
   due date never notifies; it moves the item up the ranking, as before.
2. **Morning** is nine o'clock on that day by the clock of the place where the date was set. A reminder's date is
   stored as the start of its day there, so the hour is counted from it and no timezone is kept.
3. A date moved before its notification moves the notification. Moved to a later day after it, it notifies again
   on the new day. Notify me turned on after the morning has passed sends nothing for that date: the reminder is
   already in the list.
4. **Waiting.** When something starts waiting for the person on an item (a question or a request, which covers a
   review request and a proposal to close, a stuck turn, a gone blocker), a notification is pending for every
   person who can see the item and has Notify me on it, except the person who caused it.
5. **One per reason.** A question notifies once. Nothing repeats or nags.
6. **Sent by the tick, together.** The next tick sends every pending notification whose time has come. All of one
   person's go in one mail.
7. **Dropped when moot.** A pending notification is dropped when its reason is gone before it is sent: the
   question was answered, the item closed, Notify me turned off, or the person can no longer see the item.
8. **Failure.** A send that fails stays pending and is tried again by later ticks, for a day. Then it is dropped
   and logged. The item waits in Upheap either way.
9. **Late.** A tick that comes late, after the server was down or asleep, sends what is still pending. A date's
   notification names the date.
10. **Once.** Two ticks at the same moment, on one instance or two, send each notification once.
11. **Two sources, one tick.** The server runs the tick itself, every minute, unless the operator turns that off.
    An endpoint guarded by a secret runs the same tick for a scheduler to call. A server that sleeps between
    requests uses the second.
12. **No channel, no notifications.** On a server without mail Notify me can't be set, and nothing is recorded as
    pending.
13. **The mail** is plain text: for each notification its reason, the item's title, its heap, a question's short
    text, and the link. It ends by saying why the person got it and where to turn it off.

### Open for now

- An agent acts as the person, so it can turn Notify me on. Close it if agents start doing so unasked.
- No limit on notifications in a day. An item an agent works on could send many; that would call for a least gap
  between mails.
- A notification is sent even while the person has Upheap open.
- A person who travels gets it at nine by the clock of where they set the date.

### Scenarios

1. A reminder set in Berlin for 3 March, Notify me on. The first tick after 09:00 Berlin time on 3 March sends it.
   The server was down until 14:00: it goes at 14:00 and names 3 March (rule 9).
2. Two tasks with Notify me on. An agent puts three questions on them inside a minute, and the person answers one
   before the tick. One mail, two questions (rules 6, 7).
3. A team's heap, an item with Notify me on for A and B. B puts a request on it. A is notified, B isn't. A
   replies: nothing to B, since a reply is a change (rule 4, commitment 1).
4. Mail is down for two hours on a reminder's morning: sent when it is back. Down for two days: dropped and logged,
   and the reminder is in the list as it would have been (rule 8).
5. A reminder with Notify me on, moved from the 5th to the 10th on the 4th: one mail, on the 10th. It is resolved
   on the 9th: none (rules 3, 7).
6. A reminder added at noon for today, Notify me ticked: nothing sent, and it is in the list now (rule 3).
7. An agent asks a question on a task nobody marked: nothing is sent, and it shows in Waiting for me (rule 4).

### Choices

- **Mail first, behind a channel.** A browser notification needs the browser makers' push services and a service
  worker; mail is already there, needs nothing outside the server, and reaches a phone. The switch says Notify me
  so another channel can join without a second switch.
- **Every notification goes through the tick,** also the ones known at once. Sending inside the request would be
  faster by a minute, and would give two ways to send, no retry, no gathering into one mail, and a request that
  waits on SMTP.
- **The server's own timer and an endpoint.** The endpoint alone would make a self-run server set up cron; the
  timer alone doesn't run on a server that sleeps. Decision 20's consequence stands as written.
- **A wish of the person's, not the Critical need.** The need says skipping the item does damage, and ranks it
  for everyone. Whether to interrupt is each person's call.
- **The item only.** A switch on a heap, or a choice of kinds of event, would catch what an agent asks on an item
  nobody marked, and would send far more. Marking is one item at a time, so each notification was asked for.
- **Reminders, not due dates.** A reminder exists to be remembered on its day; a due date already raises the item
  in the list.
- **Nine o'clock from the stored date,** not a stored timezone or a chosen hour: decision 2 stays.

No choice here is costly enough to reverse to need an entry in decisions.md.

## 5. Interface

| Entry | Used by | Frequency | Necessity |
| --- | --- | --- | --- |
| Add dialog: Notify me, for a reminder | adding a reminder | Weekly | Core: the reminder is where it is asked for |
| Item: Notify me | asking later, stopping | Sometimes | Core: the only way to stop, and to mark a task |
| The mail | getting it | Daily at most | Core |
| CLI: the same switch | adding from a terminal or a script | Weekly | Enabler: reminders are often added there |
| Operator: turning the timer off, the tick's endpoint and secret | setup of a sleeping server | Once | Core there, unused elsewhere |
| A page of everything I asked to be notified about | recovery | Sometimes | Nice to have: left out |

On a server without mail the switch doesn't show, as the Email card doesn't.

```
ADD DIALOG · Reminder                     weekly · core
┌──────────────────────────────────────┐
│ [Note | Reminder]                    │
│ title                                │
│ Date: <exact date>                   │
│ [ ] Notify me on the day             │  only for Reminder, unticked
│ need, effort, impact, tags …         │
│                       [Cancel] [Add] │
└──────────────────────────────────────┘
States: hidden without mail. A date of today or earlier: the box is off, with a line saying it's in the list now.
```

```
ITEM · fields                             sometimes · core
│ Date: 3 March                        │
│ Notify me: [off | on]                │  a line under it says when: "on the day" for a reminder to
│ Tags …                               │  come, "when something waits for you" otherwise
States: hidden without mail, on a closed item, and for someone who can't see the heap's items.
```

```
THE MAIL                                  core
Subject: Renew the certificate            one: its title; several: "3 things wait for you in Upheap"

Reminder for 3 March: Renew the certificate
  Home · https://upheap.example.com/…

Question: Which region should the bucket be in?
  Move the backups · Ops · https://upheap.example.com/…

You get this because you asked to be notified about these items.
Turn it off on the item.
```

```
CLI
upheap item add "Renew the certificate" --reminder 2027-03-03 --notify
upheap item edit <item> --notify | --no-notify
```

The item is a layout used many times a session, so its switch gets an approved mockup before it is built.

## 6. Stages

**Stage 1: a reminder's notification, end to end.** One pull request each, in order.

1. **The tick.** The timed run in the server, its own timer, the endpoint and its secret, the settings, and the
   operator's pages. Done when a tick runs on the timer and through the endpoint, two at once do a job once, and
   the operator's docs say how to set up each kind of server. Covers rules 10 and 11.
2. **Notify me on an item, and the date's notification.** The wish, the pending notification, the mail, the
   tick's job, in the API. Done when a reminder with Notify me on is mailed on its morning, and scenarios 1, 4, 5
   and 6 pass as tests. Covers rules 1 to 3, 5 to 9, 12 and 13.
3. **The web app:** the Add dialog's box and the item's switch, after the mockup. Done when the golden path works
   in a browser.
4. **The CLI:** the item's two flags, and the user's pages. Done when the golden path works from a terminal.

**Stage 2:** notifications for what starts waiting on a marked item (rule 4, scenarios 2, 3 and 7).

## Left out

Not part of this design.

- A browser or phone notification as a second channel.
- A switch on a heap, and choosing which kinds of event notify.
- A notification when an ordinary item comes due.
- An hour of the person's choosing, and quiet hours.
- A page on Account listing every item with Notify me on.
- A default that ticks Notify me on every new reminder.
- Other timed work for the tick, such as clearing expired sign-ins and links.
