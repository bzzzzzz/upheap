# Operations

Running Upheap in production: Google Cloud Run in front of a Neon Postgres database. Both scale to zero, so a
deployment with little use costs close to nothing. Why this pair, rather than a machine with a disk, is in
[decisions.md](decisions.md#20-production-runs-on-cloud-run-with-neon-postgres). How the server itself is configured
and deployed in general is in [architecture.md](architecture.md#configuration-and-deployment). Last reviewed
2026-11-08.

## Production setup

Set up by hand on 2026-11-08:

| Piece | Value |
| --- | --- |
| Google Cloud project | `upheap`, region `us-west1` (Oregon) |
| Neon | Postgres 17 in AWS `us-west-2` (Oregon), so queries stay in one region |
| Image repository | Artifact Registry `us-west1-docker.pkg.dev/upheap/upheap`, image `upheap-server`, tagged with the commit |
| Cloud Run service | `upheap`, public, 1 CPU and 512 MiB, 80 requests per instance, at most 3 instances, startup probe on `/readyz`, startup CPU boost |
| Configuration | `DATABASE_URL` from the Secret Manager secret `upheap-database-url` (Neon's pooled URL, `sslmode=require`), `BASE_URL=https://upheap-251561194444.us-west1.run.app`, `DATABASE_MAX_CONNECTIONS=3`, `ALLOW_REGISTRATION=false` |
| Runtime identity | The default compute service account, with `secretmanager.secretAccessor` on the secret |

Cloud Run's URL terminates TLS, so there is no reverse proxy; a mapped domain would change `BASE_URL`. Each instance
keeps a small pool through Neon's pooled endpoint (PgBouncer), because several instances may run at once. The
startup probe uses `/readyz`, so no traffic reaches an instance before its migrations finish. Cold starts take about
a second, mostly Neon waking; `--min-instances=1` removes them for about $5–10 a month if they turn out to bother
people.

## Deploying

[`.github/workflows/deploy.yml`](../.github/workflows/deploy.yml) deploys `main` after CI passes on it. CI runs on
`main` after every merge as well as on pull requests, because a pull request may merge without being rebased onto
the latest `main`. Every check CI runs, Postgres among them, is required on `main`. The deploy can also be run by
hand from the Actions tab, without waiting for CI.

It builds the Docker image on the GitHub runner, pushes it to Artifact Registry, and runs `gcloud run deploy` with
every setting in the table above, so a manual change to one of them is undone on the next deploy: change the
workflow instead. Settings the workflow doesn't list keep their value on the service, and so does the public
(`allUsers`) binding, which isn't part of a revision.

The workflow signs in through Workload Identity Federation
([decisions.md](decisions.md#21-deploys-sign-in-through-workload-identity-federation)): GitHub's OIDC token is
exchanged for the `github-deploy` service account, so no key is stored anywhere. Only `deploy.yml`, run from `main`
of this repository, can sign in; the repository is matched by its numeric id, which a renamed or recreated
repository wouldn't share. The deploy job's actions are pinned to commits, since they run while it holds the
credential.

### One-time setup

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

## Rolling back

Send traffic to an earlier revision:

```sh
gcloud run services update-traffic upheap --region us-west1 --to-revisions=<revision>=100
```

## Releasing the CLI

[`.github/workflows/release.yml`](../.github/workflows/release.yml) builds the `upheap` CLI for Apple silicon macOS and
x86_64 and aarch64 Linux when a `vX.Y.Z` tag is pushed, publishes the archives and `SHA256SUMS` as a GitHub Release,
and updates the cask in `bzzzzzz/homebrew-tap`. Why it is built this way is in
[decisions.md](decisions.md#30-the-cli-ships-as-a-homebrew-cask-from-its-own-tap-built-by-a-hand-written-workflow).

To cut a release:

1. Bump `version` in `[workspace.package]` of `Cargo.toml` in a pull request (`cargo update -w` refreshes
   `Cargo.lock`), and merge it.
2. Tag the commit on main: `git fetch origin && git tag vX.Y.Z origin/main && git push origin vX.Y.Z`.

The workflow refuses a tag that doesn't match that version or isn't on main, and never replaces a published release;
its steps' error messages say what else it checks and what to do. Pull requests that touch the packaging or what the CLI is built from run
it as a dry run that publishes nothing.

Recovering from a failed run:

- **The tap job** (a bad key, the network): re-run the failed jobs. It leaves the tap alone if the cask is unchanged.
- **A build or release failure before the release exists:** delete the tag (`git push origin :refs/tags/vX.Y.Z`), fix,
  and tag again.
- **A bad published release:** publish a new patch version. Never replace its assets.

### One-time setup

The tap repository, a deploy key that can write to it, and the secret that holds the key:

```sh
TMP=$(mktemp -d)
gh repo create bzzzzzz/homebrew-tap --public --add-readme --description "Homebrew tap for Upheap"
ssh-keygen -t ed25519 -N '' -C upheap-release -f $TMP/key
gh repo deploy-key add $TMP/key.pub --repo bzzzzzz/homebrew-tap --allow-write --title upheap-release
gh secret set HOMEBREW_TAP_DEPLOY_KEY --repo bzzzzzz/upheap < $TMP/key
rm -rf $TMP
```

## Cautions

- **The macOS binary isn't notarized**
  ([decisions.md](decisions.md#30-the-cli-ships-as-a-homebrew-cask-from-its-own-tap-built-by-a-hand-written-workflow)):
  after an upgrade, a user may be asked once to let `upheap` use its keychain entry; that's expected. A cask's `zap`
  doesn't remove keychain entries; `upheap auth logout` does.
- **sqlx through the pooler.** sqlx prepares statements and sets `extra_float_digits` when it connects, either of
  which PgBouncer can refuse. Neon's pooler accepted both on the first deploy. If a later Neon change breaks it, put
  the direct endpoint in the secret, or have `store::connect` stop sending the parameter
  (`PgConnectOptions::extra_float_digits(None)`; no column is a float).
- **Several instances start at once.** Migrations run under a transaction-scoped advisory lock, so each is applied
  once ([architecture.md](architecture.md#configuration-and-deployment)). Nothing else needs coordinating between
  instances today.
- **No work between requests.** Cloud Run pauses the CPU between requests. The server has no background timers;
  any added later must be triggered by Cloud Scheduler calling an endpoint instead.
- **Neon plan limits.** The free tier has about 0.5 GB of storage and a short restore window. Decide whether a paid
  plan is needed, and check the restore window, before real data goes in.
