# Group-managed teams: design

Team membership that follows the identity provider's groups. The operator maps provider groups to existing teams in
the server's configuration; at each sign-in through the provider, Upheap adds the person to the mapped teams their
groups name and takes them out of those their groups no longer name. It serves Product, Teams and Accounts and
sign-in, and builds on signing in through a provider (architecture.md, Authentication and authorization §3).

The core commitments are in [product.md](../product.md#teams), which is their home, and the last-admin exception is
[decisions.md](../decisions.md) §41. Where this doc and those differ, they win. Everything else stays here until it's
built: then the mechanism moves to architecture.md, the screens to ux.md, the settings to README.md, and this doc
goes.

## Constraints

1. Server config maps groups to teams. There is no in-app setup in this version. (Owner.)
2. A group gives membership only, never the admin role. Admins are still made by hand among the members. (Owner.)
3. A team mapped to a group is group-managed: it takes no hand-added or invite-link members. Only the groups decide
   who joins it. (Owner.)
4. A group maps only to a team that exists; nothing makes a team. (Owner.)
5. A sync that would remove a team's last admin removes them anyway. (Owner.)
6. Groups are read at sign-in only. Removal between sign-ins, from the provider's own push (SCIM), is later work
   built on this. (Owner.)
7. Configuration comes from environment variables, checked at startup; bad configuration stops the server.
   (Repository: architecture.md, Configuration and deployment §1.)
8. Every change to a team's members is in the audit log, shown to the team's admins. (Repository: product.md, Audit
   log.)

## 1. Value

An organisation that runs Okta, Keycloak or Entra ID already says who works on what, in its groups. Today someone
has to repeat that in Upheap by hand, and nothing takes a person out of a team when they leave the group or the
company. With this, "who is in Engineering's team?" has one answer, the provider's, and leaving the group ends
access at the next sign-in.

## 2. Golden path

1. An admin makes the team in Upheap as today, say Acme Engineering at `/teams/acme-eng`.
2. The operator maps the provider's `engineering` group to `acme-eng` in the server's settings, adds the groups
   claim to the provider's ID token if it isn't there, and restarts the server.
3. Sam, in the `engineering` group, signs in through the provider. Sam is now a member of Acme Engineering, and the
   team's Activity says "Sam joined through the engineering group".
4. On the Team page, the admin sees "Members come from Acme SSO groups: engineering." There is no Add member search
   and no Invite link; the admin can still make Sam an admin.
5. Sam leaves the `engineering` group. At Sam's next sign-in through the provider, Sam is out of the team, and
   Activity says "Sam left: no longer in the engineering group".

## 3. Rules

### Configuration

1. One setting, `UPHEAP_OIDC_GROUP_TEAMS`, holds the map as JSON: each key a team's slug, each value the list of
   group values that give membership of it, as the provider sends them.
   `{"acme-eng": ["engineering"], "acme-ops": ["ops", "sre"]}`. JSON because group names carry spaces, commas and
   `=` (Okta, Keycloak paths like `/acme/eng`), and Entra ID sends object IDs. A person is in a mapped team when
   any of its groups is among theirs. One group may appear under several teams.
2. `UPHEAP_OIDC_GROUPS_CLAIM`, default `groups`, names the ID token claim to read. Keycloak and Okta can name it
   otherwise. Group values are matched exactly, case and all.
3. The server stops at startup, naming the problem, when the map is set without a provider, isn't valid JSON of that
   shape, names a slug no team has, or names a personal team. The team must exist first (Constraint 4), so an
   operator makes it, then maps it and restarts.
4. A team is group-managed while the map names it. Removing it from the map and restarting makes it an ordinary
   team again, with the members it has then.

### At sign-in

5. Every sign-in through the provider reads the groups claim from the ID token, the first one included. A first
   sign-in keeps the groups on the pending sign-in and applies them in the same transaction that makes the user.
6. For each group-managed team, in a fixed order (by team ID, so two sign-ins never wait on each other's locks): the
   person's groups name it and they aren't a member → they are added as a member. The groups don't name it and they
   are a member → they are removed, whatever their role. Anything else changes nothing. Each team is locked as any
   membership change locks it (architecture.md, Rules that depend on state).
7. The session opens in the same transaction, so the person's first page already shows the teams their groups gave
   them.
8. The groups are unknown, and nothing changes, when the claim is missing, isn't a list of strings (a single string
   counts as a list of one), or the token says it overflowed. Entra ID leaves `groups` out and names it in
   `_claim_names` when a person is in too many groups; this is that case. An empty list is known: the person is in
   no groups and leaves every group-managed team. Unknown is logged at warn, once per sign-in, naming the reason,
   so an operator who forgot the claim can see why nothing happens.
9. A removal skips the last-admin rule (Constraint 5, decisions.md §41). A person removed as an admin who later
   comes back through a group comes back as a member.
10. Sign-in with a password changes no membership: a local account isn't in any provider group, and its teams stay
    as they are.

### The team

11. A group-managed team refuses an admin's add (`409 team_group_managed`) and every invite link: making one is
    refused the same way, and its links stop working at once, read as "off" on the invite page, without being
    revoked. They still end at their time, so a team taken out of the map later doesn't find old links working
    again for more than a week.
12. Its slug can't be changed, since the map names it. Its name can.
13. Admins can still make members admins and admins members, and the last-admin rule holds for those hand changes.
14. An admin can still remove a member, and a member can still leave. This is the way out for a local account that
    was in the team before it was mapped (rule 4 never removes one), and for a person who must lose access before
    their next sign-in. A person whose groups still name the team comes back at their next sign-in, and the page
    says so when removing.
15. Members added before the team was mapped stay until a sign-in shows they aren't in its groups.

### A team with no admin

16. A sync can leave a team with no admin. Its members see on the Team page "This team has no admin. Whoever runs
    this Upheap server can make one." Everything admins do waits for one; members keep working in its heaps.
17. The operator makes one with `upheap-server team make-admin <team slug> <username>`, which, like `user
    reset-link`, opens the database and runs no server. The person must be a member. It works on any shared team,
    group-managed or not, so it is also the way back from any team that lost its admins.

### Audit

18. A sync's add and removal are team membership entries, shown to admins like any: `GroupMemberAdded` with the
    groups that named the team, and `GroupMemberRemoved` with the role the person had. The actor is the person
    signing in, since an entry needs an account; the diff says `by: provider_groups`. Activity reads "Sam joined
    through the engineering group" and "Sam left: no longer in the engineering group" (with several groups, "in
    the engineering or sre group").
19. `make-admin` is a `MemberRoleChanged` entry with the person as actor and `made_by: operator`, as a reset link
    records the operator.
20. A removal by a group is not a removal by an admin: it doesn't stop a later join through an invite link once the
    team is no longer group-managed.

## 4. Interface

A mockup of the wide and phone Team page is approved with this design, before anything is built.

The Team page (ux.md, Teams and Team) of a group-managed team:

1. Under the Members heading, in place of the search and **Invite link**, one line for everyone in the team:
   "Members come from Acme SSO groups: engineering, sre." (the provider's label and the mapped groups, from the
   team's detail).
2. No working invite links list and no **Past links** while group-managed; their History stays in the team's
   Activity.
3. Member rows as today: role badge, Make admin or Make member, Remove. Removing a member whose sign-ins come
   through the provider says "They lose access to Launch and Support. If their groups still include this team,
   they come back at their next sign-in."
4. ⋯ on the team name has no Link name.
5. With no admin, the line in rule 16 sits under the Members heading for every member, above the groups line.
6. On the Teams list, the team's line adds "Groups" after its role: "Member · Groups · 12 members · 3 heaps".
7. On a phone the same lines sit in the same places; nothing new opens a sheet.

The API: the team's detail gains `managed_by_groups`, the list of mapped group values, or `null`; `add_member`,
`manage_invite_links` and `change_slug` leave `allowed_actions` for it. The invite page shows a link of a
group-managed team as ended: "This link no longer works. Acme Engineering's members come from its groups."

## 5. Out of scope

1. Mapping groups in the app.
2. Groups giving the admin role.
3. Making teams from groups.
4. Reading groups from the userinfo endpoint or Microsoft Graph when the ID token doesn't carry them (Entra ID's
   overflow stays unknown).
5. Removing a person between sign-ins (SCIM), and ending the sessions and CLI tokens of someone the provider took
   out. Until then, a person removed from every group keeps the teams they had until they sign in again.
