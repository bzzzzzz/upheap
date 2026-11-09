# Getting in: design

How a person gets an account, gets into a team, and keeps being able to sign in: invite links, password change and
password reset. In progress, by the steps of the maintainer's design method. Scale: a feature area, so Value is
short and [product.md](../product.md), [ux.md](../ux.md) and [architecture.md](../architecture.md) are given.
Step agreed: 2.

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
4. They create an account, though registration is closed on this server. They get their own personal team and heap
   like any new user.
5. They confirm joining, and are in the team as a member, with its heaps in reach.
6. The admin sees them in the member list, and the team's activity says they joined and through which link.
7. A second teammate, who already has an account, opens the same link, signs in, confirms, and is in.
8. A week later the link stops working. Whoever opens it is told so and to ask the team for a new one.

### Commitments

These go into product.md when the design is agreed. Where one changes an existing statement, it says which.

**Invite links**

1. A team admin can make an invite link for a team they administer. A personal team has none.
2. A link works for one week from when it's made. It can't be extended; the admin makes a new one.
3. Any number of people can join through one link while it works.
4. Any admin of the team can revoke a link, and it stops working at once. People who already joined through it
   stay.
5. A team can have several working links at a time. Admins see each one with who made it and when it ends.
6. Opening a working link shows the team's name and who made the link, to anyone, signed in or not.
7. A signed-in user joins by confirming on that page. They join as a member, never as an admin.
8. A user who is already in the team is taken to it, and nothing changes.
9. A link that has ended or been revoked lets no one in and says which happened.
10. Links are never deleted. Ended and revoked links stay in the team's history.
11. Making a link, revoking one, and each join through one are in the audit log, the join naming the link. Admins
    see them where they see membership changes.
12. An admin can still add an existing user directly. (Changes Teams §6: users join when an admin adds them or
    through an admin's invite link.)

**Creating an account**

13. A person without an account can create one from a working invite link, whether registration is open or closed.
    (Changes Accounts §2: with registration closed, a new account needs an invite link.)
14. An account made this way is like any other: it has its personal team and heap (Teams §3) and stays if the
    person later leaves or is removed from the team.

**Passwords**

15. A signed-in user can change their password in Account by giving the current one and a new one.
16. Changing a password ends every other sign-in of that user, in browsers and in command-line clients. The one
    they're using stays.
17. The operator can make a password reset link for a user with a command on the server. It works once and for one
    hour. Making a new one for the same user cancels the earlier one.
18. Opening a reset link lets the person set a new password. That ends all their sign-ins and signs them in here.
19. There is no reset inside the app. The sign-in page says to ask whoever runs the server.
20. A password change and a reset are in the audit log, without the password, and the user sees them in Account.

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
3. Confirms joining the team.
4. Is in the team, and has a personal heap of their own beside it.

**N2. The link has ended or was revoked** (recovery)

1. Opens the link and is told which happened.
2. Asks the team for a new one.

**N3. They have an account after all** (recovery)

1. Tries to create an account and is told the email address or the username is taken.
2. Signs in from the same page and carries on from M1 step 3, or, with the password forgotten, goes to P3 and
   opens the link again afterwards.

**N4. Stopped halfway** (recovery)

1. Created the account but closed the page before confirming. They have an account and aren't in the team.
2. Opens the link again, signed in or after signing in, and confirms.

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
7. A newcomer creates an account from the link's page, with registration open or closed. (N1.2)
8. Someone opening an ended or revoked link is told which. (N2.1)
9. Someone told their email address or username is taken can sign in from the same page and stay on the link.
   (N3.2)
10. Someone who signs in, or signs out and in again, from the link's page comes back to it. (M1.2, M2.2, N4.2)
11. A signed-in user sees which account they're joining as, and joins by confirming. (M1.3, N1.3)
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

## 5. Interface

## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- A member who was removed opens a link that still works: a scenario for step 4.
- Who the audit log names as the actor of an operator's reset: step 4.
- Which page a new member lands on when the team has several heaps: step 5.
- How the sign-in page knows who runs the server, for example a contact line the operator sets: step 5.
- Whether "this email address is taken" on the link's page tells a stranger who has an account: step 4.
- Wrong current password, and how many tries, in the change form: step 4.

## Later

Ideas outside the scope. Not commitments.

- Optional SMTP: reset and invitations by email, with address verification.
- Changing an account's email address.
- Invite links for one person or one use, and into a single heap.
- Operator pages: the list of accounts, deactivating one, making reset links without a shell.
- Making the first account on a server whose registration starts closed.
- Consent or a notice for a direct add.
- Membership from an identity provider's groups, with OIDC.
