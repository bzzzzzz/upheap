# Mail

Setting up outgoing mail, checking it works, what it turns on (reset links by mail and confirmed addresses), and
limiting registration mail at the proxy. Mail is about accounts only; the server never mails about items.

## Setting it up

Mail is optional: without `MAIL_URL` and `MAIL_FROM` the server sends none. To set it up, pick any SMTP provider, set
up its sending domain (the SPF and DKIM records it gives you), and set `MAIL_URL` and `MAIL_FROM`. Use port 465
(`smtps://`) or 587 (`smtp://`, which always requires STARTTLS and never sends in cleartext): many hosts, Cloud Run
included, block port 25. Percent-encode special characters in the user and password (`@` as `%40`, `/` as `%2F`, `?`
as `%3F`, `#` as `%23`). For example, with Resend:

```sh
MAIL_URL='smtps://resend:<API key>@smtp.resend.com:465'
MAIL_FROM='Upheap <upheap@example.com>'
```

Restart the server and check that the `listening` line in its log has `mail` set to the scheme, host and port.
Then send a test mail:

```sh
upheap-server mail test you@example.com
docker compose exec upheap upheap-server mail test you@example.com
```

It takes the same environment variables as the server, and prints `Sent a test mail to … from … via …` on success. Exit
codes: `0` sent, `2` wrong arguments or not an email address, `6` mail isn't set up or the send failed, `1` anything
else.

For development, `MAIL_URL=log:` writes each mail to the server's log, and `MAIL_URL=file:///tmp/upheap-mail` writes
each as an `.eml` file in that directory, which must exist. Nothing is sent.

With mail set up, users can ask for a password reset link on the sign-in page ([accounts.md](accounts.md#resetting-a-password)), and a new account (open registration, or Create account through an invite link)
confirms its address before it can sign in: a link is mailed to it, and it works for a day. Signing in again with the
right password mails a fresh link, and a person who mistyped their address can correct it on the Check your mail
card, which mails the link to the new address. Accounts made before mail was set up, or while it was off, and provider accounts
count as confirmed; with mail off, nobody has to confirm. If the mail doesn't come, confirm an account by hand:

```sh
upheap-server user confirm sam          # a username or an email address
docker compose exec upheap upheap-server user confirm sam
```

It takes the same environment variables as the server (at least `DATABASE_URL` and `BASE_URL`), needs no mail, and prints
`Confirmed @sam's address, sam@example.com.`, or says the address is already confirmed. The confirmation is in the
user's account history. It runs no migrations and starts no server, so run it once the server of the same version has
started. Exit codes: `0` confirmed or already confirmed, `2` wrong arguments, `3` no such user, `5` the database can't
be reached, `1` anything else, a database a version behind included. A reset link, which also arrives by mail,
confirms the address too.

The server limits how many addresses an account can mail and how many mails an address gets (five in 15 minutes
each), but not registration: it mails each new address once, and the server can't tell one client from another behind a proxy. On a server open to
sign-ups, rate-limit `POST /api/v1/auth/register` per client at the reverse proxy, or set `ALLOW_REGISTRATION=false`
and let people in with invite links. With nginx, for example:

```nginx
limit_req_zone $binary_remote_addr zone=register:10m rate=1r/m;

location = /api/v1/auth/register {
    limit_req zone=register burst=5 nodelay;
    limit_req_status 429;
    proxy_pass http://upheap;
}
```
