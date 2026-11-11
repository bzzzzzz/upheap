//! `upheap-server user reset-link <username or email>`: makes a one-time password reset link for a user and prints
//! it. It runs beside a server that has already migrated the database, so it doesn't migrate and doesn't listen. Only
//! the link goes to stdout, so it can be piped; everything else, logs included, goes to stderr.

use std::io::Write;
use std::process::ExitCode;

use domain::Timestamp;
use service::ServiceError;
use service::password_reset::ResetLinkMade;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::config::Config;

const USAGE: &str = "usage: upheap-server user reset-link <username or email>";

/// Exit codes, as documented in the README (Resetting a password). `0` is a link made.
const EXIT_UNEXPECTED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_NO_SUCH_USER: u8 = 3;
const EXIT_DATABASE_UNREACHABLE: u8 = 5;

/// What the arguments after `user` ask for.
#[derive(Debug, PartialEq, Eq)]
enum UserCommand {
    ResetLink { login: String },
}

/// Reads the arguments after `user`: `reset-link` and one login. Anything else is a usage error.
fn parse_args(args: &[String]) -> Result<UserCommand, &'static str> {
    match args {
        [verb, login] if verb == "reset-link" => Ok(UserCommand::ResetLink { login: login.clone() }),
        _ => Err(USAGE),
    }
}

/// Runs the command on the arguments after `user`, and returns the process's exit code.
pub async fn run(args: &[String]) -> ExitCode {
    let UserCommand::ResetLink { login } = match parse_args(args) {
        Ok(command) => command,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid configuration: {error:#}");
            return ExitCode::from(EXIT_UNEXPECTED);
        }
    };
    crate::init_tracing(config.log_format, "warn", BoxMakeWriter::new(std::io::stderr));
    let db = match store::connect(&config.database_url, config.database_max_connections).await {
        Ok(db) => db,
        Err(error) => {
            eprintln!("can't connect to the database: {error}");
            return ExitCode::from(EXIT_DATABASE_UNREACHABLE);
        }
    };
    let services = crate::build_services(&config, db);
    let login = login.trim();
    match services.create_password_reset_link(login).await {
        Ok(made) => match write_link(&mut std::io::stdout(), &mut std::io::stderr(), &made, &config.base_url) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("can't write the link: {error}");
                ExitCode::from(EXIT_UNEXPECTED)
            }
        },
        Err(ServiceError::NotFound) => {
            eprintln!("No user is called {login:?}.");
            ExitCode::from(EXIT_NO_SUCH_USER)
        }
        Err(error) => {
            eprintln!("can't make a reset link: {error}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Prints the link alone on `stdout`, and on `stderr` whom it is for and when it ends, and whether an earlier link of
/// theirs stopped working.
fn write_link(
    stdout: &mut impl Write,
    stderr: &mut impl Write,
    made: &ResetLinkMade,
    base_url: &str,
) -> std::io::Result<()> {
    writeln!(stdout, "{base_url}/reset/{}", made.token.expose())?;
    writeln!(
        stderr,
        "For {} (@{}). Works once, until {} UTC.",
        for_terminal(made.user.display_name.as_str()),
        made.user.username,
        end_time(made.expires_at),
    )?;
    if made.cancelled_earlier {
        writeln!(stderr, "An earlier reset link for them no longer works.")?;
    }
    Ok(())
}

/// `HH:MM` in UTC.
fn end_time(at: Timestamp) -> String {
    at.to_jiff().strftime("%H:%M").to_string()
}

/// `text` with every control character shown as U+FFFD, since a name comes from another person and an escape sequence
/// in it would run in the operator's terminal.
fn for_terminal(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::Duration;
    use googletest::prelude::*;
    use service::auth::Registration;
    use service::{ManualClock, Services, Settings};

    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[gtest]
    fn reset_link_with_a_login_is_understood() {
        expect_that!(
            parse_args(&args(&["reset-link", "sam"])),
            ok(eq(&UserCommand::ResetLink { login: "sam".into() }))
        );
    }

    #[gtest]
    fn everything_else_is_a_usage_error() {
        for words in [
            &[][..],
            &["reset-link"],
            &["reset-link", "sam", "extra"],
            &["frobnicate", "sam"],
            &["sam"],
        ] {
            expect_that!(parse_args(&args(words)), err(eq(&USAGE)), "{words:?}");
        }
    }

    async fn services_with_sam() -> anyhow::Result<(Services, Timestamp)> {
        let db = store::connect("sqlite::memory:", 1).await?;
        store::run_migrations(&db).await?;
        let start = Timestamp::from_millis(Duration::from_hours(10).as_millis()).expect("in range");
        let settings = Settings {
            allow_registration: true,
            ..Settings::default()
        };
        let services = Services::new(db, Arc::new(ManualClock::new(start)), settings);
        services
            .register(Registration {
                email: "sam@example.com",
                username: "sam",
                display_name: "Sam Ortiz",
                password: "correct horse battery",
            })
            .await?;
        Ok((services, start))
    }

    #[gtest]
    #[tokio::test]
    async fn the_link_is_alone_on_stdout_and_the_rest_on_stderr() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = services_with_sam().await?;
        let first = services.create_password_reset_link("sam").await?;
        let second = services.create_password_reset_link("sam").await?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());

        // Act
        write_link(&mut stdout, &mut stderr, &second, "https://upheap.example")?;

        // Assert
        let link = format!("https://upheap.example/reset/{}\n", second.token.expose());
        expect_that!(String::from_utf8(stdout)?, eq(&link));
        expect_that!(
            String::from_utf8(stderr)?,
            eq("For Sam Ortiz (@sam). Works once, until 11:00 UTC.\nAn earlier reset link for them no longer works.\n")
        );
        expect_that!(first.cancelled_earlier, eq(false));
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn the_second_stderr_line_is_only_for_a_cancelled_link() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = services_with_sam().await?;
        let made = services.create_password_reset_link("sam").await?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());

        // Act
        write_link(&mut stdout, &mut stderr, &made, "http://localhost:8080")?;

        // Assert
        expect_that!(
            String::from_utf8(stderr)?,
            eq("For Sam Ortiz (@sam). Works once, until 11:00 UTC.\n")
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn a_link_is_made_by_username_or_email_and_not_for_a_stranger() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = services_with_sam().await?;

        // Act
        let by_username = services.create_password_reset_link(" Sam ").await?;
        let by_email = services.create_password_reset_link("sam@example.com").await?;
        let unknown = services.create_password_reset_link("nobody").await;
        let with_at = services.create_password_reset_link("@sam").await;

        // Assert
        expect_that!(by_username.user.username.as_str(), eq("sam"));
        expect_that!(by_email.user.username.as_str(), eq("sam"));
        expect_that!(unknown, err(matches_pattern!(ServiceError::NotFound)));
        expect_that!(with_at, err(matches_pattern!(ServiceError::NotFound)));
        Ok(())
    }

    #[gtest]
    fn control_characters_in_a_name_are_shown_as_replacement_characters() {
        expect_that!(for_terminal("a\u{1b}[2Jb\nc"), eq("a\u{FFFD}[2Jb\u{FFFD}c"));
    }
}
