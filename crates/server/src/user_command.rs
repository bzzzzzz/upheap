//! `upheap-server user reset-link <username or email>`: makes a one-time password reset link for a user and prints
//! it. It runs beside a server that has already migrated the database, so it doesn't migrate (it stops when the
//! database is behind this version) and doesn't listen. Only the link goes to stdout, so it can be piped; everything
//! else, logs included, goes to stderr.
//!
//! `upheap-server user confirm <username or email>` confirms an account's address by hand, with the same database
//! rules, and prints what it did.
//!
//! `upheap-server user list` lists every account, one tab-separated line each. `user deactivate` and `user
//! reactivate` turn a local account off and on, and `user make-operator` and `user remove-operator` give and take
//! the operator role, which nothing else can (architecture.md, Authentication and authorization). Each prints what it
//! did, and nothing when the account was so already but a line saying so.

use std::io::Write;
use std::process::ExitCode;

use domain::Timestamp;
use service::ServiceError;
use service::email_confirmation::OperatorConfirmed;
use service::group_teams::GroupTeams;
use service::operators::{Account, AccountKind, Applied, Deactivation, Operator};
use service::password_reset::ResetLinkMade;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::config::Config;
use crate::terminal::for_terminal;

const USAGE: &str = "usage: upheap-server user reset-link <username or email>\n       upheap-server user confirm <username or email>\n       upheap-server user list\n       upheap-server user deactivate <username or email>\n       upheap-server user reactivate <username or email>\n       upheap-server user make-operator <username or email>\n       upheap-server user remove-operator <username or email>";

/// Exit codes, as documented in accounts.md (Resetting a password, Operators and accounts). `0` is a link made, an address
/// confirmed or already confirmed, or an account change made or already so.
const EXIT_UNEXPECTED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_NO_SUCH_USER: u8 = 3;
const EXIT_PROVIDER_MANAGED: u8 = 4;
const EXIT_DATABASE_UNREACHABLE: u8 = 5;

/// What the arguments after `user` ask for.
#[derive(Debug, PartialEq, Eq)]
enum UserCommand {
    ResetLink { login: String },
    Confirm { login: String },
    List,
    Deactivate { login: String },
    Reactivate { login: String },
    MakeOperator { login: String },
    RemoveOperator { login: String },
}

/// Reads the arguments after `user`: `list`, or a verb and one login. Anything else is a usage error.
fn parse_args(args: &[String]) -> Result<UserCommand, &'static str> {
    match args {
        [verb, login] if verb == "reset-link" => Ok(UserCommand::ResetLink { login: login.clone() }),
        [verb, login] if verb == "confirm" => Ok(UserCommand::Confirm { login: login.clone() }),
        [verb] if verb == "list" => Ok(UserCommand::List),
        [verb, login] if verb == "deactivate" => Ok(UserCommand::Deactivate { login: login.clone() }),
        [verb, login] if verb == "reactivate" => Ok(UserCommand::Reactivate { login: login.clone() }),
        [verb, login] if verb == "make-operator" => Ok(UserCommand::MakeOperator { login: login.clone() }),
        [verb, login] if verb == "remove-operator" => Ok(UserCommand::RemoveOperator { login: login.clone() }),
        _ => Err(USAGE),
    }
}

