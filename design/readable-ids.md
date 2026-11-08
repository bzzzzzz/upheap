# Readable IDs

Built. UUIDs are good keys but bad names: nobody can read `/heaps/0196f3a2-…/items/0196f3b8-…` or type it into a
terminal. This change gives every team, user, heap and item a short handle, so a link reads
`/teams/my-team/heaps/backlog/items/k3x9a` and the CLI takes `upheap done k3x9a`.

UUIDs stay as the database keys, and every foreign key keeps pointing at them. Handles are extra columns with unique
indexes, used in URLs, API paths and the CLI.

## The handles

| Thing | Handle | Unique within | Example | Who sets it |
| --- | --- | --- | --- | --- |
| User | username | everyone, shared with teams | `eugene` | the user, once, at sign-up |
| Team | slug | everyone, shared with users | `my-team` | admins; derived from the name if not given |
| Heap | slug | its team | `backlog` | admins; derived from the name if not given |
| Item | key | its heap | `k3x9a` | the server, at random |

Display names, team names and heap names stay as they are, free text that can repeat. Only handles are unique.

## Usernames and team slugs

Usernames and team slugs share one namespace, so `/teams/eugene` is always Eugene's personal team and never someone
else's team.

The simplest way to get that is to make the username *be* the slug of the user's personal team. A personal team has
`personal_user_id` set to its owner, and the user's username is read from that team. One unique index on `teams.slug`
then covers both, with no cross-table check, and a unique index on `personal_user_id` keeps it to one personal team
per user. The link sits on the team so a team knows it's personal without another query.

Usernames never change, so the personal team's slug doesn't either. The personal team is private to its owner:

1. It has exactly one member, the owner. Adding a member, removing one and leaving are refused with `409`
   `personal_team`, and its `allowed_actions` don't offer them.
2. Its slug can't be edited. Its name can, and it holds as many heaps as the owner likes.
3. Anything shared lives in another team.
4. Finding users by name or email (`GET /users`, for adding members) needs an admin role in a shared team. Everyone
   is admin of their own personal team, so that alone no longer counts.

## Format

Usernames, team slugs and heap slugs:

1. Lowercase letters, digits and single dashes, starting with a letter and not ending with a dash: `my-team`,
   `infra-2027`. Two to 39 characters.
2. Input is checked, never fixed silently: `My-Team` is `422` `invalid_handle`, with a hint showing `my-team`. When a
   team or heap is created without a slug, the server derives one from the name ("My Team" → `my-team`) and adds
   `-2`, `-3`… if it's taken. Two creates of one name at once can both find the same slug free; the one the unique
   index refuses starts again and takes the next suffix, so only a slug the caller chose is ever `handle_taken`.
3. A taken handle is `409` `handle_taken`.

