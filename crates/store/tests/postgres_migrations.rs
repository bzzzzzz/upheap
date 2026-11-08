//! Migration runs on Postgres, which runs them in one transaction holding a lock (`store::run_migrations`): servers
//! starting together, as Cloud Run instances do, apply each migration once, and a run that fails changes nothing.
//!
//! MySQL and SQLite do neither, so the tests run only with `TEST_DATABASE_URL` pointing at a Postgres server and
//! pass trivially elsewhere.

use anyhow::Result;
use googletest::prelude::*;
use store::{ConnectionTrait, DbBackend, Statement};

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
    expect_that!(
        applied,
        elements_are![eq(&0), eq(&0), eq(&0), eq(&store::testing::migration_count())]
    );
    expect_that!(again, eq(0));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_failed_run_leaves_the_database_as_it_was() -> Result<()> {
    // Arrange: after the first migration, add the column the third one adds, so the third fails after the second
    // has run.
    let db = store::testing::database_before_migration(1).await?;
    if db.get_database_backend() != DbBackend::Postgres {
        return Ok(());
    }
    db.execute_unprepared("ALTER TABLE items ADD COLUMN blocked_by_item_id char(36)")
        .await?;

    // Act
    let run = store::run_migrations(&db).await;

    // Assert: the second migration's tables and its record are gone with the rest of the run.
    let count = |sql: &str| {
        let db = &db;
        let sql = sql.to_owned();
        async move {
            let row = db
                .query_one_raw(Statement::from_string(DbBackend::Postgres, sql))
                .await?
                .ok_or_else(|| anyhow::anyhow!("no row"))?;
            anyhow::Ok(row.try_get_by_index::<i64>(0)?)
        }
    };
    let recorded = count("SELECT count(*) FROM seaql_migrations").await?;
    let tags_tables = count("SELECT count(*) FROM information_schema.tables WHERE table_name = 'tags'").await?;
    expect_that!(run, err(anything()));
    expect_that!(recorded, eq(1));
    expect_that!(tags_tables, eq(0));
    Ok(())
}