/// Runs the command on the arguments after `user`, and returns the process's exit code.
pub async fn run(args: &[String]) -> ExitCode {
    let command = match parse_args(args) {
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
    // The server migrates as it starts. Before it has, on this version, the tables this needs may not exist yet.
    match store::pending_migrations(&db).await {
        Ok(0) => {}
        Ok(_) => {
            eprintln!("The database isn't up to date for this version. Start the server on this version first.");
            return ExitCode::from(EXIT_UNEXPECTED);
        }
        Err(error) => {
            eprintln!("can't read the database's migrations: {error}");
            return ExitCode::from(EXIT_DATABASE_UNREACHABLE);
        }
    }
    let services = crate::build_services(&config, GroupTeams::default(), db);
    match command {
        UserCommand::ResetLink { login } => reset_link(&services, &login, &config.base_url).await,
        UserCommand::Confirm { login } => confirm(&services, &login).await,
        UserCommand::List => list(&services).await,
        UserCommand::Deactivate { login } => deactivate(&services, &login).await,
        UserCommand::Reactivate { login } => reactivate(&services, &login).await,
        UserCommand::MakeOperator { login } => make_operator(&services, &login).await,
        UserCommand::RemoveOperator { login } => remove_operator(&services, &login).await,
    }
}

/// Prints every account.
async fn list(services: &service::Services) -> ExitCode {
    match services.list_accounts(Operator::Server).await {
        Ok(accounts) => finish(write_accounts(&mut std::io::stdout(), &accounts)),
        Err(error) => {
            eprintln!("can't list the accounts: {error}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Turns the local account `login` names off, and prints what happened.
async fn deactivate(services: &service::Services, login: &str) -> ExitCode {
    let login = login.trim();
    let done = async {
        let user = services.find_account_by_login(login).await?;
        let done = services.deactivate_account(Operator::Server, user.id).await?;
        Ok((user, done))
    };
    match done.await {
        Ok((user, done)) => finish(write_deactivated(&mut std::io::stdout(), &user.username, done)),
        Err(error) => fail(&error, login, "turn the account off"),
    }
}

/// Turns the local account `login` names back on, and prints what happened.
async fn reactivate(services: &service::Services, login: &str) -> ExitCode {
    let login = login.trim();
    let done = async {
        let user = services.find_account_by_login(login).await?;
        let done = services.reactivate_account(Operator::Server, user.id).await?;
        Ok((user, done))
    };
    match done.await {
        Ok((user, done)) => finish(write_reactivated(&mut std::io::stdout(), &user.username, done)),
        Err(error) => fail(&error, login, "turn the account back on"),
    }
}

/// Makes `login` an operator, and prints what happened.
async fn make_operator(services: &service::Services, login: &str) -> ExitCode {
    let login = login.trim();
    match services.grant_operator(login).await {
        Ok((user, done)) => finish(write_operator_granted(&mut std::io::stdout(), &user.username, done)),
        Err(error) => fail(&error, login, "make an operator"),
    }
}

/// Takes the operator role from `login`, and prints what happened.
async fn remove_operator(services: &service::Services, login: &str) -> ExitCode {
    let login = login.trim();
    match services.remove_operator(login).await {
        Ok((user, done)) => finish(write_operator_removed(&mut std::io::stdout(), &user.username, done)),
        Err(error) => fail(&error, login, "remove the operator role"),
    }
}

/// The exit code for a line that was written, or couldn't be.
fn finish(written: std::io::Result<()>) -> ExitCode {
    match written {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("can't write the answer: {error}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Says why `login` couldn't be changed, and the exit code for it.
fn fail(error: &ServiceError, login: &str, what: &str) -> ExitCode {
    match error {
        ServiceError::NotFound => {
            eprintln!("No user is called {login:?}.");
            ExitCode::from(EXIT_NO_SUCH_USER)
        }
        ServiceError::AccountProviderManaged => {
            eprintln!("{login:?} signs in through the sign-in provider, which owns whether the account is on.");
            ExitCode::from(EXIT_PROVIDER_MANAGED)
        }
        other => {
            eprintln!("can't {what}: {other}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Makes and prints a reset link for `login`.
async fn reset_link(services: &service::Services, login: &str, base_url: &str) -> ExitCode {
    let login = login.trim();
    match services.create_password_reset_link(login).await {
        Ok(made) => match write_link(&mut std::io::stdout(), &mut std::io::stderr(), &made, base_url) {
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

/// Confirms the address of `login` and prints what happened.
async fn confirm(services: &service::Services, login: &str) -> ExitCode {
    let login = login.trim();
    match services.confirm_email_by_operator(login).await {
        Ok(outcome) => match write_confirmed(&mut std::io::stdout(), &outcome) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("can't write the answer: {error}");
                ExitCode::from(EXIT_UNEXPECTED)
            }
        },
        Err(ServiceError::NotFound) => {
            eprintln!("No user is called {login:?}.");
            ExitCode::from(EXIT_NO_SUCH_USER)
        }
        Err(error) => {
            eprintln!("can't confirm the address: {error}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Writes the header and a line for each account, separated by tabs, so the output pipes. A name or an address comes
/// from another person, so control characters are replaced.
fn write_accounts(stdout: &mut impl Write, accounts: &[Account]) -> std::io::Result<()> {
    writeln!(
        stdout,
        "username\tname\temail\tkind\tconfirmed\tstate\toperator\tlast sign-in"
    )?;
    for account in accounts {
        let last_sign_in = account.last_sign_in_at.map_or_else(
            || "never".to_owned(),
            |at| at.to_jiff().strftime("%Y-%m-%d %H:%M").to_string(),
        );
        writeln!(
            stdout,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{last_sign_in}",
            account.user.username,
            for_terminal(account.user.display_name.as_str()),
            for_terminal(account.user.email.as_str()),
            match account.kind {
                AccountKind::Local => "local",
                AccountKind::Provider => "provider",
            },
            if account.confirmed { "confirmed" } else { "unconfirmed" },
            if account.deactivated { "deactivated" } else { "active" },
            if account.operator { "operator" } else { "" },
        )?;
    }
    Ok(())
}

fn write_deactivated(
    stdout: &mut impl Write,
    username: &impl std::fmt::Display,
    done: Deactivation,
) -> std::io::Result<()> {
    match done {
        Deactivation::Ended(0) => writeln!(stdout, "Deactivated @{username}. No sign-ins were open."),
        Deactivation::Ended(1) => writeln!(stdout, "Deactivated @{username}. 1 sign-in ended."),
        Deactivation::Ended(n) => writeln!(stdout, "Deactivated @{username}. {n} sign-ins ended."),
        Deactivation::AlreadyOff => writeln!(stdout, "@{username} is already deactivated."),
    }
}

fn write_reactivated(stdout: &mut impl Write, username: &impl std::fmt::Display, done: Applied) -> std::io::Result<()> {
    match done {
        Applied::Changed => writeln!(stdout, "Reactivated @{username}."),
        Applied::Unchanged => writeln!(stdout, "@{username} isn't deactivated."),
    }
}

fn write_operator_granted(
    stdout: &mut impl Write,
    username: &impl std::fmt::Display,
    done: Applied,
) -> std::io::Result<()> {
    match done {
        Applied::Changed => writeln!(stdout, "@{username} is now an operator."),
        Applied::Unchanged => writeln!(stdout, "@{username} is already an operator."),
    }
}

fn write_operator_removed(
    stdout: &mut impl Write,
    username: &impl std::fmt::Display,
    done: Applied,
) -> std::io::Result<()> {
    match done {
        Applied::Changed => writeln!(stdout, "@{username} is no longer an operator."),
        Applied::Unchanged => writeln!(stdout, "@{username} isn't an operator."),
    }
}

/// Says on `stdout` whose address was confirmed, or already was.
fn write_confirmed(stdout: &mut impl Write, outcome: &OperatorConfirmed) -> std::io::Result<()> {
    match outcome {
        OperatorConfirmed::Confirmed(user) => {
            writeln!(
                stdout,
                "Confirmed @{}'s address, {}.",
                user.username,
                user.email.as_str()
            )
        }
        OperatorConfirmed::AlreadyConfirmed(user) => writeln!(
            stdout,
            "@{}'s address, {}, is already confirmed.",
            user.username,
            user.email.as_str()
        ),
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
    fn confirm_with_a_login_is_understood() {
        expect_that!(
            parse_args(&args(&["confirm", "sam"])),
            ok(eq(&UserCommand::Confirm { login: "sam".into() }))
        );
    }

    #[gtest]
    fn the_account_verbs_are_understood() {
        expect_that!(parse_args(&args(&["list"])), ok(eq(&UserCommand::List)));
        expect_that!(
            parse_args(&args(&["deactivate", "sam"])),
            ok(eq(&UserCommand::Deactivate { login: "sam".into() }))
        );
        expect_that!(
            parse_args(&args(&["reactivate", "sam"])),
            ok(eq(&UserCommand::Reactivate { login: "sam".into() }))
        );
        expect_that!(
            parse_args(&args(&["make-operator", "sam"])),
            ok(eq(&UserCommand::MakeOperator { login: "sam".into() }))
        );
        expect_that!(
            parse_args(&args(&["remove-operator", "sam"])),
            ok(eq(&UserCommand::RemoveOperator { login: "sam".into() }))
        );
    }

    #[gtest]
    fn everything_else_is_a_usage_error() {
        for words in [
            &[][..],
            &["reset-link"],
            &["reset-link", "sam", "extra"],
            &["confirm"],
            &["confirm", "sam", "extra"],
            &["list", "sam"],
            &["deactivate"],
            &["deactivate", "sam", "extra"],
            &["reactivate"],
            &["make-operator"],
            &["remove-operator", "sam", "extra"],
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

    /// A server with mail set up, where Sam registered and hasn't confirmed.
    async fn services_with_unconfirmed_sam() -> anyhow::Result<Services> {
        let db = store::connect("sqlite::memory:", 1).await?;
        store::run_migrations(&db).await?;
        let start = Timestamp::from_millis(Duration::from_hours(10).as_millis()).expect("in range");
        let settings = Settings {
            allow_registration: true,
            ..Settings::default()
        };
        let mailer = Arc::new(mail::Mailer::new("log:", "upheap@example.com")?);
        let services = Services::new(db, Arc::new(ManualClock::new(start)), settings)
            .with_mail(mailer, "https://upheap.example".to_owned());
        services
            .register(Registration {
                email: "sam@example.com",
                username: "sam",
                display_name: "Sam Ortiz",
                password: "correct horse battery",
            })
            .await?;
        Ok(services)
    }

    #[gtest]
    #[tokio::test]
    async fn an_unconfirmed_address_is_confirmed_once_and_the_second_time_changes_nothing() -> anyhow::Result<()> {
        // Arrange
        let services = services_with_unconfirmed_sam().await?;

        // Act
        let first = services.confirm_email_by_operator("sam").await?;
        let second = services.confirm_email_by_operator("sam@example.com").await?;
        let unknown = services.confirm_email_by_operator("nobody").await;

        // Assert
        let (mut said_first, mut said_second) = (Vec::new(), Vec::new());
        write_confirmed(&mut said_first, &first)?;
        write_confirmed(&mut said_second, &second)?;
        expect_that!(
            String::from_utf8(said_first)?,
            eq("Confirmed @sam's address, sam@example.com.\n")
        );
        expect_that!(
            String::from_utf8(said_second)?,
            eq("@sam's address, sam@example.com, is already confirmed.\n")
        );
        expect_that!(unknown, err(matches_pattern!(ServiceError::NotFound)));
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

    fn said(write: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>) -> anyhow::Result<String> {
        let mut out = Vec::new();
        write(&mut out)?;
        Ok(String::from_utf8(out)?)
    }

    #[gtest]
    #[tokio::test]
    async fn turning_an_account_off_and_on_says_what_happened() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = services_with_sam().await?;
        // Registering opened a session, and so does this.
        services.sign_in("sam", "correct horse battery").await?;
        let sam = services.find_account_by_login("sam").await?;

        // Act
        let off = services.deactivate_account(Operator::Server, sam.id).await?;
        let off_again = services.deactivate_account(Operator::Server, sam.id).await?;
        let on = services.reactivate_account(Operator::Server, sam.id).await?;
        let on_again = services.reactivate_account(Operator::Server, sam.id).await?;

        // Assert
        expect_that!(
            said(|out| write_deactivated(out, &sam.username, off))?,
            eq("Deactivated @sam. 2 sign-ins ended.\n")
        );
        expect_that!(
            said(|out| write_deactivated(out, &sam.username, off_again))?,
            eq("@sam is already deactivated.\n")
        );
        expect_that!(
            said(|out| write_reactivated(out, &sam.username, on))?,
            eq("Reactivated @sam.\n")
        );
        expect_that!(
            said(|out| write_reactivated(out, &sam.username, on_again))?,
            eq("@sam isn't deactivated.\n")
        );
        expect_that!(
            said(|out| write_deactivated(out, &sam.username, Deactivation::Ended(0)))?,
            eq("Deactivated @sam. No sign-ins were open.\n")
        );
        expect_that!(
            said(|out| write_deactivated(out, &sam.username, Deactivation::Ended(3)))?,
            eq("Deactivated @sam. 3 sign-ins ended.\n")
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn making_and_removing_an_operator_says_so_and_records_two_entries() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = services_with_sam().await?;
        let sam = services.find_account_by_login("sam").await?;
        let principal = service::Principal {
            user_id: sam.id,
            run: None,
        };
        let tz = jiff::tz::TimeZone::UTC;

        // Act
        let (_, made) = services.grant_operator("sam").await?;
        let (_, made_again) = services.grant_operator("sam@example.com").await?;
        let (_, removed) = services.remove_operator("sam").await?;
        let (_, removed_again) = services.remove_operator("sam").await?;
        let history = services.list_account_history(principal, None, 50, &tz).await?;

        // Assert
        expect_that!(
            said(|out| write_operator_granted(out, &sam.username, made))?,
            eq("@sam is now an operator.\n")
        );
        expect_that!(
            said(|out| write_operator_granted(out, &sam.username, made_again))?,
            eq("@sam is already an operator.\n")
        );
        expect_that!(
            said(|out| write_operator_removed(out, &sam.username, removed))?,
            eq("@sam is no longer an operator.\n")
        );
        expect_that!(
            said(|out| write_operator_removed(out, &sam.username, removed_again))?,
            eq("@sam isn't an operator.\n")
        );
        let descriptions: Vec<&str> = history.entries.iter().map(|entry| entry.description.as_str()).collect();
        expect_that!(
            descriptions,
            eq(&vec![
                "Operator role removed from the server.",
                "Made an operator from the server."
            ])
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn the_list_has_a_header_and_a_line_for_each_account() -> anyhow::Result<()> {
        // Arrange
        let (services, start) = services_with_sam().await?;
        services.grant_operator("sam").await?;
        services.sign_in("sam", "correct horse battery").await?;

        // Act
        let accounts = services.list_accounts(Operator::Server).await?;
        let out = said(|out| write_accounts(out, &accounts))?;

        // Assert
        let signed_in = start.to_jiff().strftime("%Y-%m-%d %H:%M").to_string();
        expect_that!(
            out,
            eq(&format!(
                "username\tname\temail\tkind\tconfirmed\tstate\toperator\tlast sign-in\nsam\tSam Ortiz\tsam@example.com\tlocal\tconfirmed\tactive\toperator\t{signed_in}\n"
            ))
        );
        Ok(())
    }
}
