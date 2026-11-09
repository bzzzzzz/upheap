# CLI work tracker

Tracks the build of the `upheap` command-line client against [cli.md](cli.md) (commands, output, errors, layout).
[progress.md](progress.md) tracks the product as a whole. Last reviewed 2026-11-08.

Status: ✅ done · 🟡 partial · ⬜ not started.

## Steps

| Step | What | Status | Notes |
| --- | --- | :---: | --- |
| 1 | Server: OAuth with PKCE, bearer tokens, the consent page | ✅ | `GET/POST /oauth/authorize` for the consent page, `POST /oauth/token` and `POST /oauth/revoke` for the client (form-encoded, RFC error format, no CSRF header). One client, `upheap-cli`, with `http://127.0.0.1` and `http://[::1]` redirect URIs on any port. Codes last 10 minutes and work once; access tokens 15 minutes; refresh tokens 30 days, rotated on every use, and a reused one revokes its family; revoking is one row of `oauth_token_families`, which a refresh locks. `Authorization: Bearer` resolves to the same `Principal` as a session cookie and needs no CSRF header. The web app's `/oauth/authorize` page signs in first when needed, then asks Allow or Cancel, with Use another account. The web app can't be framed (`X-Frame-Options`, CSP `frame-ancestors`). Tables `oauth_auth_codes`, `oauth_token_families` and `oauth_tokens` |
| 2 | `crates/api-types`: move the DTOs out of `api` | ⬜ | No behaviour change; its own pull request |
| 3 | `crates/cli`: config, HTTP client, `upheap auth login` / `auth logout`, `upheap heap list` / `heap use`, `upheap item list` | ⬜ | Login opens the browser on `/oauth/authorize` and listens on a loopback port; logout calls `/oauth/revoke` |
| 4 | `upheap item add`, `show`, `edit` and the state verbs | ⬜ | |
| 5 | `upheap tag list`, `add`, `edit` | ⬜ | |
| 6 | `/me/tokens`: list and revoke the CLI's tokens, server and the Account page | ⬜ | One row per sign-in: `oauth_token_families` |

## Open questions

1. **Old rows.** Used codes and expired or revoked tokens stay in their tables. They are authentication plumbing,
   which may be deleted ([engineering principles](engineering-principles.md#data)), so a small cleanup on each
   exchange, as sign-in attempts have, can come with `/me/tokens`.
