//! `upheap-server team make-admin <team slug> <username>`: makes a member of a team an admin, for a team whose last
//! admin left it, which a sync of provider groups can do and nobody in the team can then fix. It runs beside a server
//! that has already migrated the database, so it doesn't migrate (it stops when the database is behind this version)
//! and doesn't listen. It never signs anyone in, so it doesn't read which teams the provider's groups manage. What it
//! reports goes to stdout; logs go to stderr.

use std::io::Write;
use std::process::ExitCode;

use service::Services;
use service::teams::MakeAdminError;
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::config::Config;
use crate::terminal::for_terminal;

const USAGE: &str = "usage: upheap-server team make-admin <team slug> <username>";

/// Exit codes, as documented in accounts.md (Making a team admin). `0` is an admin made, or one already.
const EXIT_UNEXPECTED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_NOT_FOUND: u8 = 3;
const EXIT_DATABASE_UNREACHABLE: u8 = 5;

/// What the arguments after `team` ask for.
#[derive(Debug, PartialEq, Eq)]
enum TeamCommand {
    MakeAdmin { slug: String, username: String },
}

/// Reads the arguments after `team`: `make-admin`, a team slug and a username. Anything else is a usage error.
fn parse_args(args: &[String]) -> Result<TeamCommand, &'static str> {
    match args {
        [verb, slug, username] if verb == "make-admin" => Ok(TeamCommand::MakeAdmin {
            slug: slug.clone(),
            username: username.clone(),
        }),
        _ => Err(USAGE),
    }
}

