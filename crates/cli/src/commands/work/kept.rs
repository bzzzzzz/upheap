//! The directories `work run --root` keeps, one for each item: where one is, the marker that says when a run last
//! started in it, which ones the sweep looks at, and whether it keeps or removes each. No server and no clock here:
//! the loop reads the item and passes in the time, so every rule below is tested with plain values.
//!
//! The marker also says who made the directory (a server and a user), so a root shared by two servers or accounts
//! never has one's directories judged by the other's answers.
//!
//! The path is the whole record. There is no index file to fall out of step with the directories.

use std::collections::HashSet;
use std::io::{Error, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, SystemTime};

use domain::handle::Handle;
use domain::validation::Field;

use super::run::is_unreachable;
use crate::CliError;

/// The file a run writes into its directory as it starts. Its modification time is when a run last started there.
///
/// An agent that deletes it makes the sweep ignore the directory until the next run on the item writes it again:
/// without it, nothing says the directory is ours to remove.
pub const MARKER: &str = ".upheap-run";

/// How long a directory can go without a run before the sweep removes it. [`Reason::Unused`]'s note names it in words
/// ("7 days"), so change that text with this.
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

/// Who made a directory: the server the loop talks to and the user signed in to it at the time, read from `GET /me`
/// at each sweep and run start, since a sign-in can change under a running loop. Only a directory made by the current
/// owner is ever asked about, since another server or account answers 404 for items that are fine; one made by
/// another is never used by a run, and only removed once unused for 7 days.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owner {
    server: String,
    user: String,
}

impl Owner {
    /// `server` is the server's URL as the loop uses it, without a trailing `/`; `user` is the user's id.
    pub fn new(server: &str, user: &str) -> Self {
        Self {
            server: server.trim_end_matches('/').to_owned(),
            user: user.to_owned(),
        }
    }

    /// The marker's text. One `name value` per line.
    fn marker(&self, run_id: &str, started: &str) -> String {
        format!(
            "version {MARKER_VERSION}\nserver {}\nuser {}\nrun {run_id}\nstarted {started}\n",
            self.server, self.user
        )
    }

    /// Whether `marker`, a marker's text, names this owner. Tolerant: lines it doesn't know are ignored, and a missing
    /// or different `version`, server or user means no.
    fn made(&self, marker: &str) -> bool {
        named_in(marker).is_some_and(|(server, user)| server == self.server && user == self.user)
    }

    /// Whether `marker` names another owner: a version-1 marker with both owner lines, which are not this owner's.
    /// A marker with no owner in it, or none at all, holds nothing of anyone's that this owner could take over.
    fn conflicts(&self, marker: &str) -> bool {
        named_in(marker).is_some_and(|(server, user)| server != self.server || user != self.user)
    }
}

/// The server and user a marker's text names, or `None` unless it is version 1 and has both lines.
fn named_in(marker: &str) -> Option<(&str, &str)> {
    let (mut version, mut server, mut user) = (None, None, None);
    for line in marker.lines() {
        match line.split_once(' ') {
            Some(("version", value)) => version = Some(value.trim()),
            Some(("server", value)) => server = Some(value.trim()),
            Some(("user", value)) => user = Some(value.trim()),
            _ => {}
        }
    }
    if version == Some(MARKER_VERSION) {
        server.zip(user)
    } else {
        None
    }
}

/// The marker's text, up to [`MARKER_READ_LIMIT`], or `None` if it can't be read.
fn read_marker(marker: &Path) -> Option<String> {
    let file = std::fs::File::open(marker).ok()?;
    let mut text = String::new();
    file.take(MARKER_READ_LIMIT).read_to_string(&mut text).ok()?;
    Some(text)
}

/// The marker's format version. Any other is read as not matching.
const MARKER_VERSION: &str = "1";

/// The most of a marker that is read: it is a few lines, and what is past this is not ours.
const MARKER_READ_LIMIT: u64 = 4096;

/// What [`prepare`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Prepared {
    /// The directory is the owner's to use, and its marker says a run started now.
    Ready,
    /// The directory's marker names another server or account, so it holds someone else's work. Nothing was written.
    Held,
}

