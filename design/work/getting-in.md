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

## 4. Behaviours

## 5. Interface

## 6. Stages

## Parked

Thoughts from a lower level than the step that met them. One line each, not discussed until their step.

- A member who was removed opens a link that still works: a scenario for step 4.
- Who the audit log names as the actor of an operator's reset: step 4.
- Which page a new member lands on when the team has several heaps: step 5.

## Later

Ideas outside the scope. Not commitments.

- Optional SMTP: reset and invitations by email, with address verification.
- Changing an account's email address.
- Invite links for one person or one use, and into a single heap.
- Operator pages: the list of accounts, deactivating one, making reset links without a shell.
- Making the first account on a server whose registration starts closed.
- Consent or a notice for a direct add.
- Membership from an identity provider's groups, with OIDC.