/// Runs the command on the arguments after `team`, and returns the process's exit code.
pub async fn run(args: &[String]) -> ExitCode {
    let TeamCommand::MakeAdmin { slug, username } = match parse_args(args) {
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
    let services = crate::build_services(&config, service::group_teams::GroupTeams::default(), db);
    let code = make_admin(
        &services,
        slug.trim(),
        username.trim(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .await;
    ExitCode::from(code)
}

/// Makes `username` an admin of the team with `slug`, says what happened on `stdout` or why not on `stderr`, and
/// returns the exit code.
async fn make_admin(
    services: &Services,
    slug: &str,
    username: &str,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> u8 {
    let told = match services.make_admin_by_operator(slug, username).await {
        Ok(made) => {
            let state = if made.already_admin { "already" } else { "now" };
            writeln!(
                stdout,
                "@{} is {state} an admin of {}.",
                made.user.username,
                for_terminal(made.team.name.as_str()),
            )
            .map(|()| 0)
        }
        Err(MakeAdminError::NoSuchTeam) => writeln!(stderr, "No team has the slug {slug:?}.").map(|()| EXIT_NOT_FOUND),
        Err(MakeAdminError::NoSuchUser) => writeln!(stderr, "No user is called {username:?}.").map(|()| EXIT_NOT_FOUND),
        Err(MakeAdminError::NotAMember) => {
            writeln!(stderr, "@{username} is not a member of the team {slug:?}.").map(|()| EXIT_NOT_FOUND)
        }
        Err(MakeAdminError::Service(error)) => {
            writeln!(stderr, "can't make an admin: {error}").map(|()| EXIT_UNEXPECTED)
        }
    };
    told.unwrap_or(EXIT_UNEXPECTED)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use domain::audit::AuditAction;
    use domain::handle::Handle;
    use domain::team::Role;
    use domain::text::Name;
    use domain::validation::Field;
    use domain::{Duration, Timestamp};
    use googletest::prelude::*;
    use service::auth::Registration;
    use service::email_confirmation::Registered;
    use service::{ManualClock, Principal, Services, Settings};
    use store::audit::{AuditQuery, AuditScope};

    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[gtest]
    fn make_admin_with_a_team_and_a_username_is_understood() {
        expect_that!(
            parse_args(&args(&["make-admin", "ops", "sam"])),
            ok(eq(&TeamCommand::MakeAdmin {
                slug: "ops".into(),
                username: "sam".into()
            }))
        );
    }

    #[gtest]
    fn everything_else_is_a_usage_error() {
        for words in [
            &[][..],
            &["make-admin"],
            &["make-admin", "ops"],
            &["make-admin", "ops", "sam", "extra"],
            &["frobnicate", "ops", "sam"],
            &["ops", "sam"],
        ] {
            expect_that!(parse_args(&args(words)), err(eq(&USAGE)), "{words:?}");
        }
    }

    /// A server with the team `ops`, run by `sam`, with `alex` a member and `blair` outside it.
    async fn ops_with_sam_and_alex() -> anyhow::Result<(Services, store::DatabaseConnection)> {
        let db = store::connect("sqlite::memory:", 1).await?;
        store::run_migrations(&db).await?;
        let start = Timestamp::from_millis(Duration::from_hours(10).as_millis()).expect("in range");
        let settings = Settings {
            allow_registration: true,
            ..Settings::default()
        };
        let services = Services::new(db.clone(), Arc::new(ManualClock::new(start)), settings);
        let mut ids = Vec::new();
        for name in ["sam", "alex", "blair"] {
            let Registered::SignedIn(grant) = services
                .register(Registration {
                    email: &format!("{name}@example.com"),
                    username: name,
                    display_name: name,
                    password: "correct horse battery",
                })
                .await?
            else {
                panic!("a server without mail signs the new account in");
            };
            ids.push(grant.user.id);
        }
        let sam = Principal {
            user_id: ids[0],
            run: None,
        };
        let team = services
            .create_team(sam, Name::parse("Ops")?, Some(Handle::parse("ops", Field::Slug)?))
            .await?;
        services.add_member(sam, team.team.id, ids[1]).await?;
        Ok((services, db))
    }

    async fn run_make_admin(services: &Services, slug: &str, username: &str) -> (u8, String, String) {
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());
        let code = make_admin(services, slug, username, &mut stdout, &mut stderr).await;
        (
            code,
            String::from_utf8(stdout).expect("utf-8"),
            String::from_utf8(stderr).expect("utf-8"),
        )
    }

    #[gtest]
    #[tokio::test]
    async fn a_member_becomes_an_admin_once_and_the_audit_log_says_the_operator_did_it() -> anyhow::Result<()> {
        // Arrange
        let (services, db) = ops_with_sam_and_alex().await?;

        // Act
        let first = run_make_admin(&services, "ops", "alex").await;
        let second = run_make_admin(&services, "ops", "alex").await;

        // Assert
        expect_that!(
            first,
            eq(&(0, "@alex is now an admin of Ops.\n".to_owned(), String::new()))
        );
        expect_that!(
            second,
            eq(&(0, "@alex is already an admin of Ops.\n".to_owned(), String::new()))
        );
        let team = store::teams::find_team_by_slug(&db, &Handle::parse("ops", Field::Slug)?)
            .await?
            .expect("the team");
        let members = store::teams::list_memberships(&db, team.id).await?;
        expect_that!(members.iter().filter(|m| m.role == Role::Admin).count(), eq(2));
        let entries = store::audit::list_audit_entries(
            &db,
            AuditQuery {
                scope: AuditScope::Team(team.id),
                entity_id: None,
                actions: &[AuditAction::MemberRoleChanged],
                after: None,
                limit: 10,
            },
        )
        .await?;
        expect_that!(entries, len(eq(1)));
        expect_that!(
            entries[0].diff,
            eq(&serde_json::json!({ "role": { "before": "member", "after": "admin" }, "made_by": "operator" }))
        );
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn a_team_user_or_member_that_isnt_there_exits_3_and_says_which() -> anyhow::Result<()> {
        // Arrange
        let (services, _) = ops_with_sam_and_alex().await?;

        // Act
        let no_team = run_make_admin(&services, "nope", "alex").await;
        let no_user = run_make_admin(&services, "ops", "nobody").await;
        let not_a_member = run_make_admin(&services, "ops", "blair").await;

        // Assert
        expect_that!(
            no_team,
            eq(&(3, String::new(), "No team has the slug \"nope\".\n".to_owned()))
        );
        expect_that!(
            no_user,
            eq(&(3, String::new(), "No user is called \"nobody\".\n".to_owned()))
        );
        expect_that!(
            not_a_member,
            eq(&(
                3,
                String::new(),
                "@blair is not a member of the team \"ops\".\n".to_owned()
            ))
        );
        Ok(())
    }
}
