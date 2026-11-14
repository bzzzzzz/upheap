# Email: design

Mail from the Upheap server: a reset link a user asks for themselves, a confirmed address before sign-in, and
changing an address. It serves Product, Accounts and sign-in. Drafted by the steps of the maintainer's design
method, all six at once for one review, since the owner settled the scope beforehand. Step agreed: none.

The core commitments are in [product.md](../product.md#email), which is their home; where this doc and product.md
differ, product.md wins. Everything else stays here until it's built. As each piece lands, its mechanisms move to
architecture.md, its screens to ux.md, its operator steps to README.md and operations.md, and this doc shrinks; it
goes with the last piece.

## Constraints

1. Mail is optional. A server with no mail set up works exactly as it does today, operator reset links included.
   (Owner, 7x7y0.)
2. Plain SMTP, so any provider works. Cloud Run blocks port 25; 587 and 465 are open. (Owner, 7x7y0.)
3. Nothing is sent for real in development or CI. (Owner, 7x7y0.)
4. When mail is set up, an address is confirmed before its account can sign in. (Owner, 7x7y0.q1.)
5. No invitations by mail for now; personal invites are w6brs. (Owner, 7x7y0.q2.)
6. Self-hosted, free, one binary on SQLite, MySQL or Postgres, portable SQL only. (Repository: product.md
   Distribution, architecture.md Principles.)
7. Upheap doesn't interrupt: it shows what needs the person when they come and look. (Repository: product.md
   Pitch.)
8. Sign-in attempts are limited per address, and a refusal doesn't reveal whether an account exists.
   (Repository: decisions.md §11.)

## 1. Value

A self-hosted server today can't help a user who forgot their password unless the operator steps in, can't tell a
real address from a typo, and can't let a user move to a new address. Mail answers "I forgot my password, how do I
get back in?" without asking anyone, and "Is this account's address really theirs?" for the operator.

It is not a notification channel. Upheap still doesn't interrupt (Constraint 7): no mail about items, questions,
reviews or agents' work.

## 2. Golden path

The path, for the operator and then a user:

1. The operator sets the SMTP settings and the sender address, and restarts the server. The server starts as before
   and says in its log that mail is on.
2. The operator sends a test mail with a server command and sees it arrive. Mail now works.
3. A user who forgot their password presses **Forgot your password?** on sign-in, types their address and sends it.
   The page says a link is on its way if an account uses that address.
4. The mail arrives: a short plain-text message with a link that works once, for an hour.
5. The user opens the link and lands on the reset page they'd get from an operator's link, sets a new password and
   is signed in. Their other sign-ins end, and their account history says the reset was asked for by mail.

### Commitments

Core ones are marked **C**; they are in product.md.

Mail on the server

1. **C** Mail is optional. Without it the server behaves as it does today; with it, the features below appear.
2. **C** The server mails only about the account itself: links the user asked for, and notices of changes to their
   account. Never about items, questions, reviews or agents' work.
3. Mail goes out over SMTP to any provider the operator picks. The operator names the sender (a name and an
   address).
4. The operator can send a test mail to check the settings, without a user account.
5. In development and CI the server writes mail where a developer or a test can read it, and sends nothing.

Links in mail

6. A link in a mail works once, and ends: a reset link after an hour, a confirmation link after a day.
7. A new link of the same kind for the same account cancels the earlier one.
8. The server keeps only a hash of each link, so a copy of the database can't be used to open one.

Reset by mail

9. **C** With mail set up, a user who forgot their password gets a reset link by mail, asked for on the sign-in page.
   Setting a password with it works as with an operator's link: it ends all their sign-ins and signs them in.
10. **C** Asking for a reset answers the same whether an account uses the address or not.
11. Requests are limited per address, like sign-in, without using up the address's sign-in attempts.
12. The operator's reset link keeps working, with or without mail.

Confirming an address

13. **C** With mail set up, a new account confirms its address by a link before it can sign in. This holds for an
    account made through an invite link too.
14. An account that existed before mail was set up, or was made while mail was off, counts as confirmed.
    (Assumption.)
15. A user waiting to confirm can ask for the mail again.
16. The operator can confirm an account's address by hand, with a server command.
17. Using any link mailed to an address confirms it, a reset link included.

Changing an address

18. **C** A user can change their address by giving their password. The change takes effect only once a link mailed
    to the new address is opened, and the old address gets a notice that it changed.
19. Changing the address doesn't end other sign-ins.
20. A reset by mail, a confirmation and an address change are in the user's account history; nobody else sees
    them.

### Actors

