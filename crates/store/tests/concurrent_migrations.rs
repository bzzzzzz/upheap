//! Servers starting together on one Postgres database, as Cloud Run instances do, apply each migration once.
//!
//! Only Postgres takes a lock around the run (`store::run_migrations`), so the test runs only with
//! `TEST_DATABASE_URL` pointing at a Postgres server and passes trivially elsewhere.

use anyhow::Result;
use googletest::prelude::*;
use store::DbBackend;

#[gtest]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_runs_apply_each_migration_once() -> Result<()> {
    // Arrange
    let db = store::testing::database_before_migration(0).await?;
    if db.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }

    // Act
    let runs: Vec<_> = (0..4)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move { store::run_migrations(&db).await })
        })
        .collect();
    let mut applied = Vec::new();
    for run in runs {
        applied.push(run.await??);
    }
    let again = store::run_migrations(&db).await?;

    // Assert: one run applied them all, and the others found nothing pending.
    applied.sort_unstable();
    expect_that!(applied[..3], eq(&[0, 0, 0]));
    expect_that!(applied[3], gt(0));
    expect_that!(again, eq(0));
    Ok(())
}
