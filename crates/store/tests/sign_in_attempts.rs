//! Counting sign-in attempts. Runs on in-memory SQLite, or on MySQL with `TEST_DATABASE_URL` set; the MySQL run
//! is what proves that concurrent attempts are each counted.

use anyhow::{Context, Result};
use domain::sign_in::SignInAttempts;
use domain::{Duration, Timestamp};
use googletest::prelude::*;
use store::{ConnectionTrait, TransactionTrait};

fn at(minutes: i64) -> Result<Timestamp> {
    Timestamp::from_millis(Duration::from_minutes(minutes).as_millis()).context("in range")
}

/// Counts an attempt at `now`, opening 15-minute windows.
async fn count(db: &impl ConnectionTrait, key: &str, now: Timestamp) -> Result<SignInAttempts> {
    let window_end = now.checked_add(Duration::from_minutes(15)).context("in range")?;
    Ok(store::sign_in_attempts::count_sign_in_attempt(db, key, now, window_end).await?)
}

#[gtest]
#[tokio::test]
async fn attempts_count_up_within_a_window_and_start_over_after_it() -> Result<()> {
    // Arrange
    let db = store::testing::fresh_database().await?;

    // Act
    let first = count(&db, "a", at(0)?).await?;
    let second = count(&db, "a", at(14)?).await?;
    let after_window = count(&db, "a", at(15)?).await?;

    // Assert
    expect_that!((first.count, first.window_ends_at), eq((1, at(15)?)));
    expect_that!((second.count, second.window_ends_at), eq((2, at(15)?)));
    expect_that!((after_window.count, after_window.window_ends_at), eq((1, at(30)?)));
    Ok(())
}

#[gtest]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_attempts_are_each_counted() -> Result<()> {
    // Arrange
    let db = store::testing::fresh_database().await?;
    let now = at(0)?;

    // Act
    let attempts: Vec<_> = (0..10)
        .map(|_| {
            let db = db.clone();
            tokio::spawn(async move {
                let txn = db.begin().await?;
                let attempts = count(&txn, "a", now).await?;
                txn.commit().await?;
                anyhow::Ok(attempts)
            })
        })
        .collect();
    let mut counts = Vec::new();
    for attempt in attempts {
        counts.push(attempt.await??.count);
    }
    counts.sort_unstable();

    // Assert
    expect_that!(counts, eq(&(1..=10).collect::<Vec<u32>>()));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn clearing_and_pruning_forget_attempts() -> Result<()> {
    // Arrange
    let db = store::testing::fresh_database().await?;
    count(&db, "signed-in", at(0)?).await?;
    count(&db, "old", at(0)?).await?;
    count(&db, "ends-now", at(5)?).await?;
    count(&db, "recent", at(10)?).await?;

    // Act
    store::sign_in_attempts::clear_sign_in_attempts(&db, "signed-in").await?;
    let first_batch = store::sign_in_attempts::delete_ended_sign_in_windows(&db, at(20)?, 1).await?;
    let second_batch = store::sign_in_attempts::delete_ended_sign_in_windows(&db, at(20)?, 10).await?;
    let nothing_left = store::sign_in_attempts::delete_ended_sign_in_windows(&db, at(20)?, 10).await?;
    let signed_in = count(&db, "signed-in", at(20)?).await?;
    let recent = count(&db, "recent", at(20)?).await?;

    // Assert
    expect_that!((first_batch, second_batch, nothing_left), eq((1, 1, 0)));
    expect_that!(signed_in.count, eq(1));
    expect_that!(recent.count, eq(2));
    Ok(())
}