No names are reserved. Every handle sits after a fixed word in the URL (`teams/`, `heaps/`, `items/`), so it can't
clash with a page the app has now or adds later (see [URLs and the API](#urls-and-the-api)).

Usernames and item keys are fixed. Team and heap slugs can change, and an old link stops working when they do.
Redirects from old handles are out of scope for now.

## Item keys

Five characters from Crockford's base32 alphabet in lowercase (`0-9` and `a-z` without `i`, `l`, `o`, `u`), with at
least one digit. That's about 30 million keys per heap.

1. **Random,** so keys say nothing about when an item was made or how many a heap has.
2. **Easy to read aloud and type:** no `0`/`o` or `1`/`l` mix-ups, and leaving out `u` and requiring a digit keeps
   real words out.
3. **Collisions are handled by retrying.** The key is generated inside the item's insert transaction, under the heap
   lock every item write already takes, and checked against a unique index on `(heap_id, key)`. With 5,000 items
   in a heap a new key clashes about once in 5,000 inserts, so a retry is rare and one is almost always enough.
4. **Fixed for life.** Items are never deleted, so a key is never reused either.

This is the same idea as Nano ID with a custom alphabet. It's a few lines in `domain`, so we don't need the crate.
`domain` turns random bytes into a key, and the service draws the bytes from a `RandomSource` it's given, like its
clock: the system's random source in the server and the local build, and scripted bytes in tests that need to know
the keys. Session tokens and password salts don't use it; they always come from a generator the system seeds.

## URLs and the API

Every handle comes after a word saying what it is, and every page of a thing sits next to its handle:

```
/teams/my-team                              the team
/teams/my-team/activity                     a team page
/teams/my-team/heaps/backlog                the heap's list
/teams/my-team/heaps/backlog/tags           a heap page (also parameters, activity, closed)
/teams/my-team/heaps/backlog/items/k3x9a    an item, in the pane over the list
```

A slug can never be mistaken for a page, so nothing has to be reserved, and new pages can be added anywhere without
checking existing names. Teams start at `/teams/` rather than at the root for the same reason: the root also holds
the app's own pages (`/login`, `/account`) and the server's paths (`/api`, `/healthz`, `/readyz`, `/oauth`), and a
team called `api` would break them.

API paths are the same, under `/api/v1`: `/teams/{team}/heaps/{heap}/items/{item}`, and so on for tags, views,
parameters and audit. One shape for both means a web URL turns into its API call by adding the prefix. The flat UUID
routes (`/heaps/{id}`, `/items/{id}`) go away. Nothing has been released, so this changes in place instead of adding
a `v2`.

Responses keep the UUID `id` and add the handle (`username`, `slug`, `key`), so a client that wants a reference that
survives renames still has one. Request bodies keep using IDs (`tag_ids`, `blocked_by_item_id`), since clients have
the objects in hand when they send them.

### Short links

The full URL is easy to read but long to paste in chat, so `/h/` gives a short form with the handles alone:

```
/h/my-team                       → /teams/my-team
/h/my-team/backlog               → /teams/my-team/heaps/backlog
/h/my-team/backlog/k3x9a         → /teams/my-team/heaps/backlog/items/k3x9a
```

Each position means one thing (team, then heap, then item), so the short form needs no reserved names either. The web
app's router replaces a short link with the full URL as it opens, so it works the same in the local build, which has
no server to redirect. Items, heaps and teams offer "Copy link" with the short form. The API has no short form.

The CLI takes an item as its key within the default heap (`k3x9a`) or as a full path
(`my-team/backlog/k3x9a`), which is a short link without its `/h/`, so a pasted link works too. That
replaces the cached row numbers in [cli.md](cli.md) and settles its first open question.

## Sign-up and the local build

Registration asks for a username alongside the display name, email and password. The personal team gets it as its
slug, and the first heap is `personal`.

The local build's one user is `me`, with team `me` and a first heap named "Personal" (`personal`), so
its links read `/upheap/teams/me/heaps/personal/items/k3x9a`.

OIDC isn't built yet. When it is, a first sign-in asks for a username before creating the user.

## Migration

One forward migration, safe on a populated database, the local build's included:

1. Add the columns, nullable at first.
2. Fill them:
   - Teams and heaps get slugs derived from their names, with `-2`, `-3`… suffixes where they clash.
   - Each user's personal team is the team created in the same transaction as the user, which is the one with the
     user's `created_at`. If they've left it, or added anyone to it, it stays as a normal team and they get a new
     empty personal team instead.
   - Usernames are the personal team's slug.
   - Items get random keys.
3. Make the columns required and add the unique indexes. On SQLite the columns stay nullable, since making one
   `NOT NULL` there means rebuilding the table with its foreign keys; the service always sets them.

Usernames made this way are permanent like any other. Nothing has been released, so the only accounts are our own,
and a bad one can be fixed by hand before the migration ships.

## Order of work

All done.

1. `domain`: the handle type with its format, item key generation, the personal team rules.
2. `store`: the migration, the columns, lookups by handle.
3. `service` and `api`: handles on create and edit, the new paths, `handle_taken`, `invalid_handle`, `personal_team`.
4. `web`: the new routes, a username field on sign-up, slug fields on team and heap settings, and no member controls
   on a personal team.
5. [cli.md](cli.md): item keys in place of row numbers.
6. [offering.md](offering.md) Teams §3 says a personal team "works like any other team"; it now can't be shared.
   [technical-design.md](technical-design.md) gets the handles in its data model and API sections.

The access-control table test covers the new paths as they replace the old ones.
