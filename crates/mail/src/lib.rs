//! Sends plain-text mail: over SMTP, or for development and CI to files or the log. Built once at startup from the
//! server's `MAIL_URL` and `MAIL_FROM` settings; see architecture.md, Configuration and deployment.

use std::path::{Path, PathBuf};

use lettre::message::Mailbox;
use lettre::message::header::ContentType;
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::{AsyncFileTransport, AsyncTransport, Message, Tokio1Executor};
use url::Url;

/// One plain-text mail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mail {
    pub to: String,
    pub subject: String,
    pub body: String,
}

/// Why a mailer couldn't be built or a mail couldn't be sent. None of them holds the SMTP password.
#[derive(Debug, thiserror::Error)]
pub enum MailError {
    /// The mail URL isn't one this crate can use; the reason never repeats the URL.
    #[error("{0}")]
    InvalidUrl(String),
    #[error("the sender isn't an email address")]
    InvalidSender(#[source] lettre::address::AddressError),
    #[error("the recipient isn't an email address")]
    InvalidRecipient(#[source] lettre::address::AddressError),
    #[error("the mail wasn't sent")]
    Send(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Where a mailer's mail goes.
#[derive(Debug)]
enum Transport {
    Smtp(AsyncSmtpTransport<Tokio1Executor>),
    File(AsyncFileTransport<Tokio1Executor>),
    Log,
}

/// Sends mail from one configured sender.
#[derive(Debug)]
pub struct Mailer {
    from: Mailbox,
    transport: Transport,
    description: String,
}

impl Mailer {
    /// Builds a mailer from `MAIL_URL` (`smtps://`, `smtp://`, `file://` or `log:`) and `MAIL_FROM`. Does no network
    /// IO. An `smtp://` URL without a `tls=` parameter gets `tls=required`, so it never sends in cleartext.
    pub fn new(url: &str, from: &str) -> Result<Self, MailError> {
        let from: Mailbox = from.parse().map_err(MailError::InvalidSender)?;
        let (transport, description) = if url == "log:" {
            (Transport::Log, url.to_owned())
        } else if let Some(dir) = url.strip_prefix("file://") {
            if !Path::new(dir).is_dir() {
                return Err(MailError::InvalidUrl(format!("directory {dir} doesn't exist")));
            }
            (
                Transport::File(AsyncFileTransport::new(PathBuf::from(dir))),
                url.to_owned(),
            )
        } else if url.starts_with("smtps://") || url.starts_with("smtp://") {
            let mut parsed = Url::parse(url).map_err(|error| MailError::InvalidUrl(error.to_string()))?;
            if parsed.scheme() == "smtp" && !parsed.query_pairs().any(|(key, _)| key == "tls") {
                parsed.query_pairs_mut().append_pair("tls", "required");
            }
            let builder = AsyncSmtpTransport::<Tokio1Executor>::from_url(parsed.as_str())
                .map_err(|error| MailError::InvalidUrl(error.to_string()))?;
            // The user goes too: some providers take the API token as the user name. Removing them fails only for a
            // URL that can't hold them, which from_url has already refused.
            let _ = parsed.set_password(None);
            let _ = parsed.set_username("");
            (Transport::Smtp(builder.build()), parsed.to_string())
        } else {
            return Err(MailError::InvalidUrl(
                "must start with smtps://, smtp://, file:// or log:".to_owned(),
            ));
        };
        Ok(Self {
            from,
            transport,
            description,
        })
    }

    /// Sends `mail` from the configured sender. Fails when `mail.to` isn't an address or the transport fails.
    pub async fn send(&self, mail: Mail) -> Result<(), MailError> {
        let to: Mailbox = mail.to.parse().map_err(MailError::InvalidRecipient)?;
        // From and To are set, and those are all the builder requires.
        #[allow(clippy::expect_used)]
        let message = Message::builder()
            .from(self.from.clone())
            .to(to)
            .subject(mail.subject.as_str())
            .header(ContentType::TEXT_PLAIN)
            .body(mail.body.clone())
            .expect("a message with a sender and a recipient builds");
        match &self.transport {
            Transport::Smtp(transport) => transport.send(message).await.map(drop).map_err(boxed),
            Transport::File(transport) => transport.send(message).await.map(drop).map_err(boxed),
            Transport::Log => {
                tracing::info!(to = %mail.to, subject = %mail.subject, body = %mail.body, "mail written to the log");
                Ok(())
            }
        }
    }

    /// Where mail goes: `MAIL_URL` without the user and password. For logs and the test command.
    pub fn describe(&self) -> &str {
        &self.description
    }

    /// The sender.
    pub fn from(&self) -> &Mailbox {
        &self.from
    }
}

fn boxed(error: impl std::error::Error + Send + Sync + 'static) -> MailError {
    MailError::Send(Box::new(error))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    const FROM: &str = "Upheap <upheap@example.com>";

    #[gtest]
    fn smtp_urls_show_their_tls_and_never_the_credentials() -> Result<()> {
        // Arrange
        let urls = [
            "smtps://user:s3cret@smtp.example.com",
            "smtp://user:s3cret@smtp.example.com:2525",
            "smtp://user:s3cret@smtp.example.com?tls=opportunistic",
        ];

        // Act
        let described: Vec<String> = urls
            .iter()
            .map(|url| Mailer::new(url, FROM).map(|mailer| mailer.describe().to_owned()))
            .collect::<std::result::Result<_, _>>()
            .or_fail()?;

        // Assert
        expect_that!(
            described,
            elements_are![
                eq("smtps://smtp.example.com"),
                eq("smtp://smtp.example.com:2525?tls=required"),
                eq("smtp://smtp.example.com?tls=opportunistic"),
            ]
        );
        expect_that!(described.concat(), not(contains_substring("s3cret")));
        Ok(())
    }

    #[gtest]
    fn the_other_urls_are_described_as_given() -> Result<()> {
        // Arrange
        let dir = tempfile::tempdir().or_fail()?;
        let url = format!("file://{}", dir.path().display());

        // Act
        let (file, log) = (Mailer::new(&url, FROM).or_fail()?, Mailer::new("log:", FROM).or_fail()?);

        // Assert
        expect_that!(file.describe(), eq(&url));
        expect_that!(log.describe(), eq("log:"));
        Ok(())
    }

    #[gtest]
    fn bad_settings_are_named() {
        expect_that!(
            Mailer::new("http://x", FROM),
            err(matches_pattern!(MailError::InvalidUrl(contains_substring(
                "must start with"
            ))))
        );
        expect_that!(
            Mailer::new("smtps://", FROM),
            err(matches_pattern!(MailError::InvalidUrl(_)))
        );
        expect_that!(
            Mailer::new("file:///no/such/upheap/dir", FROM),
            err(matches_pattern!(MailError::InvalidUrl(contains_substring(
                "doesn't exist"
            ))))
        );
        expect_that!(
            Mailer::new("log:", "not an address"),
            err(matches_pattern!(MailError::InvalidSender(_)))
        );
    }

    #[gtest]
    #[tokio::test]
    async fn a_file_mailer_writes_one_plain_text_eml() -> Result<()> {
        // Arrange
        let dir = tempfile::tempdir().or_fail()?;
        let mailer = Mailer::new(&format!("file://{}", dir.path().display()), FROM).or_fail()?;
        let mail = Mail {
            to: "ana@example.com".into(),
            subject: "Hello Ana".into(),
            body: "A line of text.".into(),
        };

        // Act
        mailer.send(mail).await.or_fail()?;

        // Assert
        let files: Vec<_> = std::fs::read_dir(dir.path())
            .or_fail()?
            .collect::<std::result::Result<_, _>>()
            .or_fail()?;
        verify_that!(files, len(eq(1)))?;
        let text = std::fs::read_to_string(files[0].path()).or_fail()?;
        expect_that!(text, contains_substring("To: ana@example.com"));
        expect_that!(text, contains_substring("From: Upheap <upheap@example.com>"));
        expect_that!(text, contains_substring("Subject: Hello Ana"));
        expect_that!(text, contains_substring("text/plain"));
        expect_that!(text, contains_substring("A line of text."));
        Ok(())
    }

    #[gtest]
    #[tokio::test]
    async fn a_recipient_that_isnt_an_address_writes_nothing() -> Result<()> {
        // Arrange
        let dir = tempfile::tempdir().or_fail()?;
        let mailer = Mailer::new(&format!("file://{}", dir.path().display()), FROM).or_fail()?;
        let mail = Mail {
            to: "nobody".into(),
            subject: "Hi".into(),
            body: "Hi".into(),
        };

        // Act
        let result = mailer.send(mail).await;

        // Assert
        expect_that!(result, err(matches_pattern!(MailError::InvalidRecipient(_))));
        expect_that!(std::fs::read_dir(dir.path()).or_fail()?.count(), eq(0));
        Ok(())
    }
}
