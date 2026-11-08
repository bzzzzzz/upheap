# Cloud Run and Neon on Postgres

Plan for running Upheap in production on Google Cloud Run with a Neon Postgres database. Both scale to zero, so a
deployment with little use costs close to nothing. Last reviewed 2026-11-07.

## Why

- Cloud Run has no persistent disk, so SQLite can't hold production data there and the database has to be
  external.
- Most serverless databases that scale to zero speak Postgres (Neon, Supabase, Aurora Serverless v2). On the
  MySQL side only TiDB Serverless and Aiven are realistic.
- [technical-design.md](technical-design.md) already plans Postgres ("Postgres later") and keeps to portable SQL,
  and the store goes through SeaORM (`lock_exclusive()`, `SqlErr::UniqueConstraintViolation`), so adding
  Postgres should be mostly configuration and tests.

## Target setup

1. **App:** the existing Docker image on Cloud Run, one service in one region. Cloud Run's URL or a mapped
   domain terminates TLS, so there is no reverse proxy.
2. **Database:** Neon Postgres, reached through its pooled endpoint (PgBouncer). Each instance keeps a small
   pool, at most 2–5 connections, because Cloud Run may run several instances at once.
3. **Configuration:** `DATABASE_URL=postgres://…` and the other variables from
   [technical-design.md](technical-design.md#configuration-and-deployment), with the database URL kept in Secret
   Manager.
4. **Readiness:** Cloud Run's startup probe uses `/readyz`, so no traffic reaches an instance before migrations
   finish.
5. **Cold starts:** expected to be about a second, mostly from waking Neon. `--min-instances=1` removes them for
   about $5–10 a month if they turn out to bother people.

## Progress

Status: ✅ done · 🟡 partial · ⬜ not started.

| Step | Status | Notes |
| --- | :---: | --- |
| `postgres` feature in `crates/store` (`sqlx-postgres` for SeaORM and migrations) | ✅ | On by default, alongside `sqlite` and `mysql` |
| Server accepts `postgres://` in `DATABASE_URL` | ✅ | `postgresql://` too |
| Migrations run on Postgres | ✅ | The handles migration makes its columns `NOT NULL` there, as on MySQL |
| Store and API suites pass on Postgres (`TEST_DATABASE_URL`) | ✅ | On `postgres:17`; see the findings below |
| Postgres job in the CI matrix | ✅ | Service container, as for MySQL |
| Playwright golden paths against Postgres | ⬜ | Optional; the config starts the server on in-memory SQLite |
| Connection pool size set from config | ✅ | `DATABASE_MAX_CONNECTIONS`, default 5 |
| Migrations safe when several instances start at once | ✅ | Advisory lock around the run; `crates/store/tests/concurrent_migrations.rs` |
| Docs updated: Postgres supported for production | ✅ | [offering.md](offering.md) §4, [technical-design.md](technical-design.md), README |
| Neon project and Cloud Run service set up | ⬜ | |
| Deploy from CI on merge to `main` | ⬜ | |
| First production deploy, backups checked | ⬜ | Neon's point-in-time restore window on the chosen plan |

## What running the suites on Postgres found

- **Ambiguous names in upserts.** In `ON CONFLICT DO UPDATE`, a bare column name could mean the existing row or
  `excluded`, and Postgres refuses it. The sign-in attempt upsert now names its columns with their table, which
  SQLite and MySQL accept too.
- **`char(n)` pads.** Postgres pads shorter values with spaces and reads them back padded, so they no longer match
  the value written. Every `char` column holds values of exactly its length (UUIDs, SHA-256 hex, item keys), so
  only a test with short stand-in hashes was affected.
- **Identifier quoting.** One migration test quoted `key` with MySQL's backticks; it now quotes per backend.
- Nothing else: no use case carries on after a failed statement, and no query relied on row order or implicit
  casts.

## Risks and open questions

- **Errors end the transaction.** After any failed statement, such as a unique violation, Postgres rejects every
  later statement in that transaction. No use case does this today (the suites pass); any new one that catches a
  conflict and carries on in the same transaction must use a savepoint (a nested `begin`).
- **Concurrent migrations.** `sea-orm-migration` takes no lock. On Postgres, `store::run_migrations` runs every
  pending migration in one transaction holding `pg_advisory_xact_lock`, so other instances wait and then find
  nothing pending. The lock is transaction-scoped because Neon's pooler runs in transaction mode, where a
  session-scoped lock could be taken and released on different server connections. MySQL and SQLite have no such
  lock; they aren't run with several instances.
- **Prepared statements through the pooler.** sqlx prepares statements. Neon's PgBouncer supports protocol-level
  prepared statements in transaction mode, but this hasn't been tried against Neon yet; check it on the first
  deploy, and fall back to the direct endpoint if it fails.
- **No work between requests.** Cloud Run pauses the CPU between requests. The server has no background timers
  today; any added later must be triggered by Cloud Scheduler calling an endpoint instead.
- **Neon free tier limits.** About 0.5 GB of storage and a short restore window. Decide whether a paid plan is
  needed before real data goes in.
