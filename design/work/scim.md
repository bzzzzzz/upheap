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
2. A person the provider deactivates or deletes leaves every group-managed team at once. Their account, personal
   team and other teams stay.
3. Teams §9 holds unchanged: the operator maps groups to existing teams, groups give membership only, a
   group-managed team takes no hand-added members, and the last admin is removed like anyone else (decisions.md §41).
4. Nothing the provider pushes makes an Upheap account. A pushed person who has never signed in gets their teams at
   their first sign-in.

## Out of this design

- No account changes beyond team membership: a deactivated person is not signed out, and can still sign in if the
  provider lets them (it normally doesn't). Whether deactivation should also end their sessions and tokens is a
  separate change.
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
SHA-256 of the setting in constant time. A missing or wrong token is `401` in SCIM's error shape. The token is never
logged. The SCIM token is not a user: it reaches only the SCIM endpoints, and `/api/v1` refuses it like any unknown
bearer token.

### The endpoints

At `/scim/v2`, outside `/api/v1`, in SCIM's JSON (`application/scim+json`) and SCIM's error shape, and not in
`openapi.json`: the RFCs are the contract, and providers are the only clients (engineering-principles.md, API,
Errors §4, "standards keep their own format").

| Path | Methods |
| --- | --- |
| `/ServiceProviderConfig`, `/ResourceTypes`, `/Schemas` | `GET`: what is supported, so providers that read it skip the rest |
| `/Users` | `GET` with `filter` (`userName eq`, `externalId eq`), `startIndex`, `count`; `POST` |
| `/Users/{id}` | `GET`, `PUT`, `PATCH`, `DELETE` |
| `/Groups` | `GET` with `filter` (`displayName eq`, `externalId eq`), `startIndex`, `count`, `excludedAttributes=members`; `POST` |
| `/Groups/{id}` | `GET`, `PUT`, `PATCH`, `DELETE` |

- `PATCH` takes `add`, `remove` and `replace` on `active`, `displayName`, `externalId`, `userName` and `members`,
  including a path filter on members (`members[value eq "…"]`), and the value-object forms Okta and Entra send (a
  `replace` with no path and an object value; `active` as `"True"`/`"False"`).
- A `POST` whose `userName`, or group `displayName`, is already taken is `409` with `scimType: uniqueness`, which is
  how providers find and link an existing resource.
- A resource keeps what the provider sent, so a `GET` gives it back with Upheap's `id` and `meta`. Attributes
  Upheap doesn't use are kept as they came and not read.
- Users and groups the map doesn't name are kept like the rest, so a group mapped later already has its members,
  and nothing about them changes any team.

### What is stored

Three tables mirror the provider. They are the provider's data, not the person's, so a `DELETE` removes the rows; the
audit log keeps every team change they caused.

- `scim_users`: Upheap's `id`, `userName` (unique), `externalId` (unique when present), `active`, the resource as
  sent, and timestamps.
- `scim_groups`: `id`, `displayName` (unique), `externalId`, the resource as sent, and timestamps.
- `scim_group_members`: group and user, by Upheap's SCIM IDs. A member that names no stored user is refused with
  `400` (`scimType: invalidValue`).

A SCIM user is linked to an Upheap user by `user_identities`: the configured issuer and the SCIM user's `externalId`
as the subject. Nothing stores the link; it is looked up each time, so a person who signs in for the first time is
linked from then on. Never by email or `userName` (architecture.md, matching is by issuer and subject).

### Applying a change

A write that can change someone's groups (a user's `active` or `externalId`, a group's members, `displayName` or
`externalId`, a delete) runs in one transaction with the team changes it causes. For each SCIM user it touches who
is linked to an Upheap user, it works out their groups and runs the same sync a sign-in runs
(`sync_group_teams`), with the person as the audit entry's actor and `provider_groups` as what made the change, as
at sign-in:

- An active user's groups are the `displayName` and the `externalId` of every stored group they are in.
- An inactive or deleted user's groups are known and empty: they leave every group-managed team.
- A user with no `externalId`, or one no identity has, changes no team. The server logs it once at `info`.

Lock order: the SCIM group or user row being written, then the mapped teams in ID order, as the sync already locks
them. A group write that touches many members syncs each in the same transaction.

### Sign-in

With a SCIM token set and a SCIM user linked to the person, a sign-in through the provider syncs from the SCIM
record instead of the ID token: the provider's pushes are the one source, so a token minted a moment before a push
can't undo it. A person with no SCIM record syncs from the token's groups as now. The first sign-in (the username
step) does the same once the identity exists, so a person pushed before they ever signed in gets their teams then.

### What the operator sees

- README: the setting, the base URL, `externalId` as the subject, and a walk-through for Okta.
- Logs at `info` for each team change, as at sign-in, and at `warn` for a refused SCIM request, with its status and
  `scimType`, never the token.
- No new screens. The Team page already says a team's members come from groups; the Activity entries are the
  existing "joined through their groups" and "left: no longer in their groups".

## Checking it

- Service and API tests for each endpoint, filter and `PATCH` form, using the request bodies Okta and Entra document.
- Tests that a group push adds and removes, that deactivation and delete empty the person's group-managed teams,
  that an unlinked or unmapped user changes nothing, and that sign-in prefers the SCIM record.
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
