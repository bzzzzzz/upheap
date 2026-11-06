# Offering structure

Companion to [requirements.md](requirements.md).

## Distribution

1. Upheap is free and open source.
2. Teams self-host it. There is no hosted or paid tier.

## Architecture

1. One backend is the source of truth for all data.
2. The backend exposes an API that any client can use, for example a CLI, a native app or a web app.
3. For now the web app is the only planned client. The API should not assume it is the only one.
4. The backend supports more than one database. MySQL is the minimum for production deployments, and SQLite is supported for local and test deployments. Other databases may be added later, so the data layer should not depend on features specific to one database.

## Authentication

1. Users can sign in through a third-party identity provider, for example Okta.
2. The app can also run self-contained with its own accounts, without any external provider.
3. Anyone can register. Signing in through an identity provider for the first time creates the user record.
4. Non-web clients such as a CLI sign in through the browser: the client opens the app's sign-in page, the user signs in there, and the backend hands a token back to the client (OAuth authorization code flow with PKCE). The client never handles the user's password.

## Teams and heaps

1. Users form teams. A team has one or more admins.
2. Any user can create a team and becomes its admin.
3. Every new user gets a team and a heap named after them. A personal heap is simply a heap in a one-person team. It works like any other team and heap.
4. A team owns its heaps. A team can have more than one heap.
5. A heap belongs to exactly one team, and only that team's members can access it.
6. Users join a team only when an admin adds them. For now the admin picks an existing user, who is added directly. Invitations by email or link, and membership based on identity-provider groups, are planned for a later iteration.
7. A team admin can make any other member an admin, remove admin rights from another admin, and remove another admin from the team.
8. A team always has at least one admin. The last admin can't leave, be demoted or be removed until another member has been made admin.
9. Heaps are never deleted. Admins can archive a heap, and its audit history stays. An archived heap is read-only, and an admin can unarchive it.

## Nothing is deleted

1. Items, tags and heaps can't be deleted.
2. An item that won't be done is closed with the **won't do** state instead.
3. Tags can be renamed and their descriptions edited, but not deleted. A tag can be taken off an item, which is recorded in the audit log like any other item edit.
4. Heaps are archived instead of removed.
5. Custom parameters and their options are archived instead of removed:
   - Archiving an option requires the admin to pick a replacement option. Items in a non-terminal state (open, in progress, blocked) switch to the replacement. Items in a terminal state (done, won't do) keep the archived option.
   - A whole parameter can be archived only when no non-terminal item uses it. Terminal items keep their archived values.

## Roles and permissions

| Action | Member | Team admin |
| --- | :---: | :---: |
| Register and create teams | ✓ | ✓ |
| List own teams and heaps | ✓ | ✓ |
| Leave a team | ✓ | ✓ (not the last admin) |
| Add and edit items | ✓ | ✓ |
| Add and edit tags | ✓ | ✓ |
| Save, change and archive a heap's views | ✓ | ✓ |
| Rename the team | | ✓ |
| Add users to the team and remove them | | ✓ |
| Make a member an admin, demote an admin | | ✓ |
| Create, rename, archive and unarchive heaps | | ✓ |
| Configure a heap (parameters, weights, ranking policy) | | ✓ |
| Archive parameters and options | | ✓ |

## Audit log

1. Every action by a user or an admin is recorded in an audit log.
2. Each entry shows who made the change and what changed.
3. Entries can't be edited or deleted, and they are kept forever.
4. Users see audit entries on the page where the action happens, for every kind of action they are allowed to do. For example, every member sees item and tag changes on the heap page, and only admins see team membership changes and heap configuration changes.

