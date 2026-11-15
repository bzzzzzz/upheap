//! Ending what an agent run left running: a dev server, a watcher or a build daemon the tool started and didn't end.
//! They are found by the `UPHEAP_RUN` entry in their environment, which every process the tool starts inherits and
//! which survives `setsid` and reparenting, where a process group or a process tree doesn't. A directory being removed
//! is cleared by the working directory of its processes instead, which also covers runs from before this existed.
//! Only processes of the user running `upheap` are signalled, and never `upheap` itself.

use std::path::Path;

/// A process that was ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ended {
    pub pid: u32,
    pub name: String,
    /// It was still there after the grace and needed `SIGKILL`.
    pub killed: bool,
}

/// Ends every process whose environment has `UPHEAP_RUN=<run_id>`, and returns them. Blocks for up to the grace.
#[cfg(unix)]
pub fn end_run(run_id: &str) -> Vec<Ended> {
    use sysinfo::{ProcessRefreshKind, UpdateKind};

    let entry = format!("UPHEAP_RUN={run_id}");
    // Compared whole: `UPHEAP_RUN=ab` is not run `a`'s.
    let ended = end(
        ProcessRefreshKind::nothing()
            .with_environ(UpdateKind::Always)
            .with_user(UpdateKind::Always),
        |process| {
            process
                .environ()
                .iter()
                .any(|var| var.as_encoded_bytes() == entry.as_bytes())
        },
    );
    for process in &ended {
        tracing::info!(run_id, pid = process.pid, name = %process.name, killed = process.killed, "ended a process the run left behind");
    }
    ended
}

/// Ends every process whose working directory is `dir` or inside it, and returns them. Blocks for up to the grace.
#[cfg(unix)]
pub fn end_inside(dir: &Path) -> Vec<Ended> {
    use sysinfo::{ProcessRefreshKind, UpdateKind};

    // The system may report another spelling of the same place, such as `/private/var` for `/var` on macOS.
    let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_owned());
    let ended = end(
        ProcessRefreshKind::nothing()
            .with_cwd(UpdateKind::Always)
            .with_user(UpdateKind::Always),
        |process| process.cwd().is_some_and(|cwd| cwd.starts_with(&dir)),
    );
    for process in &ended {
        tracing::info!(dir = %dir.display(), pid = process.pid, name = %process.name, killed = process.killed, "ended a process working in a directory being removed");
    }
    ended
}

#[cfg(not(unix))]
pub fn end_run(_run_id: &str) -> Vec<Ended> {
    Vec::new()
}

#[cfg(not(unix))]
pub fn end_inside(_dir: &Path) -> Vec<Ended> {
    Vec::new()
}

/// How often the processes are looked for again while they end.
#[cfg(unix)]
const RESCAN_EVERY: std::time::Duration = std::time::Duration::from_millis(75);

/// Sends `TERM` to every process of this user, other than this one, that `matches`; scans again until none is left or
/// the grace is over, and sends `KILL` to what the last scan found. The scan is repeated rather than polling the pids,
/// so a pid reused meanwhile doesn't match, and a process respawned during the grace is found.
#[cfg(unix)]
fn end(refresh: sysinfo::ProcessRefreshKind, matches: impl Fn(&sysinfo::Process) -> bool) -> Vec<Ended> {
    use rustix::process::{Pid, Signal, kill_process};
    use sysinfo::{ProcessesToUpdate, System};

    // Nothing can be read, so nothing is ended: a scan never fails the run.
    let Ok(mut system) = System::new() else {
        return Vec::new();
    };
    let me = rustix::process::getuid().as_raw();
    let own_pid = std::process::id();
    let mut scan = || -> Vec<Ended> {
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, refresh);
        system
            .processes()
            .values()
            .filter(|process| process.pid().as_u32() != own_pid)
            .filter(|process| process.user_id().is_some_and(|user| **user == me))
            .filter(|process| matches(process))
            .map(|process| Ended {
                pid: process.pid().as_u32(),
                name: process.name().to_string_lossy().into_owned(),
                killed: false,
            })
            .collect()
    };
    // A process that is gone already is what was wanted, so a failed signal is ignored.
    let signal = |found: &[Ended], signal: Signal| {
        for process in found {
            if let Some(pid) = i32::try_from(process.pid).ok().and_then(Pid::from_raw) {
                let _ = kill_process(pid, signal);
            }
        }
    };

    let mut ended = scan();
    if ended.is_empty() {
        return ended;
    }
    signal(&ended, Signal::TERM);
    let deadline = std::time::Instant::now() + super::claude::STOP_GRACE;
    let last = loop {
        std::thread::sleep(RESCAN_EVERY);
        let found = scan();
        if found.is_empty() || std::time::Instant::now() >= deadline {
            break found;
        }
    };
    signal(&last, Signal::KILL);
    for process in last {
        match ended.iter_mut().find(|known| known.pid == process.pid) {
            Some(known) => known.killed = true,
            None => ended.push(Ended {
                killed: true,
                ..process
            }),
        }
    }
    ended
}

