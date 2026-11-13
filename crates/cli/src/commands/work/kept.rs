//! The directories `work run --root` keeps, one for each item: where one is, the marker that says when a run last
//! started or ended in it, which ones the sweep looks at, and whether it keeps or removes each. No server and no
//! clock here: the loop reads the item and passes in the time, so every rule below is tested with plain values.
//!
//! The path is the whole record. There is no index file to fall out of step with the directories.

use std::collections::HashSet;
use std::fs::{File, TryLockError};
use std::io::{Error, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use domain::handle::Handle;
use domain::validation::Field;

use super::run::is_unreachable;
use crate::CliError;

/// The file a run writes into its directory as it starts, and touches as it ends. Its modification time is when a run
/// last started or ended there.
///
/// An agent that deletes it makes the sweep ignore the directory until the next run on the item writes it again:
/// without it, nothing says the directory is ours to remove.
pub const MARKER: &str = ".upheap-run";

/// How long a directory can go without a run before the sweep removes it.
pub const UNUSED_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The start of the name a directory is renamed to before it is deleted.
pub const REMOVING: &str = ".upheap-removing-";

/// Whether `name` can be one part of a path made from a handle: a team or heap slug.
fn is_slug(name: &str) -> bool {
    Handle::parse(name, Field::Slug).is_ok()
}

/// Whether `name` is an item's key: letters and digits only, so it can't name another path.
fn is_key(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric())
}

/// `root/<team>/<heap>/<key>`. The names come from the server, so each must be a single safe path part; if one
/// isn't, no path is made.
pub fn item_directory(root: &Path, team: &str, heap: &str, key: &str) -> std::io::Result<PathBuf> {
    if is_slug(team) && is_slug(heap) && is_key(key) {
        Ok(root.join(team).join(heap).join(key))
    } else {
        Err(Error::new(
            ErrorKind::InvalidInput,
            "the server sent a name that can't name a directory",
        ))
    }
}

/// How many times [`prepare`] tries again when a sweep took the directory away while it was making it.
const PREPARE_TRIES: usize = 5;

/// Makes `dir` and its parents if they aren't there, and writes the marker, naming `server`. A directory that is there
/// is used as it is. `started` is an RFC 3339 UTC time, for a person reading the file.
///
/// Returns the marker, open and holding a shared lock: keep it for as long as the run goes on. A sweep, in this loop
/// or another one on the same root, takes the lock exclusively before it removes a directory, so it skips this one.
pub fn prepare(dir: &Path, run_id: &str, started: &str, server: &str) -> std::io::Result<File> {
    let mut tries = 0;
    loop {
        tries += 1;
        match lock_marker(dir) {
            Ok(Some(mut marker)) => {
                // Written whole every time, so its modification time moves even though the text may not.
                marker.set_len(0)?;
                marker.write_all(format!("run {run_id}\nstarted {started}\nserver {server}\n").as_bytes())?;
                return Ok(marker);
            }
            // A sweep renamed the directory, or removed an emptied parent, between the steps: make it again.
            Ok(None) if tries < PREPARE_TRIES => {}
            Err(err) if err.kind() == ErrorKind::NotFound && tries < PREPARE_TRIES => {}
            Ok(None) => return Err(Error::other("the directory kept being removed while it was made")),
            Err(err) => return Err(err),
        }
    }
}

/// Makes `dir`, opens its marker and takes a shared lock on it. `None` when, once the lock is held, the marker at
/// `dir` is no longer the file that was locked: a sweep renamed the directory away meanwhile.
fn lock_marker(dir: &Path) -> std::io::Result<Option<File>> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(MARKER);
    let marker = File::options().create(true).truncate(false).write(true).open(&path)?;
    marker.lock_shared()?;
    Ok(is_same_file(&marker, &path)?.then_some(marker))
}

/// Whether `path` still names the file `file` has open.
#[cfg(unix)]
fn is_same_file(file: &File, path: &Path) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let (open, named) = (file.metadata()?, std::fs::symlink_metadata(path)?);
    Ok(open.dev() == named.dev() && open.ino() == named.ino())
}

