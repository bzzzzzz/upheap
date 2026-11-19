# SCIM provisioning: design

The identity provider pushes changes to its groups and its people to Upheap over SCIM 2.0, so a group-managed team
(product.md, Teams §9) gains and loses members when the provider changes, not only when the person next signs in.

## Why

A sign-in is the only moment Upheap learns someone's groups today. A person taken out of a group, or out of the
organisation, keeps their group-managed teams until they sign in again, which someone who left never does. SCIM
(RFC 7643, RFC 7644) is how Okta, Entra ID and the others push such changes to an app: the provider calls the app's
SCIM endpoints with a token, creating, changing and deactivating users and groups as they change at the provider.

## What it promises

1. With provisioning set up, a change at the provider to who is in a mapped group adds the person to the team or
   takes them out at once, the way a sign-in would, with the same audit entries.
2. A person the provider deactivates or deletes leaves every group-managed team at once, and loses every session and
   token. Until the provider turns them back on they can't sign in. Their account, personal team and other teams stay.
3. Teams §9 holds unchanged: the operator maps groups to existing teams, groups give membership only, a
   group-managed team takes no hand-added members, and the last admin is removed like anyone else (decisions.md §41).
4. Nothing the provider pushes makes an Upheap account. A pushed person who has never signed in gets their teams at
   their first sign-in.

## Out of this design

- No in-app setup: the token and the map are server settings, as the map is now.
- No SCIM bulk requests, sorting, ETags, password changes or custom schemas.
- Providers whose OIDC subject a SCIM push can't carry. Entra ID's `sub` is pairwise per app and no SCIM attribute
  holds it, so Entra needs Upheap to match sign-ins on another claim (`oid`) first; this design doesn't do that.

## How it works

### Setting it up

- `UPHEAP_SCIM_TOKEN`: the bearer token the provider sends, at least 32 characters, chosen by the operator (README
  says how to make one). The server stops at startup if it's set without the OIDC provider, or is too short. Unset,
  the SCIM endpoints answer `404` as any unknown path does. Rotating it is changing the setting and restarting.
- The SCIM base URL to give the provider is `{BASE_URL}/scim/v2`.
- At the provider, the SCIM user's `externalId` must be the person's OIDC subject: the `sub` of their ID token
  (Okta: the user's ID, `user.id`, mapped to `externalId` in the app's attribute mappings).
- The map stays `UPHEAP_OIDC_GROUP_TEAMS`. A SCIM group matches a configured group when its `displayName` or its
  `externalId` equals it, so the same map works whether the provider's claim carries names or IDs.

### Authentication

Every SCIM request needs `Authorization: Bearer <token>`. The server compares the SHA-256 of what it got with the
SHA-256 of the setting; digests, not tokens, are compared. A missing or wrong token is `401` in SCIM's error shape. The token is never
logged. The SCIM token is not a user: it reaches only the SCIM endpoints, and `/api/v1` refuses it like any unknown
bearer token.

### The endpoints

At `/scim/v2`, outside `/api/v1`, in SCIM's JSON (`application/scim+json`) and SCIM's error shape, and not in
`openapi.json`: the RFCs are the contract, and providers are the only clients (engineering-principles.md, API,
Errors §4, "standards keep their own format").

| Path | Methods |
| --- | --- |
| `/ServiceProviderConfig` | `GET`: what is supported, for providers that read it. There is no `/ResourceTypes` or `/Schemas`: Okta and Entra don't read them |
| `/Users` | `GET` with `filter` (`userName eq`, `externalId eq`), `startIndex`, `count`; `POST` |
| `/Users/{id}` | `GET`, `PUT`, `PATCH`, `DELETE` |
| `/Groups` | `GET` with `filter` (`displayName eq`, `externalId eq`), `startIndex`, `count`; `POST` |
| `/Groups/{id}` | `GET`, `PUT`, `PATCH`, `DELETE` |

- `PATCH` takes `add`, `remove` and `replace` on `active`, `displayName`, `externalId`, `userName` and `members`,
  including a path filter on members (`members[value eq "…"]`), and Okta's value-object form (a `replace` with no
  path and an object value). Any other path, or key of such an object, is accepted and changes nothing; so does
  removing `userName` or `displayName`. `active` is a boolean. The resource as sent on `POST` and `PUT` is still kept
  whole.
- A `POST` whose `userName`, or group `displayName`, is already taken is `409` with `scimType: uniqueness`, which is
  how providers find and link an existing resource. `externalId` isn't unique.
- A group's `members` come back as `{"value": <user id>}` only. `attributes` and `excludedAttributes` are ignored.
- A resource keeps what the provider sent, so a `GET` gives it back with Upheap's `id` and `meta`. Attributes
  Upheap doesn't use are kept as they came and not read.
- Users and groups the map doesn't name are kept like the rest, so a group mapped later already has its members,
  and nothing about them changes any team.

### What is stored

Three tables mirror the provider. They are the provider's data, not the person's, so a `DELETE` removes the rows; the
audit log keeps every team change they caused.

- `scim_users`: Upheap's `id`, `userName` (unique), `externalId` (indexed), `active`, the resource as
  sent, and timestamps.
- `scim_groups`: `id`, `displayName` (unique), `externalId`, the resource as sent, and timestamps.
- `scim_group_members`: group and user, by Upheap's SCIM IDs. A member that names no stored user is refused with
  `400` (`scimType: invalidValue`).

