# Accounts

What whoever runs the server does for people: reset links, a team with no admin, and the operator role with the
Accounts page and the commands behind it. Every command here takes the same environment variables as the server
([configuration.md](configuration.md)).

## Resetting a password

With mail set up, a user who forgot their password asks for a reset link on the sign-in page and gets it by mail. Without
mail, or when the mail doesn't come, whoever runs the server makes a one-time reset link for them with a command; the
command works with mail too. The user opens the link, sets a new password and is signed in.
The command takes the same environment variables as the server (at least `DATABASE_URL` and `BASE_URL`):

```sh
upheap-server user reset-link sam          # a username or an email address
docker compose exec upheap upheap-server user reset-link sam
```

It prints the link, `{BASE_URL}/reset/…`, alone on stdout, and on stderr whom it is for and when it ends (it works
once, for 60 minutes). A newer link for the same user cancels the earlier one. It runs no migrations and starts no
server, so run it once the server of the same version has started. Exit codes: `0` made, `2` wrong arguments, `3` no
such user, `5` the database can't be reached, `1` anything else, a database a version behind included.

## Making a team admin

A team whose groups took out its last admin has nobody who can make another. Whoever runs the server makes a member an
admin with a command, which takes the same environment variables as the server (at least `DATABASE_URL` and `BASE_URL`):

```sh
upheap-server team make-admin ops sam          # the team's link name, a username
docker compose exec upheap upheap-server team make-admin ops sam
```

It prints `@sam is now an admin of Ops.`, or says they already are. The person must be a member of the team. The
change is in the team's Activity, marked as made by whoever runs the server. It runs no migrations and starts no
server, so run it once the server of the same version has started. Exit codes: `0` done, `2` wrong arguments, `3` no
such team, no such user or not a member (the message says which), `5` the database can't be reached, `1` anything
else, a database a version behind included.

## Operators and accounts

An operator of the server has the **Accounts** page in the web app: every account with its state and last sign-in, and
for a local account Deactivate, Reactivate and Make reset link, and each account's History. Only the server's shell
makes an operator, so nobody can grant the role from the web:

```sh
upheap-server user make-operator sam          # a username or an email address
upheap-server user remove-operator sam
docker compose exec upheap upheap-server user make-operator sam
```

They print `@sam is now an operator.` or `@sam is no longer an operator.`, or say it was so already and change nothing.
Taking the role away takes effect on the person's next request. The same commands list and turn accounts off and on,
which the page does too:

```sh
upheap-server user list                       # one tab-separated line for each account, with a header
upheap-server user deactivate sam             # a local account only
upheap-server user reactivate sam
```

`user list` prints the username, name, email, `local` or `provider`, `confirmed` or `unconfirmed`, `active` or
`deactivated`, `operator` for an operator, and the last sign-in in UTC or `never`. `user deactivate` ends every sign-in
of the account (web sessions, command-line sign-ins and personal tokens), keeps its items, teams and memberships, and
prints `Deactivated @sam. 3 sign-ins ended.`; the person then can't sign in until `user reactivate`, which restores
nothing that ended. An account that signs in through the provider is managed by the provider, so both refuse it. Every
change is in the account's history. The commands take the same environment variables as the server (at least
`DATABASE_URL` and `BASE_URL`), run no migrations and start no server, so run them once the server of the same version has started.
Exit codes: `0` done, or already so, `2` wrong arguments, `3` no such user, `4` the account signs in through the
provider (`deactivate` and `reactivate`), `5` the database can't be reached, `1` anything else, a database a version
behind included.