/// Whether `path` still names the file `file` has open. Without inode numbers, whether a file is there at all.
#[cfg(not(unix))]
fn is_same_file(_file: &File, path: &Path) -> std::io::Result<bool> {
    std::fs::symlink_metadata(path).map(|_| true)
}

/// The server a marker's text names, from its `server` line.
fn marker_server(text: &str) -> Option<String> {
    text.lines()
        .find_map(|line| line.strip_prefix("server "))
        .map(str::to_owned)
}

/// A directory the sweep looks at.
#[derive(Debug, PartialEq, Eq)]
pub struct Kept {
    pub dir: PathBuf,
    pub team: String,
    pub heap: String,
    pub key: String,
    /// How long ago a run last started or ended here. Zero for a marker dated in the future, as after a clock change.
    pub age: Duration,
    /// When the marker was last written, as listed. A sweep removes the directory only if it still is.
    pub modified: Option<SystemTime>,
    /// The server the last run here worked with, from the marker. Only that server is asked about the item: another
    /// server, or none, can't say whether it is closed, so only age removes the directory.
    pub server: Option<String>,
}

impl Kept {
    /// `ada/work/k3x9a`, as the sweep says it.
    pub fn name(&self) -> String {
        format!("{}/{}/{}", self.team, self.heap, self.key)
    }
}

/// Whether `path` is a real directory: not a symlink to one.
fn is_real_directory(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// The names of the real directories in `dir` that pass `fits`. A directory that can't be read has none.
fn child_directories(dir: &Path, fits: fn(&str) -> bool) -> Vec<(String, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| Some((entry.file_name().into_string().ok()?, entry.path())))
        .filter(|(name, path)| fits(name) && is_real_directory(path))
        .collect()
}

/// The directories under `root` that are kept: `<team>/<heap>/<key>` with a marker file in it, every part a real
/// directory. Anything else is not ours and is left out, such as the old `<key>-<run id>` directories, stray files,
/// symlinks, and directories without a marker.
pub fn list(root: &Path, now: SystemTime) -> Vec<Kept> {
    let mut kept = Vec::new();
    for (team, team_dir) in child_directories(root, is_slug) {
        for (heap, heap_dir) in child_directories(&team_dir, is_slug) {
            for (key, dir) in child_directories(&heap_dir, is_key) {
                let marker = dir.join(MARKER);
                let Ok(metadata) = std::fs::symlink_metadata(&marker) else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let modified = metadata.modified().ok();
                let age = modified
                    .and_then(|modified| now.duration_since(modified).ok())
                    .unwrap_or_default();
                let server = std::fs::read_to_string(&marker)
                    .ok()
                    .and_then(|text| marker_server(&text));
                kept.push(Kept {
                    dir,
                    team: team.clone(),
                    heap: heap.clone(),
                    key,
                    age,
                    modified,
                    server,
                });
            }
        }
    }
    kept
}

/// The directories in `root` that an interrupted sweep renamed and did not finish deleting.
pub fn leftovers(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(REMOVING))
        .map(|entry| entry.path())
        .collect()
}

/// How many times [`delete`] tries before it gives up, and how long it waits between tries.
const DELETE_TRIES: u32 = 3;
const DELETE_PAUSE: Duration = Duration::from_millis(200);

/// Deletes the directory `path` and everything in it, trying again when it is found not empty at the end.
///
/// On macOS, unlinking entries while reading a very large directory (a `target/debug/deps` with 100,000 files) can
/// skip some, and a process left running can write into the tree while it is deleted. Either way the last `rmdir`
/// fails with "Directory not empty", and a fresh pass reads what the last one missed.
pub fn delete(path: &Path) -> std::io::Result<()> {
    retry_not_empty(DELETE_TRIES, DELETE_PAUSE, || std::fs::remove_dir_all(path))
}