A SCIM user is linked to an Upheap user by `user_identities`: the configured issuer and the SCIM user's `externalId`
as the subject. Nothing stores the link; it is looked up each time, so a person who signs in for the first time is
linked from then on. Never by email or `userName` (architecture.md, matching is by issuer and subject).

Several SCIM users may share an `externalId` (it isn't unique), and together they are one person: one rule reads their
state from all the records, for a push, a sign-in and the refusal alike. The person is inactive only when every record
is inactive; their groups are those of the active records together; they have SCIM records when at least one exists.
After a user write the person's state is read again from all their records, not taken from the row written, and so is
the state of the person a record belonged to before a write changed its `externalId`.

### Applying a change

A write that can change someone's groups (a user's `active` or `externalId`, a group's members, `displayName` or
`externalId`, a delete) runs in one transaction with the team changes it causes. For each SCIM user it touches who
is linked to an Upheap user, it works out their groups and runs the same sync a sign-in runs
(`sync_group_teams`), with the person as the audit entry's actor and `provider_groups` as what made the change, as
at sign-in:

- Each mapped team is decided on its own. A team is pushed when every one of its groups matches a stored group (the
  group's `displayName` or `externalId` is it). An active person's membership in a pushed team follows
  their groups: the `displayName` and the `externalId` of every stored group their active records are in. A team
  that isn't pushed is left alone by pushes, so pushing the group of one team never empties another, pushing a
  team's groups one at a time doesn't empty it, and pushing people before their groups doesn't empty any team.
- The write that makes a team pushed for the first time syncs every pushed person for that team. The write that
  makes a team stop being pushed (deleting one of its matching groups, or renaming it or changing its `externalId`
  away from the map) syncs the people it touched for that team with their remaining groups as known, so those in none
  of its groups leave it, and from then on it follows the token.
- An inactive person's groups, and those of a person whose last record was deleted, are known and empty in every
  mapped team, pushed or not: they leave every group-managed team.
- A user with no `externalId`, or one no identity has, changes no team. The server logs the second at `info`.

Lock order: the mapped teams in ID order, as the sync already locks them (the caller locks them before it syncs), then
the SCIM row being written, with a locking read, so two writes of one resource run one after the other. A group write
that touches many members syncs each in the same transaction.

### Deactivation

A write that leaves a linked person inactive (every record's `active` false, by `POST`, `PUT` or `PATCH`), or deletes
their last record, ends
every sign-in of their account in the same transaction: their web sessions are deleted, their OAuth token families
revoked, their authorization codes expired and their personal tokens revoked. The write reads whether the person
was off before it (they have records and every one is inactive). A change from on to off records `account_deactivated`
in the person's personal team, with the number of sign-ins ended; from off to on, `account_reactivated`. A repeated
push ends nothing more and records nothing. It is logged at `info` with the number of sign-ins ended.

The refusal sits where every sign-in, registration and password reset opens a session: while SCIM is set up and the
person is inactive, opening one fails with `account_deactivated` (`403`) and the surrounding
transaction rolls back, so a password reset then changes nothing. The provider itself normally refuses a deactivated
person's sign-in, so this is the backstop; a password reset can give a provider-made account a password, which is why
it is there. Reactivating is `active` true again, which records `account_reactivated` and nothing else. A person with no record is
no longer refused; the provider won't authenticate a person it deleted, and their sessions and tokens ended at the
delete.

### Sign-in

With a SCIM token set and the person having a SCIM record, a sign-in through the provider syncs the pushed teams from
the SCIM state instead of the ID token: the provider's pushes are the one source, so a token minted a moment before a push
can't undo it. For a mapped team that is not pushed, and for a person with no SCIM record, the sign-in syncs from the
token's groups as now. The first sign-in (the username
step) does the same once the identity exists, so a person pushed before they ever signed in gets their teams then.

### What the operator sees

- README: the setting, the base URL, `externalId` as the subject, and a walk-through for Okta.
- Logs at `info` for each team change, as at sign-in, and at `warn` for a refused SCIM request, with its status and
  `scimType`, never the token.
- No new screens. The Team page already says a team's members come from groups; the Activity entries are the
  existing "joined through their groups" and "left: no longer in their groups".

## Checking it

- Service and API tests for each endpoint, filter and `PATCH` form, using the request bodies Okta documents.
- Tests that a group push adds and removes, that deactivation and delete empty the person's group-managed teams,
  that an unlinked or unmapped user changes nothing, that sign-in prefers the SCIM record, and that a turn-off and
  a turn-on record their `account_deactivated` and `account_reactivated` entries once, and a repeat records none.
- Against a real provider: Okta's free developer org, with a SCIM 2.0 app integration pointed at a server it can
  reach, pushing a user, a group, a membership change and a deactivation.

## Choices

- **The provider's pushes win over the token at sign-in.** Instead of: applying both, each when it arrives. Two
  sources make the last word depend on timing, and the push is the one the provider sends when something changes.
- **`externalId` carries the OIDC subject, fixed.** Instead of: a setting that names the attribute. Every provider
  that can push the subject can put it in `externalId`, so a setting would only add a way to get it wrong.
- **A group matches by `displayName` or `externalId`.** Instead of: a second map for SCIM. The operator writes one
  map, in the words their ID token already uses.
- **SCIM at `/scim/v2` in the RFC's format, outside the OpenAPI spec.** Instead of: under `/api/v1` in Upheap's own
  shapes. Providers speak only SCIM, and the URL is set at the provider, so moving it later breaks every setup.