| Actor | May |
| --- | --- |
| Operator | Set mail up or turn it off; send a test mail; make a reset link; confirm an address by hand |
| User, signed out | Ask for a reset link; ask for the confirmation mail again; open any link mailed to them |
| User, signed in | Change their address; see their account history |

### Out

Invitations by mail (Constraint 5), mail about work, HTML mail, a mail queue with retries, and per-user mail
settings. The last three are under Open for now.

## 3. Workflows

Operator

1. **Set mail up** (setup). Pick a provider and set up the sending domain there → set the SMTP settings and sender →
   restart → send a test mail → see it arrive.
2. **Mail stops working** (recovery). Users say no mail arrives → the log shows each failed send → fix the settings
   and send a test mail. Meanwhile make reset links and confirm addresses by hand. Turning mail off lets every
   account sign in again without confirming (Rule 9).
3. **A user can't get the confirmation mail** (recovery): typo, spam filter, a provider down. Confirm the account by
   hand, or the user signs up again with the right address.

User

4. **Forgot the password** (recovery, the golden path). Sign-in → Forgot your password? → address → mail → link →
   new password → signed in.
5. **The reset mail doesn't come** (recovery). Ask again, after the limit's window if it was hit; or ask the
   operator, as today.
6. **Sign up** (setup). Create account → "Check your mail" → open the link → confirmed and signed in, on the
   personal heap or, through an invite link, in the team.
7. **Sign in before confirming** (recovery). The right password, address not confirmed → the page says so and offers
   to send the mail again.
8. **Change the address** (sometimes). Account → Email → new address and password → "Check your new address" → open
   the link → the address changes; a notice goes to the old one.
9. **Someone else changed my address** (recovery). The notice at the old address says the account's mail now goes
   to the new one, and to ask whoever runs the server, who can make a reset link. (Rule 15.)

User stories come straight from these steps; none asks for anything the commitments don't cover.

Not core: invitations by mail, mail about work, a digest, unsubscribing (no mail is unwanted: each one was asked
for, or is a security notice).

## 4. Behaviours

### Concepts

1. **Mail set up**: the server has a mail transport configured: SMTP, or the development transport. A property of
   the server, read at startup. Not a setting in the database.
2. **Mail**: a message to one address, plain text, from the configured sender. Made by the server from a fixed
   template per kind: reset link, confirmation link, address-change link, address-changed notice, test.
3. **Mailed link**: a one-use token for one account and one purpose (reset, confirm, change to a given new address).
   States: working → used, cancelled or ended. Derived from its times, like an operator's reset link.
4. **Address state** of an account: confirmed, or waiting. Waiting matters only while mail is set up.

### Rules

Sending

1. Mail is sent after the transaction that made its link commits, and off the request: the answer never waits for
   the provider, so a request about an existing account takes as long as one about no account.
2. A send that fails is logged with the kind and the account, never the link, and isn't retried. The user asks
   again.
3. The development transport writes each mail, link included, to the log or to a file, and is the only one that
   records a link anywhere.

Links