/// Makes `dir` and its parents if they aren't there, and writes the marker. A directory that is there is used as it
/// is, unless its marker names another owner: that one is [`Prepared::Held`] and left untouched. `started` is an
/// RFC 3339 UTC time, for a person reading the file.
pub fn prepare(dir: &Path, owner: &Owner, run_id: &str, started: &str) -> std::io::Result<Prepared> {
    std::fs::create_dir_all(dir)?;
    let marker = dir.join(MARKER);
    if read_marker(&marker).is_some_and(|text| owner.conflicts(&text)) {
        return Ok(Prepared::Held);
    }
    // Written whole every time, so its modification time moves even though the text may not.
    std::fs::write(marker, owner.marker(run_id, started))?;
    Ok(Prepared::Ready)
}

/// A directory the sweep looks at.
#[derive(Debug, PartialEq, Eq)]
pub struct Kept {
    pub dir: PathBuf,
    pub team: String,
    pub heap: String,
    pub key: String,
    /// How long ago a run last started here. Zero for a marker dated in the future, as after a clock change.
    pub age: Duration,
    /// Whether the marker names the owner `list` was given (none, if the sweep couldn't tell who is signed in). Only
    /// then may the server's answer remove it.
    pub mine: bool,
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
pub fn list(root: &Path, now: SystemTime, owner: Option<&Owner>) -> Vec<Kept> {
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
                let age = metadata
                    .modified()
                    .ok()
                    .and_then(|modified| now.duration_since(modified).ok())
                    .unwrap_or_default();
                let mine = owner.is_some_and(|owner| read_marker(&marker).is_some_and(|text| owner.made(&text)));
                kept.push(Kept {
                    dir,
                    team: team.clone(),
                    heap: heap.clone(),
                    key,
                    age,
                    mine,
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
    /// held. `None` when a run is in it, else the new path or why the rename failed.
    pub fn rename_away(&self, root: &Path, dir: &Path) -> Option<std::io::Result<PathBuf>> {
        let live = self.lock();
        if live.contains(dir) {
            return None;
        }
        let away = root.join(format!("{REMOVING}{}", unique_suffix()));
        Some(std::fs::rename(dir, &away).map(|()| away))
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
/// nothing for an open item. Only the server answering 404 removes: the item is gone, moved to another heap, or in a
/// heap the person can no longer see (the server answers 404 for that too). Any other failure keeps the directory,
/// 403 and a bad answer included. The caller asks only about directories made through this server by the account signed in at the time.
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

    fn owner() -> Owner {
        Owner::new("http://upheap.test/", "u-1")
    }

    #[gtest]
    fn a_marker_made_for_an_owner_matches_only_that_owner() {
        let marker = owner().marker("run-1", "2026-11-12T10:00:00Z");

        expect_that!(owner().made(&marker), eq(true));
        expect_that!(Owner::new("http://other.test", "u-1").made(&marker), eq(false));
        expect_that!(Owner::new("http://upheap.test", "u-2").made(&marker), eq(false));
    }

    #[gtest]
    fn a_marker_is_read_tolerantly_but_only_in_version_1_with_both_owner_lines() {
        let owner = owner();
        let with_extra = "version 1\nfuture thing\nserver http://upheap.test\nuser u-1\nrun x\n";
        let no_owner = "run x\nstarted y\n";
        let no_user = "version 1\nserver http://upheap.test\nrun x\n";
        let wrong_version = "version 2\nserver http://upheap.test\nuser u-1\n";
        let no_version = "server http://upheap.test\nuser u-1\n";

        expect_that!(owner.made(with_extra), eq(true));
        expect_that!(owner.made(no_owner), eq(false));
        expect_that!(owner.made(no_user), eq(false));
        expect_that!(owner.made(wrong_version), eq(false));
        expect_that!(owner.made(no_version), eq(false));
        expect_that!(owner.made(""), eq(false));
    }

    #[gtest]
    fn only_a_marker_naming_another_owner_conflicts() {
        let owner = owner();
        let other_server = "version 1\nserver http://other.test\nuser u-1\n";
        let other_user = "version 1\nserver http://upheap.test\nuser u-2\n";
        let mine = "version 1\nserver http://upheap.test\nuser u-1\nrun x\n";

        expect_that!(owner.conflicts(other_server), eq(true));
        expect_that!(owner.conflicts(other_user), eq(true));
        expect_that!(owner.conflicts(mine), eq(false));
        expect_that!(owner.conflicts("run x\nstarted y\n"), eq(false));
        expect_that!(owner.conflicts("version 1\nserver http://other.test\n"), eq(false));
        expect_that!(
            owner.conflicts("version 2\nserver http://other.test\nuser u-1\n"),
            eq(false)
        );
        expect_that!(owner.conflicts(""), eq(false));
    }

    #[gtest]
    fn preparing_leaves_a_directory_held_by_another_owner_untouched() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");
        prepare(&dir, &Owner::new("http://other.test", "u-1"), "r1", "t1")?;
        let before = std::fs::read_to_string(dir.join(MARKER))?;

        let prepared = prepare(&dir, &owner(), "r2", "t2")?;

        verify_that!(prepared, eq(&Prepared::Held))?;
        verify_that!(std::fs::read_to_string(dir.join(MARKER))?, eq(&before))
    }

    #[gtest]
    fn preparing_takes_a_directory_without_an_owner_in_its_marker() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let dir = root.path().join("ada/work/k3x9a");
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(MARKER), "run x\nstarted y\n")?;

        let prepared = prepare(&dir, &owner(), "r2", "t2")?;

        verify_that!(prepared, eq(&Prepared::Ready))?;
        verify_that!(owner().made(&std::fs::read_to_string(dir.join(MARKER))?), eq(true))
    }

    #[gtest]
    fn a_kept_directory_is_mine_only_when_its_marker_names_the_owner() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let mine = root.path().join("ada/work/aaaaa");
        prepare(&mine, &owner(), "r", "t")?;
        let theirs = root.path().join("ada/work/bbbbb");
        prepare(&theirs, &Owner::new("http://other.test", "u-1"), "r", "t")?;
        keep(root.path(), &["ada", "work", "ccccc"])?;

        let mut kept = list(root.path(), SystemTime::now(), Some(&owner()));
        kept.sort_by(|a, b| a.key.cmp(&b.key));

        verify_that!(
            kept.iter().map(|kept| kept.mine).collect::<Vec<_>>(),
            eq(&[true, false, false])
        )
    }