/// Runs `attempt` up to `tries` times while it fails with "Directory not empty", pausing between tries.
fn retry_not_empty(
    tries: u32,
    pause: Duration,
    mut attempt: impl FnMut() -> std::io::Result<()>,
) -> std::io::Result<()> {
    let mut tried = 1;
    loop {
        match attempt() {
            Err(err) if err.kind() == ErrorKind::DirectoryNotEmpty && tried < tries => {
                tried += 1;
                std::thread::sleep(pause);
            }
            done => return done,
        }
    }
}

/// The directories runs of this loop are in, or about to start in. The sweep leaves them alone.
///
/// One lock guards both the set and the sweep's rename (and the removal of an emptied parent), so a run can't start
/// in a directory at the moment the sweep takes it away: either the run registered first and the sweep skips the
/// directory, or the sweep renamed it first and the run makes a fresh one.
#[derive(Debug, Default)]
pub struct LiveDirs(Mutex<HashSet<PathBuf>>);

/// A directory in the [`LiveDirs`], until this is dropped.
#[derive(Debug)]
pub struct Live<'a> {
    dirs: &'a LiveDirs,
    dir: PathBuf,
}

impl Drop for Live<'_> {
    fn drop(&mut self) {
        self.dirs.lock().remove(&self.dir);
    }
}

impl LiveDirs {
    fn lock(&self) -> MutexGuard<'_, HashSet<PathBuf>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether a run is in `dir`.
    pub fn is_live(&self, dir: &Path) -> bool {
        self.lock().contains(dir)
    }

    /// Marks `dir` as in use. Done before the run makes or touches it.
    pub fn register(&self, dir: PathBuf) -> Live<'_> {
        self.lock().insert(dir.clone());
        Live { dirs: self, dir }
    }

    /// Renames `dir` to a name of its own under `root`, unless a run is in it, so it can be deleted without the lock
    /// held. `None` when a run is in it, in this loop or another (its marker is locked), or a run started there since
    /// it was listed (its marker was written after `listed`), or its marker is gone. Else the new path or why the
    /// rename failed.
    pub fn rename_away(&self, root: &Path, dir: &Path, listed: Option<SystemTime>) -> Option<std::io::Result<PathBuf>> {
        let live = self.lock();
        if live.contains(dir) {
            return None;
        }
        let marker = File::open(dir.join(MARKER)).ok()?;
        match marker.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Error(err)) => return Some(Err(err)),
        }
        if marker.metadata().ok()?.modified().ok() != listed {
            return None;
        }
        let away = root.join(format!("{REMOVING}{}", unique_suffix()));
        // The lock is held across the rename, so a run that opened the marker just before waits, then finds it moved.
        let renamed = std::fs::rename(dir, &away).map(|()| away);
        drop(marker);
        Some(renamed)
    }

    /// Removes the heap directory above `dir`, then the team directory above that, each only if it is empty and no
    /// run is in or about to be in it. Ignores a failure: a directory with something in it stays.
    pub fn remove_emptied_parents(&self, dir: &Path) {
        let live = self.lock();
        let free = |parent: &Path| !live.iter().any(|live| live.starts_with(parent));
        if let Some(heap) = dir.parent().filter(|heap| free(heap))
            && std::fs::remove_dir(heap).is_ok()
            && let Some(team) = heap.parent().filter(|team| free(team))
        {
            let _ = std::fs::remove_dir(team);
        }
    }
}

/// A name part no other rename of this root, by this process or an earlier one, has had: the process, the time and a
/// count. (The `uuid` dependency here has no random IDs, and a name only has to differ.)
fn unique_suffix() -> String {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Why a directory is removed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    Closed,
    NotFound,
    Unused,
}

impl Reason {
    /// How the note after `Removed ada/work/k3x9a: ` finishes.
    pub fn text(self) -> &'static str {
        match self {
            Self::Closed => "the item is closed.",
            Self::NotFound => "the item can't be found.",
            Self::Unused => "unused for 7 days.",
        }
    }
}

/// What the sweep does with a directory.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    /// Keep it, and every directory after it: the server can't be reached, so nothing more can be asked.
    KeepAndStop,
    Remove(Reason),
}

/// Whether a directory last used `age` ago has gone unused for too long. Checked before asking the server anything.
pub fn is_unused(age: Duration) -> bool {
    age >= UNUSED_FOR
}

