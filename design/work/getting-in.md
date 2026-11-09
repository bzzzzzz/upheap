# Getting in: design

How a person gets an account, gets into a team, and keeps being able to sign in: invite links, password change and
password reset. Designed by the steps of the maintainer's design method. Scale: a feature area, so Value is short
and [product.md](../product.md), [ux.md](../ux.md) and [architecture.md](../architecture.md) are given. Step
agreed: 6. The design is agreed, its commitments are in product.md, and stage 1 is next.

## Constraints

1. **The server sends no email.** Nothing here may need it. (Owner.)
2. **There is no operator role in the app.** The operator is whoever runs the server: they set its configuration
   and can run its commands. (Repository, and the owner's choice of an operator command for reset.)
3. **Adding an existing user directly stays** (Product Teams §6). Invite links come beside it. (Owner.)
4. **An invite link is reusable, lives one week, can't be extended and can be revoked.** (Owner.)
5. **With registration closed, an invite link from any team admin lets a new person create an account.** (Owner.)
6. **A password reset is a one-time link the operator makes with a server command.** (Owner.)
7. **Account has a password change form, and a change signs out the user's other sessions.** (Owner.)
8. **Sign-in through an identity provider stays as planned,** unchanged by this design (Product Accounts §3).
   (Owner.)
9. **The product's commitments hold:** nothing is deleted, every action is in the audit log, the roles are member
   and team admin, a personal team is never shared, a username never changes. (Repository.)
10. **The server build only.** The local build has no accounts and no teammates (Product Distribution §3).
    (Repository.)

## 1. Value

1. **Who and the problem.** The pitch says a heap can be shared by a team. Today a teammate gets in only if
   registration is open, they register on their own, tell an admin their username and wait to be added; with
   registration closed they can't get in at all. And anyone who forgets their password has lost the account.
2. **The questions it answers:**
   - How do I get my teammates into our heap?
   - Someone sent me a link to their team. How do I get in?
   - I forgot my password. How do I get back in?
   - I think someone knows my password. How do I shut them out?
3. **What it is not:**
   - Not email: no invitation by address, no reset by mail, no address verification.
   - Not user administration: no list of every account, no deactivating one, no operator pages in the app.
   - Not single sign-on or membership from an identity provider's groups. That is the planned OIDC work.
   - Not an approval queue: a link is the admin's yes, given in advance.

## 2. Golden path

### The path

1. An admin opens their team's page and makes an invite link. They get a link to copy and the day it stops
   working.
2. The admin sends the link to the team, by whatever they use to talk. Upheap takes no part in this.
3. A newcomer opens the link. They see which team it's for and who made it, and can create an account or sign in.
4. They create an account, though registration is closed on this server. That puts them in the team as a member,
   with its heaps in reach, and they get their own personal team and heap like any new user.
5. The admin sees them in the member list, and the team's activity says they joined and through which link.
6. A second teammate, who already has an account, opens the same link, signs in, confirms, and is in.
7. A week later the link stops working. Whoever opens it is told so and to ask the team for a new one.

### Commitments

Agreed, and now in [product.md](../product.md): Invite links §1 to §13, Teams §6, and Accounts and sign-in §2 and
§5 to §10. The sections below cite them there, as Product and the section.

### Actors and roles

| Action | Newcomer (no account) | Member | Team admin | Operator |
| --- | :---: | :---: | :---: | :---: |
| Open an invite link and see the team's name | ✓ | ✓ | ✓ | |
| Create an account from a working link | ✓ | | | |
| Join a team through a working link | after creating an account | ✓ | ✓ | |
| Make and revoke the team's invite links, see them | | | ✓ | |
| Add an existing user directly | | | ✓ | |
| Change their own password | | ✓ | ✓ | |
| Make a password reset link | | | | ✓ |
| Open or close registration | | | | ✓ |

The operator acts from outside the app and has no account because of it.

### Out

Each of these is in Later, not in the commitments.

1. Email of any kind: inviting by address, reset by mail, verifying an address, changing one.
2. Links for one person or one use, and links into a single heap.
3. Operator pages in the app, and deactivating an account.
4. The first account on a server that starts with registration closed. Today the operator opens registration,
   registers, and closes it.
5. Asking the invited person before a direct add, or telling them after one.

## 3. Workflows

None of these is a loop flow: nobody does them daily. Each is **setup** (once, or rarely) or **recovery** (after a
mistake or a failure). Signing in, the one loop flow nearby, is built and unchanged
([ux.md](../ux.md#a-team-the-server-build)).

### Team admin

**A1. Invite the team** (setup: once per team, and again when people join later)

1. Opens the team's page and makes an invite link.
2. Copies it and sends it to the team, outside Upheap.
3. Sees people appear in the member list as they join.
4. When the link has ended and more people are to come, makes a new one.

**A2. Add someone already on the server** (setup). Built, unchanged: finds the user by name and adds them
([ux.md](../ux.md#teams-and-team)).

**A3. Shut a link that went where it shouldn't** (recovery)

1. Looks at the team's links: who made each, when it ends.
2. Revokes the link.
3. Looks at who joined through it.
4. Removes anyone who shouldn't be in the team, the way members are removed today.
5. Makes a new link if the team still needs one.

### Newcomer, without an account

**N1. Join with a new account** (setup: once)

1. Opens the link and sees which team it's for and who made it.
2. Chooses to create an account and gives a name, a username, an email address and a password.
3. Is in the team, and has a personal heap of their own beside it.

**N2. The link has ended or was revoked** (recovery)

1. Opens the link and is told which happened.
2. Asks the team for a new one.

**N3. They have an account after all** (recovery)

1. Tries to create an account and is told the email address or the username is taken.
2. Signs in from the same page and carries on from M1 step 3, or, with the password forgotten, goes to P3 and
   opens the link again afterwards.

### Member, with an account

**M1. Join another team** (setup: once per team)

1. Opens the link.
2. Signs in if they aren't, and comes back to the link's page.
3. Sees the team, who made the link, and which account they're signed in as, and confirms.
4. Is in the team, and finds it among their teams.

**M2. Signed in as the wrong account** (recovery)

1. Sees on the link's page that the account isn't the one they want.
2. Signs out, signs in as the other, and is back on the link's page.
3. If they had already joined with the wrong one, leaves the team from it, the way members leave today.

**M3. Already in the team** (recovery). Opens the link and is taken to the team.

### Any user

**P1. Change the password** (setup: rarely)

1. Opens Account and gives the current password and a new one.
2. Is told it changed and that their other sign-ins have ended.
3. On another device or in the command-line client, the next use asks them to sign in again.

**P2. Shut out someone who may know the password** (recovery)

1. Changes the password as in P1, which ends every other sign-in.
2. Looks at the account's history for password changes that weren't theirs.

**P3. Forgotten password** (recovery)

1. On the sign-in page, reads that a reset comes from whoever runs the server.
2. Asks them, outside Upheap.
3. Receives a link and opens it within the hour.
4. Sets a new password and is signed in. Every other sign-in has ended.

**P4. The reset link has ended or was used** (recovery). Opens it, is told which, and asks again.

**P5. Doesn't know the current password while signed in** (recovery). Can't change it in Account; goes to P3.

### Operator

**O1. Give someone a reset link** (recovery: rarely)

1. Hears from the user and makes sure it's them, outside Upheap.
2. Runs the server's command with the user's username or email address.
3. Gets the link and when it ends, or is told there is no such user.
4. Sends the link to the user.

**O2. Run a closed server** (setup: once)

1. Closes registration in the server's configuration.
2. From then on, new accounts come only through team admins' invite links.

### User stories

One per step that asks something new of the product. Steps that use what's built have none.

1. An admin makes an invite link on the team's page and gets the link and when it ends. (A1.1)
2. An admin copies a link they made earlier while it still works. (A1.2)
3. An admin sees the team's links, each with who made it, when it ends, and whether it works. (A3.1)
4. An admin revokes a link and it stops working at once. (A3.2)
5. An admin sees who joined through a link. (A3.3)
6. Anyone opening a working link sees the team's name and who made the link. (N1.1, M1.1)
7. A newcomer creates an account from the link's page, with registration open or closed, and is in the team.
   (N1.2)
8. Someone opening an ended or revoked link is told which. (N2.1)
9. Someone told their email address or username is taken can sign in from the same page and stay on the link.
   (N3.2)
10. Someone who signs in, or signs out and in again, from the link's page comes back to it. (M1.2, M2.2)
11. A signed-in user sees which account they're joining as, and joins by confirming. (M1.3)
12. A user who is already in the team is taken to it. (M3)
13. A user changes their password in Account with the current one and a new one, and is told their other
    sign-ins ended. (P1.1, P1.2)
14. A client whose sign-in was ended by a password change is asked to sign in again, and says why it can. (P1.3)
15. A user sees their password changes and resets in Account's history. (P2.2)
16. The sign-in page says how a forgotten password is reset. (P3.1)
17. Someone opening a working reset link sets a new password and is signed in. (P3.4)
18. Someone opening an ended or used reset link is told which. (P4)
19. The operator runs a command with a username or an email address and gets a reset link and when it ends, or
    is told there is no such user. (O1.2, O1.3)

### Not core

1. Getting rid of an account that came in through a link and shouldn't exist. The admin removes the person from
   the team; the account stays, with its empty personal team. Deactivating one is in Later.
2. Telling a person they were added directly, or asking them first.
3. Turning down an invitation. Not opening the link is the answer.
4. The first account on a closed server (Out §4).
5. Changing a username or an email address.

## 4. Behaviours

### Concepts

1. **Invite link.** Belongs to one team that isn't a personal team. It has who made it, when it was made and when
   it ends, and, once revoked, who revoked it and when. Holding the link is all it takes to open it. States:
   - **working**, from the moment it's made;
   - **ended**, a week after it was made, with nobody acting;
   - **revoked**, when an admin revokes a working link.

   Ended and revoked are final: nothing moves a link out of them.
2. **Join.** A person becoming a member of a team: by an admin's direct add, as today, or through an invite link.
   A join through a link remembers the link.
3. **Removal.** An admin taking a member out of a team. It differs from leaving, which the member does. The team
   remembers when each person was last removed.
4. **Reset link.** Belongs to one user. Made by the operator's command and shown once, there. States:
   - **working**, from the moment it's made;
   - **used**, once a password has been set with it;
   - **ended**, an hour after it was made;
   - **cancelled**, when a newer reset link is made for the same user, or the user changes their password.

   Used, ended and cancelled are final.
5. **Sign-in.** One browser session or one command-line client's sign-in, as today. "Ending a user's sign-ins"
   means all of both kinds.
6. **Account history.** The record of what happened to one user's account. It belongs to the user, not to a team.

### Rules

**Invite links**

1. Only an admin of the team makes, sees and revokes its links, checked when they act.
2. A link belongs to the team, not to its maker. It keeps working when its maker is demoted, leaves or is removed,
   and any admin of the team can revoke it.
3. A link is working when it hasn't been revoked and was made less than 7 × 24 hours ago. Only a working link can
   be revoked.
4. Admins can see a working link again and copy it, for as long as it works. An ended or revoked link is listed
   without it.
5. Anyone holding a link, signed in or not, sees the team's name and the maker's name, and nothing else of the
   team. This is the one place a person outside a team sees its name. For an ended or revoked link they see the
   same and which of the two happened.
6. A join through a link needs the link to be working at the moment of the join, not when the page was opened.
7. A person joins as a member. A person who is already in the team, member or admin, is left as they are, and
   nothing is recorded.
8. A person who was removed from the team can't join through a link made before the removal. A link made after it
   lets them in, and so does a direct add. A person who left by their own choice can join through any working link.
9. Any number of people can join through one link, and a team can have any number of working links.

**Creating an account from a link**

10. It is one act: the account, its personal team and heap, and the membership are all made, or none is.
11. It passes the same checks as registration (the username and the email address are free, the password is
    acceptable) and answers them the same way, plus rule 6. When the link has stopped working in the meantime,
    no account is made, whether registration is open or closed.
12. With registration closed, this is the only way an account is made.

**Passwords**

13. Changing a password needs a browser session and the current password. A command-line sign-in can't change it.
14. A wrong current password counts as a failed sign-in attempt for the account, under the same limit.
15. A change ends every other sign-in of the user in the same act, leaves the session it was made from, and
    cancels the user's working reset link.
16. The operator's command makes a reset link for one existing user, named by username or by email address. It
    works for 60 minutes. Making it cancels the user's earlier working one. The link is shown once, by the command.
17. Anyone holding a working reset link sees whose account it's for and can set a new password. Doing so uses the
    link up, ends all of the user's sign-ins, starts a session in that browser, and clears the account's count of
    failed sign-in attempts.
18. A used, ended or cancelled reset link sets nothing and says which happened.

**What is kept and who sees it**

19. Invite links are never deleted. Making and revoking one are recorded for the team, with the admin as the
    actor, and seen by its admins. A link ending by time is not an act and has no entry.
20. A join through a link is recorded as a membership change, with the person who joined as the actor and the
    link named. A direct add is recorded as today.
21. Account history records a password change (actor: the user), a reset link being made (actor: the operator, who
    has no account) and a password being set with one (actor: the user, by reset link). No entry holds a password
    or a link.
22. Only the user sees their account history. Team admins don't, and the operator has no pages.
23. None of this exists in the local build.

### Scenarios

Each is walked through the rules by hand, and kept as a test case.

1. **The link ends while the form is open.** Made Monday 10:00. A newcomer opens it the next Monday at 09:59 and
   submits the account form at 10:01. Rules 3, 6 and 11: no account is made, and they're told the link has ended.
2. **The maker loses admin rights.** Ana makes a link on Monday and is demoted on Tuesday. Rule 2: the link keeps
   working. Rule 1: Ana no longer sees it and can't revoke it; the other admin can.
3. **A removed member with an old link.** Sam is removed on Wednesday and still has Monday's link. Rule 8: it
   doesn't let Sam in, and the page says to ask an admin. A link made on Thursday does.
4. **A member who left, with the same link.** Sam left on Wednesday by choice. Rule 8: Monday's link lets Sam back
   in.
5. **A leaked link.** A stranger creates an account through it on a closed server. The admin revokes the link
   (rule 3) and removes the stranger. Rule 8 keeps them out of older links, and there are none working. Their
   account stays, with its personal team (Workflows, Not core §1).
6. **An admin opens their own team's link.** Rule 7: they're taken to the team, stay an admin, nothing is recorded.
7. **Someone else has a session.** A thief holds a session on Sam's account without knowing the password. Sam
   changes the password: rule 15 ends the thief's session. The thief tries first: rule 13 asks for the current
   password, and after five wrong tries rule 14 refuses more for the window. Sam's own session keeps working
   through it, though a new sign-in by Sam waits out the same window.
8. **Two reset links.** The operator makes one at 10:00 and, Sam having lost it, another at 10:20. Rule 16: the
   first is cancelled. Sam opens the first and is told so (rule 18), then the second at 11:15, which works.
9. **Password remembered after all.** Sam asks for a reset, then signs in and changes the password in Account.
   Rule 15: the reset link is cancelled, so the copy sitting in a chat can't be used by someone else.
10. **Locked out, then reset.** Sam has used up the sign-in attempts and gets a reset link. Rule 17: setting the
    password signs Sam in at once and clears the count.

### Choices

Each goes to decisions.md when the design is agreed.

1. **A working invite link can be seen again.** Context: one link is sent around for a week, to people who join on
   different days. Instead of: showing it once, like a reset link, which would have admins make a new link each
   time someone asks. Consequence: the link is kept readable, so a leaked database holds working links; each is
   worth a membership for at most a week, and revoking ends it.
2. **A link belongs to the team and outlives its maker's admin rights.** Instead of: ending a maker's links when
   they're demoted or removed. Reason: the team sent that link out, and people still hold it; any admin can revoke
   it. Consequence: demoting an admin for cause means looking at the links too.
3. **A removed person can't come back through an older link.** Instead of: letting any working link admit them,
   which makes removal mean nothing while a link is out; or revoking every link on a removal, which locks out
   everyone still on their way in. Consequence: the team remembers removals, and leaving is told apart from being
   removed.
4. **An account made from a link joins at once; a signed-in user confirms.** Instead of: a confirm for both.
   Reason: the newcomer opened the link to join, while a signed-in user may be in the wrong account. Consequence:
   no accounts stranded outside every team on a closed server.
5. **A password is changed only from a browser session, with the current password.** Instead of: letting a
   command-line sign-in change it. Reason: the same as for consent, where a token can't widen its own access.
6. **A reset comes from the operator's command, once, for an hour.** Instead of: email, which the server doesn't
   send; or a link made by a team admin, who could then take over a member's account and with it their personal
   team and other teams. Consequence: a reset waits for a person with access to the server.
7. **Account history is the user's alone.** Instead of: showing password changes to team admins. Reason: an
   account isn't a team's, and a person may be in several.

### Unproven mechanisms

None. Each rule is something the server already does in another form: single-use secrets with a lifetime, ending
sign-ins, membership writes under the team lock, a subcommand of the server binary.

## 5. Interface

The web app's screens follow [ux.md](../ux.md): a card on a wide screen and the bare form on a phone, a problem
with an input under it, the settings layout for the Team page, History collapsed where a thing is edited. The
wording below shows what a line says, not its final copy.

### Inventory

One build, the server's, so necessity is rated once. Nothing here is used more than occasionally; the ratings
differ in necessity.

| # | Entry | Flow steps | Frequency | Necessity | Why (4 and 5) |
| --- | --- | --- | :---: | :---: | --- |
| 1 | Team page, Members: invite links (make, copy, revoke, past links, each link's history) | A1, A3 | 2 | 5 | The only way in on a closed server, and A3 can't shut a link without it |
| 2 | Invite page, signed out: create account or sign in | N1, N3, M1.1–2 | 1 | 5 | Where every newcomer enters |
| 3 | Invite page, signed in: confirm joining | M1.3, M2 | 1 | 5 | Where everyone with an account enters |
| 4 | Invite page, link can't be used: ended, revoked, removed, not a link | N2, scenario 3 | 1 | 4 | The flow dead-ends without saying what to do |
| 5 | Team activity: lines for a link made, revoked, and a join through one | A3.3 | 1 | 4 | How an admin learns who came in through a leaked link |
| 6 | Sign-in page: how a forgotten password is reset | P3.1 | 1 | 4 | Without it a locked-out person has no next step |
| 7 | Reset page: set a new password; link ended, used or cancelled | P3.3–4, P4 | 1 | 5 | The only way back into an account |
| 8 | Account, Password: change it, with its history | P1, P2, P5 | 2 | 4 | Shutting out someone who knows the password has no other way short of the operator |
| 9 | Operator command: make a reset link | O1 | 1 | 5 | The only source of reset links |
| 10 | Sign-in page with registration closed: say that an account needs an invite link | O2 | 1 | 3 | |
| 11 | Sign-in page: a contact line the operator sets, for "who runs this server" | P3.1 | 1 | 2 | |
| 12 | Command-line client: say why a sign-in ended after a password change | P1.3 | 1 | 2 | |

**The first set** is entries 1 to 9. Entries 10 to 12 are in Later: today's "registration is closed" error, a
general "ask whoever runs this server", and the client's existing "sign in again" do the job.

**Layout by frequency.** Nothing here is used several times a session, so nothing is added to the list, the
sidebar or the tabs. Each entry sits one step inside the page that owns it: links in the Team page's Members,
the password in Account, both closed until asked for.

**Where a new member lands:** on the team's heap when it has exactly one that isn't archived, otherwise on the
Team page, which lists the heaps. Either way a line says they joined.

### Wireframes

```
WIDE · Team page, Members, for an admin          frequency 2 · necessity 5
┌────────────────────────────────────────────────────────────┐
│ Members                                                     │
│ [Find someone on this server…]              [Invite link]   │
│                                                             │
│ Invite links          Anyone with a link joins as a member. │
│  Made by Ana · ends Mon 16 Nov      [Copy] [Revoke] History │
│  Made by Ben · ends Thu 12 Nov      [Copy] [Revoke] History │
│  Past links (3) ▸                                           │
│                                                             │
│ member rows, as today                                       │
└────────────────────────────────────────────────────────────┘
States: no working link (the button and the one line only); just made (new row first,
  marked, Copy as its main action); copied (a status line); revoking (the row turns
  into an inline confirm: it stops working at once, people who joined stay);
  failed (the server's message on the row).
Past links, opened: "Made by Ana 2 Nov · ended 9 Nov · 4 joined", or "· revoked by Ben
  5 Nov", each with History and no link.
History on a row: made, revoked, and each person who joined through it.
Members who aren't admins see none of this.
Phone: one column, Invite link spans the width, a row's actions in its ⋯ sheet.
Next likely action: Copy.
```

```
Invite page, signed out                          frequency 1 · necessity 5
┌──────────────────────────────────┐
│ Join Acme                         │
│ Ana Ruiz invited you.             │
│ (Create account) (Sign in)        │   switch, Create account first
│ name                              │
│ username        checked as typed  │
│ email                             │
│ password                          │
│ [Create account and join]         │
└──────────────────────────────────┘
Sign in side: email or username, password, [Sign in]; then the signed-in page below.
States: checking the link; username taken (under it); email taken (under it, with
  "Sign in instead", which switches side and keeps the address); too many attempts;
  the link stopped working meanwhile (the page becomes "can't be used").
After: where a new member lands, with "You joined Acme".
Next likely action: Create account and join.
```

```
Invite page, signed in                           frequency 1 · necessity 5
┌──────────────────────────────────┐
│ Join Acme?                        │
│ Ana Ruiz invited you.             │
│ You're signed in as Sam Ortiz (@sam).
│ [Join Acme]                       │
│ Not you? Sign out                 │   comes back here, signed out
└──────────────────────────────────┘
States: already in the team (no page: straight to the team, with a line saying so);
  joining; the link stopped working meanwhile.
Next likely action: Join.
```

```
Invite page, link can't be used                  frequency 1 · necessity 4
┌──────────────────────────────────┐
│ This invite link has ended.       │   or: was revoked
│ It was for Acme, from Ana Ruiz.   │
│ Ask the team for a new one.       │
│ [Open Upheap]                     │   the app when signed in, else sign-in
└──────────────────────────────────┘
Removed from the team (signed in, rule 8): says they were removed, that this link
  can't add them back, and to ask one of the team's admins.
Not an invite link: says so, names no team.
```

```
Team activity, new lines                         frequency 1 · necessity 4
  Ana made an invite link
  Ben revoked Ana's invite link
  Sam joined through Ana's invite link
Worded by the server, shown to admins, like other membership lines.
```

```
Sign-in page, addition                           frequency 1 · necessity 4
│ password                          │
│ Forgot your password? ▸           │   opens one line, no form:
│   Whoever runs this Upheap server can send you a reset link.
```

```
Reset page                                       frequency 1 · necessity 5
┌──────────────────────────────────┐
│ Set a new password                │
│ for Sam Ortiz (@sam)              │
│ new password                      │
│ [Set password and sign in]        │
│ This signs you out everywhere else.
└──────────────────────────────────┘
States: password not acceptable (under it); the link has ended, was already used, or
  was replaced by a newer one (each says which, and to ask for a new one); not a
  reset link.
After: where signing in lands, with a line that the password changed.
```

```
Account, Password card                           frequency 2 · necessity 4
Between Profile and Appearance. Closed:
│ Password                                    [Change password] │
Open:
│ current password                                              │
│ new password                                                  │
│ [Change password] [Cancel]                                    │
│ This signs you out of your other browsers and the command line.
│ History ▸                                                     │
States: wrong current password (under it); too many attempts (above the button, with
  how long to wait); new password not acceptable (under it); changed (the card
  closes, a status line says so and how many other sign-ins ended).
History, opened: password changed; reset link made by the operator; password set
  with a reset link. Each with its time.
Not in the local build.
```

```
Operator command                                 frequency 1 · necessity 5
$ upheap-server user reset-link sam              a username or an email address
https://upheap.example/reset/…                                       (stdout)
For Sam Ortiz (@sam). Works once, until 11:20 UTC.                   (stderr)
An earlier reset link for them no longer works.                      (stderr, when so)

The link alone is on stdout, so it can be piped. No prompts. It reads the server's own
configuration, and needs the database and the server's address from it.
Exit: 0 made; 2 no user given; 3 no such user; 5 the database can't be reached.
```

## 6. Stages

### Stage 1: the golden path, end to end

An admin makes a link, a newcomer creates an account through it on a closed server and lands in the team, a
second person with an account joins through the same link, and the link stops after a week. Five pieces, one pull
request each, in this order. Product is product.md; rules and scenarios are numbered as in Behaviours.

1. **Invite links on the server.** An admin makes a link for a team, sees the team's links with the working ones
   readable, and revokes one. A link is working, ended or revoked by rules 1 to 4 and 9, and making and revoking
   are in the team's audit log (rule 19). Done when: the API does all three for an admin, refuses a member and
   hides the team from an outsider, with rows in the access-control table test; a personal team has none;
   scenario 2 is a test; the suites pass on SQLite, MySQL and Postgres. Product Invite links §1 to §5 and §10, and
   the making and revoking half of §11.
2. **Opening a link and joining.** Anyone holding a link reads the team's name, the maker's name and the link's
   state (rule 5). A signed-in user joins as a member (rules 6 and 7), a person removed since the link was made is
   refused (rule 8), and the join is in the audit log naming the link (rule 20). Done when: scenarios 3, 4 and 6
   are tests; a direct add still works as before. Product Invite links §6, §8 and §9, the signed-in
   half of §7, the rest of §11, and Teams §6.
3. **Creating an account from a link.** One act makes the account, its personal team and heap, and the membership
   (rules 10 to 12), with registration open or closed. Done when: scenarios 1 and 5 are tests; plain registration
   on a closed server is still refused. Product Invite links §12 and §13, the rest of §7, and Accounts §2.
4. **Team page: invite links.** For an admin, the Invite link button and the working links as rows with Copy and
   Revoke, the revoke confirmed inline (Interface 1, without past links and History). The team's activity shows
   the new lines (Interface 5). Done when: it works at phone, tablet and wide widths, and a member sees none of
   it.
5. **The invite page, and the path as a test.** Signed out with Create account and Sign in, signed in with the
   confirm, and the page for a link that can't be used (Interface 2 to 4), with the landing rule. Done when: a
   Playwright golden path runs the whole of stage 1 against a server with registration closed, at three widths:
   an admin makes a link, a newcomer creates an account and lands in the heap, a second user signs in and joins,
   and a revoked link says so.

Deployment needs no piece: production already runs with registration closed, and the path is what makes that
usable. Each pull request updates the Getting in row in progress.md.

### Stage 2: the rest of the first set

1. **Past links and each link's history** on the Team page (the rest of Interface 1).
2. **Password change** in Account, ending the user's other sign-ins and cancelling a reset link (Interface 8
   without History; Product Accounts §5 and §6; rules 13 to 15; scenario 7).
3. **Password reset:** the operator's command, the reset page, and the forgot-password line on the sign-in page
   (Interface 6, 7 and 9; Product Accounts §7 to §9; rules 16 to 18; scenarios 8 to 10).
4. **Account history** under the Password card (the rest of Interface 8; Product Accounts §10; rules 21 and 22).
5. **This doc dissolves:** the flows into ux.md, the concepts and rules into architecture.md, the choices into
   decisions.md, and the file goes.

### Later

The Later list at the end of this doc. Nothing there is a commitment.

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- Where a removal is remembered, since the membership row goes: for the implementation.
- An invite link is kept readable while session and reset secrets are kept hashed: for the implementation.
- The shape of the invite and reset addresses, under architecture.md's rule for handles in URLs: for the
  implementation.

## Later

Ideas outside the scope. Not commitments.

- Optional SMTP: reset and invitations by email, with address verification.
- Changing an account's email address.
- Invite links for one person or one use, and into a single heap.
- Operator pages: the list of accounts, deactivating one, making reset links without a shell.
- Making the first account on a server whose registration starts closed.
- Consent or a notice for a direct add.
- Membership from an identity provider's groups, with OIDC.
- The sign-in page saying, with registration closed, that an account needs an invite link (Interface 10).
- A contact line the operator sets, shown where the sign-in page says who resets passwords (Interface 11).
- The command-line client saying that a sign-in ended because the password changed (Interface 12).
