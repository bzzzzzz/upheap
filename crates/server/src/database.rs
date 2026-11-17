//! Connecting to the database at startup, retried with backoff: a database that scales to zero (Neon) can take
//! longer to wake than one try's timeout, and an instance that exits on the first timeout fails its deploy.

use std::future::Future;
use std::time::Duration;

use tokio::time::Instant;

/// How long to wait between tries: `first`, doubling after each failure, never more than `max`.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    pub first: Duration,
    pub max: Duration,
}

impl Backoff {
    /// 1 s, 2 s, 4 s, then 8 s between tries.
    pub const STARTUP: Self = Self {
        first: Duration::from_secs(1),
        max: Duration::from_secs(8),
    };
}

/// Opens the pool for `url`, trying again after each failure until `deadline` has passed since the first try. No new
/// try starts after the deadline; the last error is returned then. Every error is retried: a wrong URL or password
/// fails after the deadline rather than at once.
pub async fn connect(
    url: &str,
    max_connections: u32,
    deadline: Duration,
    backoff: Backoff,
) -> Result<store::DatabaseConnection, store::StoreError> {
    retry(|| store::connect(url, max_connections), deadline, backoff).await
}

async fn retry<T, E, F, Fut>(mut attempt: F, deadline: Duration, backoff: Backoff) -> Result<T, E>
where
    E: std::fmt::Display,
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    let started = Instant::now();
    let mut delay = backoff.first;
    let mut tries: u32 = 1;
    loop {
        match attempt().await {
            Ok(value) => {
                if tries > 1 {
                    tracing::info!(tries, "connected to the database");
                }
                return Ok(value);
            }
            Err(error) if started.elapsed() + delay < deadline => {
                tracing::warn!(
                    try_number = tries,
                    error = %error,
                    retry_in_ms = u64::try_from(delay.as_millis()).unwrap_or(u64::MAX),
                    "can't connect to the database yet",
                );
                tokio::time::sleep(delay).await;
                delay = (delay * 2).min(backoff.max);
                tries += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    const FAST: Backoff = Backoff {
        first: Duration::from_millis(20),
        max: Duration::from_millis(80),
    };

    /// A SQLite file in a directory that doesn't exist yet can't be opened; once the directory appears it can,
    /// as a database that accepts connections only after a while does.
    #[tokio::test]
    async fn connects_once_the_database_accepts() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("late");
        let url = format!("sqlite://{}?mode=rwc", dir.join("heap.db").display());
        let appears = tokio::spawn({
            let dir = dir.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(150)).await;
                std::fs::create_dir(dir)
            }
        });

        let db = connect(&url, 1, Duration::from_secs(10), FAST).await?;
        appears.await??;
        db.ping().await?;
        Ok(())
    }

    #[tokio::test]
    async fn gives_up_with_the_last_error_after_the_deadline() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let url = format!("sqlite://{}?mode=rwc", root.path().join("never/heap.db").display());
        let started = std::time::Instant::now();

        let result = connect(&url, 1, Duration::from_millis(200), FAST).await;

        assert_that!(result.map(drop), err(anything()));
        assert_that!(started.elapsed(), lt(Duration::from_secs(2)));
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn backs_off_doubling_up_to_the_cap_and_stops_at_the_deadline() -> anyhow::Result<()> {
        let started = Instant::now();
        let mut tried_at = Vec::new();

        let result: std::result::Result<(), String> = retry(
            || {
                tried_at.push(started.elapsed());
                std::future::ready(Err("refused".to_owned()))
            },
            Duration::from_secs(30),
            Backoff::STARTUP,
        )
        .await;

        assert_that!(result, err(eq("refused")));
        let secs: Vec<u64> = tried_at.iter().map(Duration::as_secs).collect();
        assert_that!(secs, elements_are![eq(&0), eq(&1), eq(&3), eq(&7), eq(&15), eq(&23)]);
        Ok(())
    }
}
