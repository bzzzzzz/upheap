//! The macOS keychain through `/usr/bin/security`, the system's own command-line tool.
//!
//! A keychain entry lets the program that made it read it without asking, and macOS tells programs apart by their
//! code signature. An entry `upheap` made itself was therefore closed to the next build of `upheap`: every upgrade
//! of an unsigned binary, and every rebuild from source, put a dialog in front of the user, and a command run by a
//! script or an agent waited on it for ever. An entry made by `security` trusts `security`, which is the same
//! program whatever calls it. The cost is that any program running as the user can read the entry the same way,
//! without a dialog; the keychain still keeps it encrypted at rest and away from other users.
//!
//! The secret never goes on a command line, where `ps` would show it: it's written to the tool's standard input.

use std::ffi::OsString;
use std::io::Write;
use std::process::{Command, Stdio};

/// What `security` exits with when no entry matches (`errSecItemNotFound`, 25300, modulo 256).
const NOT_FOUND: i32 = 44;

/// The longest line sent to `security -i`. It reads its commands in lines of 4096 bytes and runs what's left of a
/// longer one as a command of its own, having stored a cut-off secret. A sign-in is a few hundred bytes.
const LONGEST_LINE: usize = 2048;

#[derive(Debug, thiserror::Error)]
pub enum SecurityToolError {
    #[error("can't run {program}")]
    Start {
        program: String,
        #[source]
        source: std::io::Error,
    },
    #[error("`security {command}` failed{}: {stderr}", exit_text(*code))]
    Failed {
        command: &'static str,
        code: Option<i32>,
        stderr: String,
    },
    #[error("{what} can't be written to the keychain: it has a quote, a backslash or a control character")]
    Unquotable { what: &'static str },
    #[error("the sign-in is too long to write to the keychain")]
    TooLong,
}

fn exit_text(code: Option<i32>) -> String {
    code.map(|code| format!(" with exit code {code}")).unwrap_or_default()
}

/// The `security` program, and how to start it.
pub struct SecurityTool {
    program: OsString,
    /// Arguments that go before the tool's own. Empty for the real tool; a test runs a script through `sh`.
    leading: Vec<OsString>,
}

impl SecurityTool {
    /// The system's tool, by its full path, so nothing on `PATH` can stand in for it.
    #[cfg(target_os = "macos")]
    pub fn system() -> Self {
        Self {
            program: "/usr/bin/security".into(),
            leading: Vec::new(),
        }
    }

    /// The secret of the generic password `service` holds for `account`, as the tool prints it: as written when
    /// it's printable text, in hexadecimal otherwise. `None` when there's no such entry.
    pub fn find(&self, service: &str, account: &str) -> Result<Option<String>, SecurityToolError> {
        let output = self.run(
            "find-generic-password",
            &["find-generic-password", "-s", service, "-a", account, "-w"],
            None,
        )?;
        Ok(output.map(|stdout| stdout.trim_end_matches('\n').to_owned()))
    }

    /// Writes `secret` as the generic password `service` holds for `account`, replacing one that's there.
    ///
    /// The old entry is removed first, not updated in place: an update keeps the list of programs the entry trusts,
    /// so one made by an `upheap` that used the keychain itself would go on asking. Removing asks nothing, and the
    /// new entry is `security`'s. The two steps aren't one: if the write fails after the removal the sign-in is
    /// gone, so what can be checked is checked before either.
    pub fn add(&self, service: &str, account: &str, secret: &[u8]) -> Result<(), SecurityToolError> {
        let line = add_line(service, account, secret)?;
        self.delete(service, account)?;
        self.run("add-generic-password", &["-i"], Some(&line)).map(|_| ())
    }

    /// Removes the generic password `service` holds for `account`. Removing one that isn't there succeeds.
    pub fn delete(&self, service: &str, account: &str) -> Result<(), SecurityToolError> {
        self.run(
            "delete-generic-password",
            &["delete-generic-password", "-s", service, "-a", account],
            None,
        )
        .map(|_| ())
    }

    /// Runs the tool with `args`, writing `input` to its standard input. Its standard output, or `None` when it
    /// says there's no such entry.
    fn run(
        &self,
        command: &'static str,
        args: &[&str],
        input: Option<&str>,
    ) -> Result<Option<String>, SecurityToolError> {
        let start = |source| SecurityToolError::Start {
            program: self.program.to_string_lossy().into_owned(),
            source,
        };
        let mut child = Command::new(&self.program)
            .args(&self.leading)
            .args(args)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(start)?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            // Dropped at the end of the block, which closes the pipe, so the tool sees the end of its commands.
            stdin.write_all(input.as_bytes()).map_err(start)?;
        }
        let output = child.wait_with_output().map_err(start)?;
        match output.status.code() {
            Some(0) => Ok(Some(String::from_utf8_lossy(&output.stdout).into_owned())),
            Some(NOT_FOUND) => Ok(None),
            code => Err(SecurityToolError::Failed {
                command,
                code,
                stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            }),
        }
    }
}

/// The line that tells `security -i` to write `secret`. The secret goes in hexadecimal, so nothing in it can end the
/// line or a quoted word; the tool stores the bytes themselves.
fn add_line(service: &str, account: &str, secret: &[u8]) -> Result<String, SecurityToolError> {
    let line = format!(
        "add-generic-password -U -s {} -a {} -X {}\n",
        quoted(service, "the keychain service's name")?,
        quoted(account, "the server's address")?,
        hex(secret)
    );
    if line.len() > LONGEST_LINE {
        return Err(SecurityToolError::TooLong);
    }
    Ok(line)
}

/// `text` as one word for `security -i`, which splits its lines as a shell does. What would need escaping is
/// refused instead: a newline would start a second command, and a server's address never has one.
fn quoted(text: &str, what: &'static str) -> Result<String, SecurityToolError> {
    if text.is_empty() || text.chars().any(|c| c == '"' || c == '\\' || c.is_control()) {
        return Err(SecurityToolError::Unquotable { what });
    }
    Ok(format!("\"{text}\""))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests;
