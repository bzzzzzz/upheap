# Cloud Run and Neon on Postgres

Plan for running Upheap in production on Google Cloud Run with a Neon Postgres database. Both scale to zero, so a
deployment with little use costs close to nothing. Last reviewed 2026-11-08.

## Why

- Cloud Run has no persistent disk, so SQLite can't hold production data there and the database has to be
  external.
- Most serverless databases that scale to zero speak Postgres (Neon, Supabase, Aurora Serverless v2). On the
  MySQL side only TiDB Serverless and Aiven are realistic.
- [architecture.md](architecture.md) keeps to portable SQL, and the store goes through SeaORM
  (`lock_exclusive()`, `SqlErr::UniqueConstraintViolation`), so adding Postgres was mostly configuration and tests.

## Target setup

1. **App:** the existing Docker image on Cloud Run, one service in one region. Cloud Run's URL or a mapped
   domain terminates TLS, so there is no reverse proxy.
2. **Database:** Neon Postgres, reached through its pooled endpoint (PgBouncer). Each instance keeps a small
   pool, at most 2–5 connections, because Cloud Run may run several instances at once.
3. **Configuration:** `DATABASE_URL=postgres://…` and the other variables from
   [architecture.md](architecture.md#configuration-and-deployment), with the database URL kept in Secret
   Manager.
4. **Readiness:** Cloud Run's startup probe uses `/readyz`, so no traffic reaches an instance before migrations
   finish.
5. **Cold starts:** expected to be about a second, mostly from waking Neon. `--min-instances=1` removes them for
   about $5–10 a month if they turn out to bother people.

## Production setup

What runs today, set up by hand on 2026-11-08:

| Piece | Value |
| --- | --- |
| Google Cloud project | `upheap`, region `us-west1` (Oregon) |
| Neon | Postgres 17 in AWS `us-west-2` (Oregon), so queries stay in one region |
| Image repository | Artifact Registry `us-west1-docker.pkg.dev/upheap/upheap`, image `upheap-server`, tagged with the commit |
| Cloud Run service | `upheap`, public, 1 CPU and 512 MiB, 80 requests per instance, at most 3 instances, startup probe on `/readyz`, startup CPU boost |
| Configuration | `DATABASE_URL` from the Secret Manager secret `upheap-database-url` (Neon's pooled URL, `sslmode=require`), `BASE_URL=https://upheap-251561194444.us-west1.run.app`, `DATABASE_MAX_CONNECTIONS=3`, `ALLOW_REGISTRATION=false` |
| Runtime identity | The default compute service account, with `secretmanager.secretAccessor` on the secret |

The service uses Google's `run.app` URL for now; a mapped domain would change `BASE_URL`.

### Deploying

[`.github/workflows/deploy.yml`](../.github/workflows/deploy.yml) deploys `main` after CI passes on it. CI runs on
`main` after every merge as well as on pull requests, because a pull request may merge without being rebased onto
the latest `main`. Every check CI runs, Postgres among them, is required on `main`. The deploy can also be run by
hand from the Actions tab, without waiting for CI.

It builds the Docker image on the GitHub runner, pushes it to Artifact Registry, and runs `gcloud run deploy` with
every setting in the table above, so a manual change to one of them is undone on the next deploy: change the
workflow instead. Settings the workflow doesn't list keep their value on the service, and so does the public
(`allUsers`) binding, which isn't part of a revision.

The workflow signs in through Workload Identity Federation: GitHub's OIDC token is exchanged for the
`github-deploy` service account, so no key is stored anywhere. Only `deploy.yml`, run from `main` of this
repository, can sign in; the repository is matched by its numeric id, which a renamed or recreated repository
wouldn't share. The deploy job's actions are pinned to commits, since they run while it holds the credential.
The one-time setup:

```sh
PROJECT_ID=upheap
PROJECT_NUMBER=$(gcloud projects describe $PROJECT_ID --format='value(projectNumber)')
DEPLOY_SA=github-deploy@$PROJECT_ID.iam.gserviceaccount.com
REPO=bzzzzzz/upheap
REPO_ID=$(gh api repos/$REPO --jq .id)

gcloud services enable iamcredentials.googleapis.com sts.googleapis.com
gcloud iam service-accounts create github-deploy --display-name="GitHub deploy"
# Deploys this one service only.
gcloud run services add-iam-policy-binding upheap --region=us-west1 \
  --member=serviceAccount:$DEPLOY_SA --role=roles/run.developer
gcloud artifacts repositories add-iam-policy-binding upheap --location=us-west1 \
  --member=serviceAccount:$DEPLOY_SA --role=roles/artifactregistry.writer
# Lets the deploy start revisions that run as the runtime service account.
gcloud iam service-accounts add-iam-policy-binding $PROJECT_NUMBER-compute@developer.gserviceaccount.com \
  --member=serviceAccount:$DEPLOY_SA --role=roles/iam.serviceAccountUser

gcloud iam workload-identity-pools create github --location=global --display-name=GitHub
gcloud iam workload-identity-pools providers create-oidc upheap --location=global \
  --workload-identity-pool=github --issuer-uri=https://token.actions.githubusercontent.com \
  --attribute-mapping=google.subject=assertion.sub,attribute.repository_id=assertion.repository_id \
  --attribute-condition="assertion.repository_id=='$REPO_ID' && assertion.ref=='refs/heads/main' && assertion.workflow_ref=='$REPO/.github/workflows/deploy.yml@refs/heads/main'"
gcloud iam service-accounts add-iam-policy-binding $DEPLOY_SA --role=roles/iam.workloadIdentityUser \
  --member=principalSet://iam.googleapis.com/projects/$PROJECT_NUMBER/locations/global/workloadIdentityPools/github/attribute.repository_id/$REPO_ID

gh variable set GCP_PROJECT_ID --body $PROJECT_ID
gh variable set GCP_PROJECT_NUMBER --body $PROJECT_NUMBER
gh variable set GCP_REGION --body us-west1
gh variable set GCP_DEPLOY_SERVICE_ACCOUNT --body $DEPLOY_SA
gh variable set GCP_WORKLOAD_IDENTITY_PROVIDER \
  --body projects/$PROJECT_NUMBER/locations/global/workloadIdentityPools/github/providers/upheap
gh variable set BASE_URL --body https://upheap-$PROJECT_NUMBER.us-west1.run.app
```

To roll back, send traffic to an earlier revision:
`gcloud run services update-traffic upheap --region us-west1 --to-revisions=<revision>=100`.

## What running the suites on Postgres found

- **Ambiguous names in upserts.** In `ON CONFLICT DO UPDATE`, a bare column name could mean the existing row or
  `excluded`, and Postgres refuses it. The sign-in attempt upsert now names its columns with their table, which
  SQLite and MySQL accept too.
- **`char(n)` pads.** Postgres pads shorter values with spaces and reads them back padded, so they no longer match
  the value written. Every `char` column holds values of exactly its length (UUIDs, SHA-256 hex, item keys), so
  only a test with short stand-in hashes was affected.
- **Identifier quoting.** One migration test quoted `key` with MySQL's backticks; it now quotes per backend.
- **`LOWER` beyond ASCII.** SQLite's `LOWER` lowercases ASCII only and Postgres's all of Unicode, so a user lookup
  for "Öla" missed "Ölaf" on Postgres: the name was lowercased in SQL and the query only in Rust, for ASCII. The
  query now goes through the same `LOWER` as the name. A review found this; the suites had no such name.
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
- **sqlx through the pooler: works.** sqlx prepares statements and sets `extra_float_digits` when it connects,
  either of which PgBouncer can refuse. Neon's pooler accepted both on the first deploy (2026-11-08). If a later
  Neon change breaks it, put the direct endpoint in the secret, or have `store::connect` stop sending the parameter
  (`PgConnectOptions::extra_float_digits(None)`; no column is a float).
- **No work between requests.** Cloud Run pauses the CPU between requests. The server has no background timers
  today; any added later must be triggered by Cloud Scheduler calling an endpoint instead.
- **Neon free tier limits.** About 0.5 GB of storage and a short restore window. Decide whether a paid plan is
  needed before real data goes in.
