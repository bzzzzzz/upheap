# Signing in through a provider

Letting people sign in through an OpenID Connect provider (Okta, Microsoft Entra ID, Google Workspace, Keycloak),
making it the only way in, filling teams from the provider's groups, and having the provider push changes over
SCIM. The variables named here are listed with their defaults in [configuration.md](configuration.md).

## Setting up the provider

With the four `UPHEAP_OIDC_*` variables set, the sign-in and invite pages offer a button to sign in through the
provider, next to local accounts. Register this redirect URI at the provider, as a confidential web client using the
authorization code flow:

```
{BASE_URL}/api/v1/auth/oidc/callback
```

The provider must send a verified `email` claim (`email_verified: true`) in the ID token; a person without one is
refused. A first sign-in asks for a username and makes the account, also when `ALLOW_REGISTRATION` is `false`. A
person whose email already belongs to a local account is refused, not linked. People who sign in through the provider
have no password in Upheap. The server doesn't contact the provider at startup, so a provider that is down never
stops the server or local sign-in. Start with one setting missing and the server stops, naming it.

With `UPHEAP_OIDC_PROVIDER_LOGOUT=true`, every sign-out in the web app (Account, and "Not you?" on the invite and
consent pages) also ends the person's session at the provider (OpenID Connect RP-Initiated Logout), so the next person
on a shared machine can't sign in as them with one click. Register `{BASE_URL}/login` at the provider as a post-logout
redirect URI, where it sends the person afterwards. With the setting on, the ID token of a provider sign-in is kept on
the session row until the person signs out or their sessions are ended (a password change or reset); a session that only
expires keeps its row, and the token with it. A provider without logout (Google, say) and a provider that is down just
sign the person out of Upheap. The CLI's sign-out is unchanged.

## Provider only

Set `UPHEAP_PASSWORD_SIGN_IN=false` and the provider is the only way in: the sign-in and invite pages show just its
button. The server refuses password sign-in, sign-up (also through an invite link, and whatever `ALLOW_REGISTRATION`
says), asking for a password reset, setting a password with a reset link, and `upheap-server user reset-link`. Joining
a team by invite link as a provider user keeps working.

At every startup with the setting off, the server ends the sign-ins of every local account (one with no provider
identity): its web sessions, OAuth sign-ins and personal tokens. A sign-in an older instance still running with
passwords on makes later is refused on each request. Passwords stay in the database. A person who has a local account
and signs in through the provider with its email is refused, not linked, and is told to ask whoever runs the server.

If the provider is down nobody can sign in, and people already signed in through it stay signed in. To let local
accounts in again, set the variable back to `true` (or unset it) and restart. They sign in with their passwords, and
make tokens again, since their old sign-ins and tokens ended. People who only have the provider have no password and
wait for it.

## Teams from groups

The provider's groups can say who is in a team. Make the teams first (people make them in the app), then map each
team's slug to the groups whose members it has:

```sh
UPHEAP_OIDC_GROUP_TEAMS='{"ops": ["engineering", "sre"], "support": ["helpdesk"]}'
```

At each sign-in through the provider, the person joins the mapped teams their groups name, as members, and leaves
those their groups no longer name, whatever their role. Group names match exactly, capitals included. Groups give
membership only: admins are still made in the app. A mapped team takes no members added by hand and has no invite
links, and its link name can't change; it can still be renamed, and an admin can still remove a member (who comes back
at their next sign-in if their groups still name the team).

At the provider, send the person's groups in the ID token, under the claim `groups` or the one named by
`UPHEAP_OIDC_GROUPS_CLAIM`. For most providers that means a `groups` scope or a groups claim on the client: add the
scope to `UPHEAP_OIDC_SCOPES` where the provider needs it. A token that doesn't say, because the claim is missing or
is neither a list of strings nor one string, changes no membership and is logged as a warning. So does Microsoft Entra ID's overage, which
leaves the groups out of the token for a person in too many of them. An empty list is an answer: it takes the person out
of every mapped team. People who sign in with a password never change teams this way.

