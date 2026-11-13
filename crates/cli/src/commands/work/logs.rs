//! The run logs: where each run's streamed agent output is saved, and removing the old ones. The log of a run is
//! `<config dir>/runs/<run id>.jsonl`, next to the loop's own log and outside every run's directory, so the agent
//! can't read it. They are kept as long as a kept directory is.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::kept::UNUSED_FOR;

/// The directory in the config directory that holds the logs.
const RUNS_DIR: &str = "runs";

/// The extension of a log: one JSON event a line.
const EXTENSION: &str = "jsonl";

/// The directory the logs are in.
pub(super) fn runs_dir(config_dir: &Path) -> PathBuf {
    config_dir.join(RUNS_DIR)
}

/// The log of the run `run_id`.
pub(super) fn log_path(config_dir: &Path, run_id: &str) -> PathBuf {
    runs_dir(config_dir).join(format!("{run_id}.{EXTENSION}"))
}

/// Removes the logs in `runs_dir` last written more than 7 days before `now`: files ending in `.jsonl`, nothing else.
/// Never fails: what can't be removed is a warning.
pub(super) fn prune(runs_dir: &Path, now: SystemTime) {
    let entries = match std::fs::read_dir(runs_dir) {
        Ok(entries) => entries,
        // No log was ever made.
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return,
        Err(err) => {
            tracing::warn!(dir = %runs_dir.display(), error = %err, "couldn't list the run logs");
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let old = entry.file_type().is_ok_and(|kind| kind.is_file())
            && path.extension().is_some_and(|extension| extension == EXTENSION)
            && entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| now.duration_since(modified).is_ok_and(|age| age > UNUSED_FOR));
        if !old {
            continue;
        }
        if let Err(err) = std::fs::remove_file(&path) {
            tracing::warn!(log = %path.display(), error = %err, "couldn't remove an old run log");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn a_log_is_named_by_its_run_in_the_runs_directory() {
        expect_that!(
            log_path(Path::new("/config/upheap"), "abc-123"),
            eq(&PathBuf::from("/config/upheap/runs/abc-123.jsonl"))
        );
    }

    #[gtest]
    fn only_jsonl_files_older_than_the_kept_period_are_pruned() -> googletest::Result<()> {
        // Arrange
        let dir = tempfile::tempdir()?;
        let now = SystemTime::now();
        let write = |name: &str, age: Duration| -> std::io::Result<PathBuf> {
            let path = dir.path().join(name);
            std::fs::write(&path, "{}\n")?;
            std::fs::File::options()
                .write(true)
                .open(&path)?
                .set_modified(now - age)?;
            Ok(path)
        };
        let old = write("old.jsonl", UNUSED_FOR + Duration::from_secs(60))?;
        let fresh = write("fresh.jsonl", UNUSED_FOR - Duration::from_secs(60))?;
        let other = write("old.txt", UNUSED_FOR * 2)?;

        // Act
        prune(dir.path(), now);

        // Assert
        verify_that!(old.exists(), eq(false))?;
        verify_that!(fresh.exists(), eq(true))?;
        verify_that!(other.exists(), eq(true))
    }

    #[gtest]
    fn pruning_a_directory_that_is_not_there_does_nothing() {
        prune(Path::new("/definitely/not/here/runs"), SystemTime::now());
    }
}