/// What to do with a directory that is not too old, from what reading its item answered: the time it was closed, or
/// nothing for an open item. A failure to read is never a reason to remove, except the server saying the item isn't
/// there: a permission change or a bad answer must not delete someone's work.
pub fn verdict(read: Result<Option<i64>, &CliError>) -> Verdict {
    match read {
        Ok(Some(_closed_at)) => Verdict::Remove(Reason::Closed),
        Ok(None) => Verdict::Keep,
        Err(err) if err.problem_status() == Some(404) => Verdict::Remove(Reason::NotFound),
        Err(err) if is_unreachable(err) => Verdict::KeepAndStop,
        Err(_) => Verdict::Keep,
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;
    use crate::client::ClientError;

    fn problem(status: u16) -> ClientError {
        ClientError::Problem {
            problem: Box::new(api_types::problem::Problem {
                code: "some_code".to_owned(),
                title: "Refused".to_owned(),
                status,
                field: None,
                hint: None,
                item_key: None,
                highest: None,
                request_id: None,
            }),
            body: String::new(),
        }
    }

    fn refused(status: u16) -> CliError {
        CliError::Client(problem(status))
    }

    fn unreachable_error() -> CliError {
        // The cheapest `reqwest::Error` to make: a request to an address that isn't one.
        let source = reqwest::Client::new().get("not a url").build().map(|_| ()).unwrap_err();
        CliError::Client(ClientError::Unreachable {
            server: "http://127.0.0.1:1".to_owned(),
            source,
        })
    }

    #[gtest]
    fn a_directory_is_unused_from_seven_days_on() {
        expect_that!(is_unused(UNUSED_FOR - Duration::from_secs(1)), eq(false));
        expect_that!(is_unused(UNUSED_FOR), eq(true));
        expect_that!(is_unused(UNUSED_FOR * 2), eq(true));
        expect_that!(is_unused(Duration::ZERO), eq(false));
    }

    #[gtest]
    fn a_closed_item_removes_its_directory() {
        expect_that!(verdict(Ok(Some(1_000))), eq(&Verdict::Remove(Reason::Closed)));
    }

    #[gtest]
    fn an_open_item_keeps_its_directory() {
        expect_that!(verdict(Ok(None)), eq(&Verdict::Keep));
    }

    #[gtest]
    fn an_item_the_server_cannot_find_removes_its_directory() {
        expect_that!(verdict(Err(&refused(404))), eq(&Verdict::Remove(Reason::NotFound)));
    }

    #[gtest]
    fn the_item_not_found_error_of_a_read_counts_as_not_found_too() {
        let err = CliError::NoSuchItem {
            name: "k3x9a".to_owned(),
            heap: "ada/work".to_owned(),
            source: problem(404),
        };

        expect_that!(verdict(Err(&err)), eq(&Verdict::Remove(Reason::NotFound)));
    }

    #[gtest]
    fn any_other_failure_keeps_the_directory() {
        for status in [500, 401, 403, 409] {
            expect_that!(verdict(Err(&refused(status))), eq(&Verdict::Keep));
        }
        expect_that!(verdict(Err(&CliError::NoAgents)), eq(&Verdict::Keep));
    }

    #[gtest]
    fn a_server_out_of_reach_keeps_the_directory_and_stops_the_sweep() {
        expect_that!(verdict(Err(&unreachable_error())), eq(&Verdict::KeepAndStop));
    }

    #[gtest]
    fn only_a_safe_team_heap_and_key_make_a_path() {
        let root = Path::new("/runs");

        expect_that!(
            item_directory(root, "ada", "work", "k3x9a"),
            ok(eq(&PathBuf::from("/runs/ada/work/k3x9a")))
        );
        for (team, heap, key) in [
            ("..", "work", "k3x9a"),
            ("ada", "..", "k3x9a"),
            ("ada", "work", ".."),
            ("a/b", "work", "k3x9a"),
            ("ada", "w/k", "k3x9a"),
            ("ada", "work", "k/3"),
            ("", "work", "k3x9a"),
            ("ada", "", "k3x9a"),
            ("ada", "work", ""),
            ("Ada", "work", "k3x9a"),
            ("ada", "Work", "k3x9a"),
            ("ada", "work", "k3x-a"),
        ] {
            let made = item_directory(root, team, heap, key);
            expect_that!(
                made.as_ref().map_err(Error::kind),
                err(eq(ErrorKind::InvalidInput)),
                "{team:?} {heap:?} {key:?}"
            );
        }
    }

    /// A kept directory under `root`, with a marker.
    fn keep(root: &Path, parts: &[&str]) -> std::io::Result<PathBuf> {
        let dir = parts.iter().fold(root.to_owned(), |path, part| path.join(part));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(MARKER), "run x\n")?;
        Ok(dir)
    }

    /// When the marker in `dir` was last written.
    fn modified(dir: &Path) -> Option<SystemTime> {
        std::fs::metadata(dir.join(MARKER)).ok()?.modified().ok()
    }

    #[gtest]
    fn a_directory_whose_marker_another_run_holds_is_not_renamed_away() -> googletest::Result<()> {
        // The marker is locked as a run in another loop on the same root holds it: this loop's set doesn't know it.
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");
        let marker = prepare(&dir, "run-1", "t", "s")?;
        let listed = modified(&dir);
        let live = LiveDirs::default();

        verify_that!(live.rename_away(root.path(), &dir, listed).is_none(), eq(true))?;
        drop(marker);
        verify_that!(live.rename_away(root.path(), &dir, listed).is_some(), eq(true))
    }

    #[gtest]
    fn a_directory_a_run_started_in_since_it_was_listed_is_not_renamed_away() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        let listed = SystemTime::now() - Duration::from_secs(8 * 24 * 60 * 60);
        std::fs::File::options()
            .write(true)
            .open(dir.join(MARKER))?
            .set_modified(listed)?;
        // Another run starts there after the sweep listed it.
        drop(prepare(&dir, "run-2", "t", "s")?);

        verify_that!(
            LiveDirs::default()
                .rename_away(root.path(), &dir, Some(listed))
                .is_none(),
            eq(true)
        )?;
        verify_that!(dir.join(MARKER).is_file(), eq(true))
    }

    #[gtest]
    fn preparing_after_the_directory_was_renamed_away_makes_a_fresh_one() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");
        drop(prepare(&dir, "run-1", "t", "s")?);
        let live = LiveDirs::default();
        let away = live.rename_away(root.path(), &dir, modified(&dir)).expect("not live")?;
        live.remove_emptied_parents(&dir);

        let marker = prepare(&dir, "run-2", "t", "s")?;

        verify_that!(dir.join(MARKER).is_file(), eq(true))?;
        verify_that!(away.join(MARKER).is_file(), eq(true))?;
        verify_that!(is_same_file(&marker, &dir.join(MARKER))?, eq(true))
    }

    #[gtest]
    fn preparing_while_a_sweep_renames_the_directory_away_makes_a_fresh_one() -> googletest::Result<()> {
        // Arrange: a sweep holds the marker's lock; a run starts preparing and waits on it.
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");
        drop(prepare(&dir, "run-1", "t", "s")?);
        let sweeping = File::open(dir.join(MARKER))?;
        sweeping.try_lock()?;
        let preparing = {
            let dir = dir.clone();
            std::thread::spawn(move || prepare(&dir, "run-2", "t", "s"))
        };
        std::thread::sleep(Duration::from_millis(200));

        // Act: the sweep renames the directory away and lets go.
        let away = root.path().join(format!("{REMOVING}x"));
        std::fs::rename(&dir, &away)?;
        drop(sweeping);
        let marker = preparing.join().expect("prepare panicked")?;

        // Assert: the run holds the marker of a new directory at the item's path, not the one taken away.
        verify_that!(is_same_file(&marker, &dir.join(MARKER))?, eq(true))?;
        verify_that!(
            std::fs::read_to_string(away.join(MARKER))?,
            eq("run run-1\nstarted t\nserver s\n")
        )
    }

    #[gtest]
    fn kept_directories_have_a_marker_and_sit_at_team_heap_key() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        // None of these is kept: no marker, an old run directory, a stray file, a marker at the wrong depth, and
        // names that are not safe path parts.
        std::fs::create_dir_all(root.path().join("ada/work/nomarker"))?;
        keep(root.path(), &["k3x9a-0191d3c2-7b1e-7a60-8a43-5b2f4a1c9d10"])?;
        keep(root.path(), &["ada", "k3x9a"])?;
        keep(root.path(), &["ada", "work", "k3x9a", "deeper"])?;
        keep(root.path(), &["Ada", "work", "k3x9a"])?;
        keep(root.path(), &["ada", "work", "k3-9a"])?;
        std::fs::write(root.path().join("stray"), "")?;
        std::fs::write(root.path().join("ada/work/stray"), "")?;
        // A marker that is a directory is not a marker.
        std::fs::create_dir_all(root.path().join("ada/work/k4x9a").join(MARKER))?;

        let kept = list(root.path(), SystemTime::now());

        verify_that!(
            kept.iter().map(|kept| kept.dir.clone()).collect::<Vec<_>>(),
            unordered_elements_are![eq(&dir)]
        )?;
        verify_that!(kept[0].name(), eq("ada/work/k3x9a"))
    }

    #[cfg(unix)]
    #[gtest]
    fn symlinks_are_never_kept() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let outside = tempfile::tempdir()?;
        let real = keep(outside.path(), &["real"])?;
        std::fs::create_dir_all(root.path().join("ada/work"))?;
        // The key, the heap and the team are each a link to a real directory with a marker below it.
        std::os::unix::fs::symlink(&real, root.path().join("ada/work/k3x9a"))?;
        keep(outside.path(), &["h", "k3x9a"])?;
        std::os::unix::fs::symlink(outside.path().join("h"), root.path().join("ada/linked"))?;
        keep(outside.path(), &["t", "work", "k3x9a"])?;
        std::os::unix::fs::symlink(outside.path().join("t"), root.path().join("linked"))?;

        verify_that!(list(root.path(), SystemTime::now()), is_empty())
    }

    #[gtest]
    fn age_is_the_markers_age_and_a_marker_from_the_future_is_new() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let old = keep(root.path(), &["ada", "work", "aaaaa"])?;
        let future = keep(root.path(), &["ada", "work", "bbbbb"])?;
        let now = SystemTime::now();
        let set = |dir: &Path, to: SystemTime| -> std::io::Result<()> {
            std::fs::File::options()
                .write(true)
                .open(dir.join(MARKER))?
                .set_modified(to)
        };
        set(&old, now - Duration::from_secs(8 * 24 * 60 * 60))?;
        set(&future, now + Duration::from_secs(3600))?;

        let mut kept = list(root.path(), now);
        kept.sort_by(|a, b| a.key.cmp(&b.key));

        verify_that!(
            kept.iter().map(|kept| is_unused(kept.age)).collect::<Vec<_>>(),
            eq(&[true, false])
        )?;
        verify_that!(kept[1].age, eq(Duration::ZERO))
    }

    #[gtest]
    fn preparing_makes_the_directory_and_rewrites_the_marker_but_leaves_the_rest() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");

        drop(prepare(
            &dir,
            "run-1",
            "2026-11-12T10:00:00Z",
            "https://upheap.example",
        )?);
        std::fs::write(dir.join("left.txt"), "kept")?;
        drop(prepare(
            &dir,
            "run-2",
            "2026-11-12T11:00:00Z",
            "https://upheap.example",
        )?);

        verify_that!(
            std::fs::read_to_string(dir.join(MARKER))?,
            eq("run run-2\nstarted 2026-11-12T11:00:00Z\nserver https://upheap.example\n")
        )?;
        verify_that!(
            list(root.path(), SystemTime::now())[0].server.as_deref(),
            some(eq("https://upheap.example"))
        )?;
        verify_that!(std::fs::read_to_string(dir.join("left.txt"))?, eq("kept"))
    }

    #[gtest]
    fn a_file_where_the_directory_should_be_cannot_be_prepared() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("ada/work"))?;
        std::fs::write(root.path().join("ada/work/k3x9a"), "")?;

        verify_that!(
            prepare(&root.path().join("ada/work/k3x9a"), "r", "t", "s"),
            err(anything())
        )
    }

    #[gtest]
    fn leftovers_are_the_renamed_directories_only() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let left = root.path().join(format!("{REMOVING}abc"));
        std::fs::create_dir(&left)?;
        std::fs::create_dir(root.path().join("ada"))?;

        verify_that!(leftovers(root.path()), eq(&[left]))
    }

    /// An attempt that fails with `errors` in turn, then succeeds, counting its calls in `calls`.
    fn failing(calls: &mut u32, errors: &[ErrorKind]) -> std::io::Result<()> {
        let at = *calls as usize;
        *calls += 1;
        errors.get(at).map_or(Ok(()), |kind| Err(Error::from(*kind)))
    }

    #[gtest]
    fn a_delete_found_not_empty_is_tried_again_until_it_succeeds() -> googletest::Result<()> {
        let mut calls = 0;
        let not_empty = [ErrorKind::DirectoryNotEmpty, ErrorKind::DirectoryNotEmpty];

        let deleted = retry_not_empty(3, Duration::ZERO, || failing(&mut calls, &not_empty));

        verify_that!(deleted, ok(()))?;
        verify_that!(calls, eq(3))
    }

    #[gtest]
    fn a_delete_still_not_empty_after_every_try_fails_with_that_error() -> googletest::Result<()> {
        let mut calls = 0;
        let not_empty = [ErrorKind::DirectoryNotEmpty; 5];

        let deleted = retry_not_empty(3, Duration::ZERO, || failing(&mut calls, &not_empty));

        verify_that!(deleted.map_err(|err| err.kind()), err(eq(ErrorKind::DirectoryNotEmpty)))?;
        verify_that!(calls, eq(3))
    }

    #[gtest]
    fn a_delete_that_fails_for_another_reason_is_not_tried_again() -> googletest::Result<()> {
        let mut calls = 0;

        let deleted = retry_not_empty(3, Duration::ZERO, || {
            failing(&mut calls, &[ErrorKind::PermissionDenied])
        });

        verify_that!(deleted.map_err(|err| err.kind()), err(eq(ErrorKind::PermissionDenied)))?;
        verify_that!(calls, eq(1))
    }

    #[gtest]
    fn a_live_directory_is_not_renamed_away_and_keeps_its_parents() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let live_dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        let live = LiveDirs::default();
        let _guard = live.register(live_dir.clone());

        verify_that!(
            live.rename_away(root.path(), &live_dir, modified(&live_dir)).is_none(),
            eq(true)
        )?;
        live.remove_emptied_parents(&live_dir.join("nothing"));
        verify_that!(live_dir.is_dir(), eq(true))
    }

    #[gtest]
    fn a_directory_is_renamed_away_once_its_run_is_over_and_its_empty_parents_go() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        let other = keep(root.path(), &["ada", "house", "aaaaa"])?;
        let live = LiveDirs::default();
        drop(live.register(dir.clone()));

        let away = live.rename_away(root.path(), &dir, modified(&dir)).expect("not live")?;
        live.remove_emptied_parents(&dir);

        verify_that!(dir.exists(), eq(false))?;
        verify_that!(away.join(MARKER).is_file(), eq(true))?;
        verify_that!(away.parent(), some(eq(root.path())))?;
        verify_that!(root.path().join("ada/work").exists(), eq(false))?;
        // The team still holds another heap, so it stays.
        verify_that!(other.is_dir(), eq(true))
    }

    #[gtest]
    fn a_parent_with_a_run_about_to_start_below_it_stays() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        let live = LiveDirs::default();
        let _next = live.register(root.path().join("ada/work/bbbbb"));
        let away = live.rename_away(root.path(), &dir, modified(&dir)).expect("not live")?;
        drop(away);

        live.remove_emptied_parents(&dir);

        verify_that!(root.path().join("ada/work").is_dir(), eq(true))
    }
}