The server stops at startup, naming the setting or the slug, when the map is set without a provider, isn't a JSON
object of non-empty lists of strings, names a slug no team has, or names a personal team. If a sync takes a team's last
admin out, the team shows that it has no admin; see [Making a team admin](accounts.md#making-a-team-admin).

## Provisioning over SCIM

Groups arrive at sign-in, so a person taken out of a group, or out of the organisation, keeps their teams until they
sign in again. With SCIM the provider pushes its users, groups and deactivations to Upheap as they change. Set
`UPHEAP_SCIM_TOKEN` (the server stops at startup if it is shorter than 32 characters, or the provider isn't set up) and
give the provider:

- the base URL `{BASE_URL}/scim/v2`, and
- the token as a bearer token: `Authorization: Bearer <token>`. Rotating it is changing the setting and restarting.

Each user's `externalId` must be the person's OpenID Connect subject, the value of the ID token claim
`UPHEAP_OIDC_SUBJECT_CLAIM` names (`sub` by default): that is how a pushed user is matched to an account, never by email
or user name. A pushed person with no account yet gets their teams when they first sign in; a push never makes an
account. A pushed group matches a group in `UPHEAP_OIDC_GROUP_TEAMS` by its
`displayName` or its `externalId`, so the same map works whether the token carries names or IDs. List each group once, in the form the
provider pushes it: a team follows the pushes only once every group it lists has been pushed. Members join and leave
the mapped teams as the push arrives, with the same entries in the team's activity as at sign-in. Each mapped team is decided on
its own. A team is pushed once the provider has pushed every one of its groups. For a pushed team, a pushed person's
membership follows the groups the provider says, at each push and at sign-in instead of the ID token. A team with a group
not pushed yet is left alone by pushes, and a sign-in syncs it from the ID token as without SCIM. If the
provider deletes or renames one of a team's groups, that group's members leave the team unless another of its groups holds them, and it follows the ID token again. If the provider pushes several users with
one `externalId`, they are one person: deactivated only when all are, and in the groups of the active ones.

When the provider deactivates a person, or deletes them, they leave every mapped team and lose every sign-in at once:
web sessions, command-line sign-ins and personal tokens end. Their account, personal team and other teams stay. While
the provider keeps them deactivated they can't sign in to Upheap by any means, a password reset included; turning
them back on at the provider lets them sign in as usual.

The provider's pushes are stored, so a group mapped later already has its members.

### Okta

1. In the Okta admin console, go to Applications, Browse App Catalog, and add **SCIM 2.0 Test App (Header Auth)**, or
   create an app integration and enable SCIM provisioning on its General tab.
2. On the Provisioning tab, choose Integration and set SCIM connector base URL to `{BASE_URL}/scim/v2`, Unique
   identifier field for users to `userName`, and tick Push New Users, Push Profile Updates and Push Groups.
3. Set Authentication Mode to HTTP Header and paste the token as the Authorization header's value, with `Bearer ` in
   front. Use Test Connector Configuration to check it.
4. Under To App, enable Create Users, Update User Attributes and Deactivate Users.
5. In the app's attribute mappings (Profile Editor, Directory), map `externalId` to `user.id`, the same value as the
   `sub` of the ID token Okta sends Upheap. Leave `userName` as it is.
6. Assign the people, and on the Push Groups tab push the groups your `UPHEAP_OIDC_GROUP_TEAMS` names.

### Microsoft Entra ID

1. Register a single-tenant app for the sign-in, and set `UPHEAP_OIDC_SUBJECT_CLAIM=oid`. The issuer is
   `https://login.microsoftonline.com/<tenant-id>/v2.0`. Entra's `sub` is different for each app and SCIM can't carry
   it, but `oid`, the person's object ID, can be pushed. Until a person has signed in once through the provider after
   the switch, SCIM pushes don't reach their account, so ask everyone to sign in once.
2. In the Enterprise application, go to Provisioning, set the mode to Automatic, Tenant URL to `{BASE_URL}/scim/v2`
   and Secret Token to the token.
3. In the attribute mappings for users, map `externalId` to `objectId`: Entra's default maps it to `mailNickname`.
   Keep `userName` as it is, and keep the group mapping on. The groups claim must carry the IDs or names that
   `UPHEAP_OIDC_GROUP_TEAMS` lists.
4. Assign the people and the groups, and start provisioning.