    /// A kept directory under `root`, with a marker.
    fn keep(root: &Path, parts: &[&str]) -> std::io::Result<PathBuf> {
        let dir = parts.iter().fold(root.to_owned(), |path, part| path.join(part));
        std::fs::create_dir_all(&dir)?;
        std::fs::write(dir.join(MARKER), "run x\n")?;
        Ok(dir)
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

        let kept = list(root.path(), SystemTime::now(), Some(&owner()));

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

        verify_that!(list(root.path(), SystemTime::now(), Some(&owner())), is_empty())
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

        let mut kept = list(root.path(), now, Some(&owner()));
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

        prepare(&dir, &owner(), "run-1", "2026-11-12T10:00:00Z")?;
        std::fs::write(dir.join("left.txt"), "kept")?;
        prepare(&dir, &owner(), "run-2", "2026-11-12T11:00:00Z")?;

        verify_that!(
            std::fs::read_to_string(dir.join(MARKER))?,
            eq("version 1\nserver http://upheap.test\nuser u-1\nrun run-2\nstarted 2026-11-12T11:00:00Z\n")
        )?;
        verify_that!(std::fs::read_to_string(dir.join("left.txt"))?, eq("kept"))
    }

    #[gtest]
    fn a_file_where_the_directory_should_be_cannot_be_prepared() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        std::fs::create_dir_all(root.path().join("ada/work"))?;
        std::fs::write(root.path().join("ada/work/k3x9a"), "")?;

        verify_that!(
            prepare(&root.path().join("ada/work/k3x9a"), &owner(), "r", "t"),
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

    #[gtest]
    fn a_live_directory_is_not_renamed_away_and_keeps_its_parents() -> googletest::Result<()> {
        let root = tempfile::tempdir()?;
        let live_dir = keep(root.path(), &["ada", "work", "k3x9a"])?;
        let live = LiveDirs::default();
        let _guard = live.register(live_dir.clone());

        verify_that!(live.rename_away(root.path(), &live_dir).is_none(), eq(true))?;
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

        let away = live.rename_away(root.path(), &dir).expect("not live")?;
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
        let away = live.rename_away(root.path(), &dir).expect("not live")?;
        drop(away);

        live.remove_emptied_parents(&dir);

        verify_that!(root.path().join("ada/work").is_dir(), eq(true))
    }
}
