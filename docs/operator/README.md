# Running Upheap

For whoever runs an Upheap server: from nothing to a running system, and what to set up next. One binary,
`upheap-server`, serves the API and the web app, on SQLite, MySQL or Postgres. Using Upheap once it runs is in the
[user's guide](../user/README.md).

## 1. Start it

Pick one of three ways. Each ends with the server on `http://localhost:8080`.

With Docker, on SQLite kept on a volume:

```sh
docker compose up --build
```

From a release, a static binary with the web app inside. It isn't in Homebrew; `brew install` is for the CLI. Pick
the target for your platform: `aarch64-apple-darwin` for macOS with Apple silicon, or `x86_64-unknown-linux-musl` or
`aarch64-unknown-linux-musl` for Linux:

```sh
target=x86_64-unknown-linux-musl
base=https://github.com/bzzzzzz/upheap/releases/latest/download
curl -fsSLO "$base/upheap-server-$target.tar.gz"
curl -fsSLO "$base/SHA256SUMS"
grep " upheap-server-$target.tar.gz\$" SHA256SUMS | sha256sum -c   # on macOS: shasum -a 256 -c
tar -xzf "upheap-server-$target.tar.gz" upheap-server
./upheap-server --version   # upheap-server <version>
DATABASE_URL="sqlite://heap.db?mode=rwc" BASE_URL=http://localhost:8080 ./upheap-server
```

From source (Rust 1.95+, Node 24+):

```sh
(cd web && npm ci && npm run build)
DATABASE_URL="sqlite://data/heap.db?mode=rwc" BASE_URL=http://localhost:8080 LOG_FORMAT=pretty \
  cargo run --release -p server
```

The server needs two settings, `DATABASE_URL` and `BASE_URL`. It runs its migrations when it starts.

## 2. Check it's up

`/healthz` answers `200` while the process is up, and `/readyz` answers `200` when the database answers, which is
only after the migrations ran. Point a startup or readiness probe at `/readyz`.

```sh
curl -i http://localhost:8080/readyz
```

Then open the address in a browser and create the first account. Registering makes the person's own team and a
first heap.

## 3. Put it in production

1. **TLS.** Put a TLS-terminating reverse proxy in front and set `BASE_URL` to its `https://` address. Cookies are
   `Secure` unless `BASE_URL` starts with `http://`.
2. **The database.** SQLite is enough for a small team. For more, point `DATABASE_URL` at Postgres
   (`postgres://user:pass@host/db`) or MySQL (`mysql://user:pass@host/db`). MySQL needs row-based or mixed binary
   logging (`binlog_format`, ROW by default), because SCIM user writes run at READ COMMITTED.
3. **Sign-ups.** Registration is open by default. Set `ALLOW_REGISTRATION=false` once your people are in; a team
   admin's invite link still lets a new person create an account.

## 4. Set up what you need

| To | Read |
| --- | --- |
| See every setting and its default | [configuration.md](configuration.md) |
| Let people sign in through Okta, Entra ID or another OpenID Connect provider, fill teams from its groups, or have it push changes over SCIM | [sign-in.md](sign-in.md) |
| Send mail, so people reset their own passwords and confirm their addresses | [mail.md](mail.md) |
| Make a reset link, give a team an admin, or become an operator with the Accounts page | [accounts.md](accounts.md) |
| See how we run our own production on Cloud Run with Neon, as a worked example | [operations.md](operations.md) |
