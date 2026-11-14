//! `upheap-server mail test <address>`: sends one test mail through the configured mailer, to check that mail is set
//! up. It reads the server's environment (so `DATABASE_URL` must be set) but doesn't connect to the database. Results
//! go to stdout and stderr; logs go to stderr.

use std::io::Write;
use std::process::ExitCode;

use mail::{Mail, Mailer};
use tracing_subscriber::fmt::writer::BoxMakeWriter;

use crate::config::Config;

const USAGE: &str = "usage: upheap-server mail test <address>";

/// Exit codes, as documented in the README (Setting up mail). `0` is a mail sent.
const EXIT_UNEXPECTED: u8 = 1;
const EXIT_USAGE: u8 = 2;
const EXIT_MAIL: u8 = 6;

/// Reads the arguments after `mail`: `test` and one address. Anything else is a usage error.
fn parse_args(args: &[String]) -> Result<&str, &'static str> {
    match args {
        [verb, address] if verb == "test" => Ok(address),
        _ => Err(USAGE),
    }
}

/// Runs the command on the arguments after `mail`, and returns the process's exit code.
pub async fn run(args: &[String]) -> ExitCode {
    let address = match parse_args(args) {
        Ok(address) => address,
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
    let Some(mailer) = &config.mail else {
        eprintln!("Mail isn't set up: set MAIL_URL and MAIL_FROM.");
        return ExitCode::from(EXIT_MAIL);
    };
    match send_test(
        mailer,
        address,
        &config.base_url,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
    )
    .await
    {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("can't write the result: {error}");
            ExitCode::from(EXIT_UNEXPECTED)
        }
    }
}

/// Sends the test mail to `address`, says on `stdout` or `stderr` how it went, and returns the exit code.
async fn send_test(
    mailer: &Mailer,
    address: &str,
    base_url: &str,
    stdout: &mut impl Write,
    stderr: &mut impl Write,
) -> std::io::Result<u8> {
    let mail = Mail {
        to: address.to_owned(),
        subject: "Test mail from Upheap".to_owned(),
        body: format!(
            "This is a test mail from the Upheap server at {base_url}.\n\nMail is set up and works. You can ignore this mail.\n"
        ),
    };
    match mailer.send(mail).await {
        Ok(()) => {
            writeln!(
                stdout,
                "Sent a test mail to {address} from {} via {}.",
                mailer.from(),
                mailer.describe()
            )?;
            Ok(0)
        }
        Err(mail::MailError::InvalidRecipient(_)) => {
            writeln!(stderr, "{address} isn't an email address.")?;
            Ok(EXIT_USAGE)
        }
        Err(error) => {
            writeln!(
                stderr,
                "Can't send the test mail via {}: {}",
                mailer.describe(),
                error_chain(&error)
            )?;
            Ok(EXIT_MAIL)
        }
    }
}

/// An error and what caused it, joined with `: `.
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    #[gtest]
    fn test_with_an_address_is_understood() {
        expect_that!(parse_args(&args(&["test", "a@x"])), ok(eq("a@x")));
    }

    #[gtest]
    fn everything_else_is_a_usage_error() {
        for words in [&[][..], &["test"], &["test", "a", "b"], &["frob", "a"]] {
            expect_that!(parse_args(&args(words)), err(eq(USAGE)), "{words:?}");
        }
    }

    #[gtest]
    #[tokio::test]
    async fn a_test_mail_is_written_and_reported() -> anyhow::Result<()> {
        // Arrange
        let dir = tempfile::tempdir()?;
        let mailer = Mailer::new(&format!("file://{}", dir.path().display()), "Upheap <u@example.com>")?;
        let (mut stdout, mut stderr) = (Vec::new(), Vec::new());

        // Act
        let code = send_test(
            &mailer,
            "ana@example.com",
            "https://upheap.example",
            &mut stdout,
            &mut stderr,
        )
        .await;
        let bad = send_test(
            &mailer,
            "nobody",
            "https://upheap.example",
            &mut Vec::new(),
            &mut Vec::new(),
        )
        .await;

        // Assert
        expect_that!(code, ok(eq(&0)));
        expect_that!(
            String::from_utf8(stdout)?,
            starts_with("Sent a test mail to ana@example.com from Upheap <u@example.com> via file://")
        );
        expect_that!(stderr, is_empty());
        let files = std::fs::read_dir(dir.path())?.collect::<std::result::Result<Vec<_>, _>>()?;
        assert_that!(files, len(eq(1)));
        let text = std::fs::read_to_string(files[0].path())?;
        expect_that!(text, contains_substring("Subject: Test mail from Upheap"));
        expect_that!(text, contains_substring("the Upheap server at https://upheap.example."));
        expect_that!(bad, ok(eq(&EXIT_USAGE)));
        Ok(())
    }
}