#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::{Duration, Instant};

    use googletest::prelude::*;

    use super::*;

    /// What the leftovers are: this test binary, started again to run [`stays_for_a_leftover`] and nothing else. A
    /// system `sleep` is no use: on macOS the environment of Apple's own programs can't be read, so a run's mark is
    /// never seen on them, as it is on the dev servers and watchers that are real leftovers.
    const STAYS: &str = "UPHEAP_TEST_STAYS";

    /// Not a test: the body of a leftover. Does nothing unless a test started this binary to stay.
    #[gtest]
    fn stays_for_a_leftover() {
        if std::env::var_os(STAYS).is_some() {
            std::thread::sleep(Duration::from_secs(30));
        }
    }

    /// Kills the process on drop, so a test that fails early leaves nothing behind.
    struct Leftover(u32);

    impl Drop for Leftover {
        fn drop(&mut self) {
            let _ = Command::new("kill")
                .args(["-KILL", &self.0.to_string()])
                .stderr(std::process::Stdio::null())
                .status();
        }
    }

    /// Starts a leftover with `UPHEAP_RUN=<run_id>`, detached as a real one is: in a process group of its own, its
    /// shell gone and the process reparented to init. `prefix` is run in the shell first, and its settings are kept.
    fn start_leftover(prefix: &str, run_id: &str) -> googletest::Result<Leftover> {
        let script = format!(
            "({prefix} exec \"$0\" --exact agent::leftovers::tests::stays_for_a_leftover) >/dev/null 2>&1 & echo $!"
        );
        let output = Command::new("sh")
            .arg("-c")
            .arg(script)
            .arg(std::env::current_exe()?)
            .env("UPHEAP_RUN", run_id)
            .env(STAYS, "1")
            .process_group(0)
            .output()?;
        let leftover = Leftover(String::from_utf8(output.stdout)?.trim().parse()?);
        // The shell prints the pid before it has become the leftover, and until then its environment can't be read.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !has_become_the_leftover(leftover.0) {
            verify_that!(Instant::now() < deadline, eq(true))?;
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(leftover)
    }

    fn has_become_the_leftover(pid: u32) -> bool {
        Command::new("ps")
            .args(["-o", "command=", "-p", &pid.to_string()])
            .output()
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("stays_for_a_leftover"))
    }

    fn is_alive(pid: u32) -> bool {
        Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Waits up to a couple of seconds for `pid` to be gone: init reaps it a moment after it is signalled.
    fn is_gone_soon(pid: u32) -> bool {
        let deadline = Instant::now() + Duration::from_secs(2);
        while is_alive(pid) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        true
    }

    #[gtest]
    fn a_process_of_the_run_is_ended_and_one_of_another_run_is_not() -> googletest::Result<()> {
        // Arrange
        let ours = start_leftover("", "leftovers-test-run-a")?;
        let other = start_leftover("", "leftovers-test-run-ab")?;

        // Act
        let ended = end_run("leftovers-test-run-a");

        // Assert
        verify_that!(is_gone_soon(ours.0), eq(true))?;
        verify_that!(
            ended.iter().map(|process| process.pid).collect::<Vec<_>>(),
            elements_are![eq(&ours.0)]
        )?;
        verify_that!(ended[0].killed, eq(false))?;
        verify_that!(is_alive(other.0), eq(true))
    }

    #[gtest]
    fn a_process_that_ignores_term_is_killed_after_the_grace() -> googletest::Result<()> {
        // Arrange
        let stubborn = start_leftover("trap \'\' TERM;", "leftovers-test-run-stubborn")?;

        // Act
        let ended = end_run("leftovers-test-run-stubborn");

        // Assert
        verify_that!(is_gone_soon(stubborn.0), eq(true))?;
        verify_that!(
            ended.iter().any(|process| process.pid == stubborn.0 && process.killed),
            eq(true)
        )
    }
}
