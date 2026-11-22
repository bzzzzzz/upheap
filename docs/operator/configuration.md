# Configuration

Every setting of `upheap-server`, once: an environment variable each, with its default and what it means. The
server commands (`upheap-server user …`, `team …`, `mail test`) read the same variables. How to set up what the
longer ones turn on is in [sign-in.md](sign-in.md) and [mail.md](mail.md); our own production's values are in
[operations.md](operations.md#production-setup).

| Variable | Default | Meaning |
| --- | --- | --- |
| `DATABASE_URL` | required | `sqlite://path?mode=rwc`, `mysql://…` or `postgres://…` |
| `DATABASE_MAX_CONNECTIONS` | `5` | Most database connections the server keeps open. Size it to the database's limit divided by the instances that may run. In-memory SQLite always keeps one |
| `DATABASE_CONNECT_DEADLINE_SECS` | `45` | How long startup keeps retrying the database, with backoff from 1 s to 8 s, before the server exits with the last error. `0` tries once. The last try can start just before the deadline and take 10 s, so keep it that much and the migrations' time under the startup probe's limit |
| `BASE_URL` | required | Public URL. Cookies are `Secure` unless it starts with `http://` |
| `BIND_ADDR` | `0.0.0.0:8080` | Listen address |
| `ALLOW_REGISTRATION` | `true` | Set to `false` to close sign-ups; a team admin's invite link still lets a new person create an account |
| `UPHEAP_PASSWORD_SIGN_IN` | `true` | Set to `false` to make the provider the only way to sign in. Needs the `UPHEAP_OIDC_*` provider, or the server stops. See [Provider only](sign-in.md#provider-only) |
| `SESSION_TTL_DAYS` | `30` | How long a sign-in lasts |
| `LOG_FORMAT` | `json` | `json` or `pretty`; levels via `RUST_LOG` |
| `MAIL_URL` | unset | Where mail goes: `smtps://user:pass@host:465`, `smtp://user:pass@host:587` (STARTTLS), or for development `file:///dir` or `log:`. Unset means no mail |
| `MAIL_FROM` | unset | The sender, `Upheap <upheap@example.com>`. Required with `MAIL_URL` |
| `UPHEAP_OIDC_ISSUER` | none | The issuer URL of an OpenID Connect provider (Okta, Google Workspace, Keycloak…), exactly as the provider names itself, trailing `/` included. Set it with the next three, or none of the four |
| `UPHEAP_OIDC_CLIENT_ID` | none | The client ID the provider gave Upheap |
| `UPHEAP_OIDC_CLIENT_SECRET` | none | The client secret the provider gave Upheap |
| `UPHEAP_OIDC_LABEL` | none | What the sign-in button calls the provider: "Sign in with Acme SSO" |
| `UPHEAP_OIDC_SCOPES` | `openid email profile` | Space-separated scopes to ask for; `openid` is always asked |
| `UPHEAP_OIDC_GROUP_TEAMS` | none | A JSON object from team slug to the list of provider groups whose members the team has: `{"ops": ["engineering", "sre"]}`. Needs the provider. See [Teams from groups](sign-in.md#teams-from-groups) |
| `UPHEAP_OIDC_GROUPS_CLAIM` | `groups` | The ID token claim that lists a person's groups. Only read when `UPHEAP_OIDC_GROUP_TEAMS` is set |
| `UPHEAP_OIDC_SUBJECT_CLAIM` | `sub` | The ID token claim an account is matched on. Set `oid` for Microsoft Entra ID, so SCIM can link people. Existing accounts move over at their next sign-in. The claim must be stable and never given to another person: `sub` or `oid`, not an email or user name |
| `UPHEAP_OIDC_PROVIDER_LOGOUT` | `false` | `true` also signs a person out at the provider when they sign out of the web app, if the provider supports RP-initiated logout. Needs the provider |
| `UPHEAP_SCIM_TOKEN` | none | The bearer token the provider's SCIM client sends, at least 32 characters; make one with `openssl rand -hex 32`. Needs the provider. Unset, there are no SCIM endpoints. See [Provisioning over SCIM](sign-in.md#provisioning-over-scim) |
| `UPHEAP_OPERATOR_CONTACT` | none | One line of plain text, at most 200 characters, shown on the sign-in, reset and team pages wherever they say to ask whoever runs the server, and in the mail that tells the old address it was changed: who that is and how to reach them, such as `Ask Sam in #it`. Public: anyone who opens the sign-in page sees it |

