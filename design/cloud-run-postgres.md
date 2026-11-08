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
| `postgres` feature in `crates/store` (`sqlx-postgres` for SeaORM and migrations) | ⬜ | Alongside `sqlite` and `mysql` |
| Server accepts `postgres://` in `DATABASE_URL` | ⬜ | |
| Migrations run on Postgres | ⬜ | Look for column types only MySQL has |
| Store and API suites pass on Postgres (`TEST_DATABASE_URL`) | ⬜ | Against a local `postgres:17` container first |
| Postgres job in the CI matrix | ⬜ | Service container, as for MySQL |
| Playwright golden paths against Postgres | ⬜ | Optional; once the suites pass |
| Connection pool size set from config | ⬜ | Small default for Cloud Run |
| Migrations safe when several instances start at once | ⬜ | See risks |
| Docs updated: Postgres supported for production | ⬜ | [offering.md](offering.md) §4, [technical-design.md](technical-design.md), README |
| Neon project and Cloud Run service set up | ⬜ | |
| Deploy from CI on merge to `main` | ⬜ | |
| First production deploy, backups checked | ⬜ | Neon's point-in-time restore window on the chosen plan |

## Risks and open questions

- **Errors end the transaction.** After any failed statement, such as a unique violation, Postgres rejects every
  later statement in that transaction. A use case that catches a conflict and carries on in the same transaction
  works on MySQL and SQLite but fails on Postgres. The test suites will show whether any do.
- **Stricter types.** No implicit casts between integers and strings; booleans are a real type rather than
  `TINYINT`.
- **Row order and text comparison.** Queries without `ORDER BY` can return rows in a different order. Text
  comparison is case-sensitive, which `name_key` already handles.
- **Concurrent migrations.** Every instance runs pending migrations on startup. Two instances starting together
  could race. Check whether `sea-orm-migration` takes a lock on Postgres; if not, take an advisory lock around
  the run.
- **No work between requests.** Cloud Run pauses the CPU between requests. The server has no background timers
  today; any added later must be triggered by Cloud Scheduler calling an endpoint instead.
- **Neon free tier limits.** About 0.5 GB of storage and a short restore window. Decide whether a paid plan is
  needed before real data goes in.