4. A token is 256 random bits; only its SHA-256 is stored. A link is `{BASE_URL}/<purpose>/{token}`.
5. Lifetimes: reset one hour (as an operator's link), confirmation 24 hours, address change 24 hours.
6. One working link per account and purpose. A new one cancels the old in the same transaction. An operator's reset
   link and a mailed one are the same kind, so each cancels the other, and a password change cancels both.
7. Using a link is a conditional update, so of two racing requests one wins.
8. Using any mailed link confirms the address it was sent to (Commitment 17): the holder has shown it reaches them.

Requests from a signed-out user

9. Confirmation is required only while mail is set up. With mail off, a waiting account signs in as before.
10. A reset request for an unknown address, or for a waiting account, answers the same as for a confirmed one.
    A waiting account gets a reset link too; using it confirms the address (Rule 8).
11. Reset and confirmation-resend requests are counted per address, five per 15 minutes, in a count of their own,
    so asking for mail never locks the owner out of signing in, and many sign-in attempts never block a reset.
12. Sign-in with the right password on a waiting account is refused with its own answer (address not confirmed),
    and only after the password is checked, so it reveals nothing to someone without the password.

Confirmation

13. Opening a confirmation link confirms the address and signs that browser in. A new account through an invite
    link joined the team when it was made, and lands there.
14. The operator's confirm command confirms a waiting account; for a confirmed one it does nothing and says so.

Address change

15. Asking for a change takes the current password, counted like a password change. The new address must be free
    then, and is checked again when the link is opened; if it was taken meanwhile, the link is refused and the
    address stays.
16. Opening the change link sets the new address (confirmed), cancels the account's working reset link, and sends the
    notice to the old address. Sign-ins stay.
17. The account's sign-in limit and any counts follow the address, so a change starts a fresh count.

History

18. These go in the user's account history, worded like the password entries: a reset link mailed, a password set
    with it, an address confirmed (by link or by the operator), an address change asked, an address changed (from
    and to).

### Scenarios

1. **Mail turned on over existing users.** Accounts made before count as confirmed (Commitment 14), so nobody is
   locked out by a mail that may not arrive. Only sign-ups from then on confirm.
2. **Squatted address.** Someone signs up with Ana's address and never confirms. Ana can't sign up: the address is
   taken. She asks for a reset by mail, gets the link, sets a password and is in, confirmed (Rules 8, 10). The
   account keeps the username the squatter picked; Ana can't change a username today. (Open for now.)
3. **Mail breaks for a day.** Sign-ups pile up waiting. The operator confirms them by hand, or turns mail off, which
   lets them sign in (Rule 9), and on again once fixed: they count as waiting again until they confirm, which is
   the price of Rule 9's simplicity. (Open for now.)
4. **Reset requested twice.** The second mail cancels the first link; the first mail's link says it no longer
   works, as with operator links.
5. **Operator link and mailed link.** An operator makes a link while a mailed one works: the mailed one is
   cancelled. The user uses whichever came last.
6. **Change to an address someone took meanwhile.** Ana asks to move to `a@x`; before she opens the link, Ben signs
   up with `a@x`. Ana's link is refused and her address stays (Rule 15). Ben's account waits for confirmation he
   can give, since he holds `a@x`.
7. **Stolen session changes the address.** The thief needs the password too (Rule 15), and the old address gets a
   notice. With the password already stolen, the owner's recovery is the operator.
8. **Timing.** A reset request for an existing account takes as long as one for no account, since the send is off
   the request (Rule 1).

### Choices

1. **Plain SMTP, not a provider's HTTP API.** One code path for every provider; switching is four settings.
   Passes the bar for decisions.md (D).
2. **Plain text only, no HTML.** Every client shows it, links are clickable, nothing to render or test across
   clients. HTML can come later without changing a link.
3. **Send after commit, off the request, no queue.** A queue table with retries is safer against a provider
   outage, but adds a worker and a table; a user can ask again. Timing equality comes free.
4. **Existing and mail-off accounts count as confirmed,** instead of confirming at next sign-in. Turning mail on
   can't lock anyone out. The cost: those addresses were never checked. (Asked.)
5. **Invite-link accounts confirm too,** instead of the link vouching for them. An invite link works for anyone
   who has it, so it says nothing about the address. (Asked.)
6. **A mailed reset link is the same kind as the operator's,** one table and one page, instead of a second kind.
7. **Using any link confirms the address,** instead of only the confirmation link. Unblocks the squatted-address
   case with no new flow.
8. **Confirmation requirement depends on mail being set up now,** not on how the account was made, except for
   Rule 14's starting state. Turning mail off is the operator's escape hatch.

### Open for now

1. No queue or retry: watch for complaints of lost mail during provider outages.
2. A squatter's username stays with the account a reset rescues: watch for it in practice.
3. Accounts that waited, signed in while mail was off, and wait again when it's back: watch for confusion.
4. No limit on how many address changes a user asks for, beyond the per-address count.

## 5. Interface

### Inventory

| Entry | Flows | Frequency | Necessity |
| --- | --- | --- | --- |
| SMTP settings and sender | 1, 2 | Once | Core: nothing sends without them |
| Server command: send a test mail | 1, 2 | Once | Enabler: otherwise the first user finds a broken setup |
| Operator docs: setting up mail | 1 | Once | Enabler: without them a self-hoster guesses settings and DNS |
| Development transport | dev, CI | Constant for developers | Core: Constraint 3, and every test of the flows reads mail through it |
| Sign-in: Forgot your password?, with mail | 4, 5 | Sometimes | Core: the golden path starts here |
| Reset page (existing) | 4 | Sometimes | Core, unchanged |
| Sign-up: Check your mail | 6 | Once | Core under Constraint 4 |
| Sign-in: address not confirmed | 7 | Sometimes | Enabler: otherwise a waiting user dead-ends |
| Confirm page `/confirm/{token}` | 6 | Once | Core |
| Account: Email card | 8 | Sometimes | Core for changing an address |
| Address-change page `/change-email/{token}` | 8 | Sometimes | Core |
| Server command: confirm an address | 3 | Sometimes | Enabler: the recovery for a mail that never arrives |
| Account history entries | 4, 8 | Sometimes | Nice to have: the history exists; the new lines follow its pattern |
| Mail templates (five kinds) | all | — | Core |

Everything is in the first set. The Email card and the history lines follow the Password card and its History; the
link pages follow the Reset page; the sign-in additions sit where Forgot your password? is today. So no new kind
of page and no change to a core loop: whether a piece needs a mockup is settled in its own spec, by CLAUDE.md's
Interface changes.

### Wireframes

```
WEB · Sign in, Forgot your password? opened, mail set up          sometimes · core
┌──────────────────────────────┐
│ Email or username            │
│ Password                     │
│   Forgot your password? ▾    │
│   Email  [__________] [Send me a link]
│ [Sign in]                    │
└──────────────────────────────┘
After send: "If an account uses that address, a link is on its way. It works for an hour."
Too many: the server's message. Mail not set up: today's line about the operator, no form.
Next likely action: open the mail.
```

```
WEB · Check your mail (after Create account, mail set up)                once · core
┌──────────────────────────────┐
│ Check your mail              │
│ We sent a link to a@x.dev.   │
│ It works for a day.          │
│ [Send it again]  Wrong address? Create the account again.
└──────────────────────────────┘
Signing in to a waiting account shows the same card, from the server's answer.
Next likely action: open the mail.
```

```
WEB · /confirm/{token}                                                  once · core
States as the Reset page: checking, not a link, used, cancelled, ended ("Ask for a new one" → sign-in).
Working: confirms on open, signs in, lands on the heap (or the team, for an invite) with
the toast "Address confirmed. You're signed in."
```

```
WEB · Account, Email card (between Profile and Password)           sometimes · core
┌──────────────────────────────┐
│ Email   a@x.dev   [Change]   │
│  (open) New address, Current password, [Send link] [Cancel]
│  after: "Check a@new.dev. Your address changes when you open the link."
└──────────────────────────────┘
Shown only when mail is set up; without it the address is read-only, as today.
```

```
CLI · upheap-server
$ upheap-server mail test ana@example.com
Sent a test mail to ana@example.com from "Upheap <upheap@example.com>" via smtp.example.com:587.
$ upheap-server user confirm sam
Confirmed sam's address, sam@example.com.
Exit codes as `user reset-link`: 0 done, 2 arguments, 3 no such user, 5 database, 1 else; `mail test`: 6 mail not set up or send failed.
```

```
MAIL · reset link (plain text)
Subject: Reset your Upheap password
Someone asked to reset the password for @sam on <BASE_URL host>.
Open this link to set a new one. It works once, for an hour:
<link>
If it wasn't you, ignore this mail. Your password stays as it is.
```

## 6. Stages

### Stage 1: reset by mail, end to end

1. **Design:** this doc, and product.md no longer promising "no email". Done when merged.
2. **Mail foundation:** optional SMTP settings and sender; a sender other pieces call, with the
   development transport (log and file); `upheap-server mail test`; the operator docs, in README.md beside
   Resetting a password, and decisions.md's entry for Choice 1. Done when: unset, the server behaves as today;
   set, a test mail goes out; the tests send mail through the file transport. Commitments 1–5.
3. **Production mail:** the owner picks the provider, sets up the sending domain, and puts
   the settings in Cloud Run; operations.md records it. Done when `mail test` from production arrives.
4. **Reset by mail:** Forgot your password? with mail, the mailed link, the count of its own, history
   entries. Done when the golden path works in production. Commitments 6–12, 20 in part.

### Stage 2

- **Change an address:** the Email card, the change link, the notice to the old address. Commitments 18–20.
- **Confirm an address:** Check your mail, the confirm page, sign-in refusing a waiting account, the
  confirm command. Commitments 13–17.

## Parked

Below this design's level; for the pieces' specs.

- Settings as one `SMTP_URL` (lettre parses it) or as separate host, port, user, password and TLS variables; the
  sender as `MAIL_FROM`. Piece 2 decides.
- Where mailed links live: the existing `password_reset_links` table for resets, and one new table with a purpose
  column for confirm and change, or one table for all three.
- The address state as an `email_confirmed_at` column, filled for every existing account by the migration.
- How the web app learns mail is set up: a field on an existing public response.
- The SMTP client: `lettre` with rustls, behind a small trait so the file transport and tests swap in.
- Sending off the request: a spawned task with a timeout; at shutdown, in-flight sends drain with the requests.
- The Playwright tests read the file transport's directory to find a link.
