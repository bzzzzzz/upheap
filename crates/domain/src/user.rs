//! Users and their credentials rules.

use crate::text::parse_required;
use crate::validation::{Field, ValidationError};
use crate::{Timestamp, UserId, Version};

/// A normalised email address: trimmed and lowercased, with one `@` and something on both sides.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Email(String);

impl Email {
    /// RFC 5321 limits a forward path to 254 characters.
    pub const MAX_CHARS: usize = 254;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let normalised = raw.trim().to_lowercase();
        let Some((local, domain)) = normalised.split_once('@') else {
            return Err(ValidationError::EmailInvalid);
        };
        let well_formed = !local.is_empty()
            && !domain.is_empty()
            && !domain.contains('@')
            && !normalised.chars().any(char::is_whitespace)
            && normalised.chars().count() <= Self::MAX_CHARS;
        if !well_formed {
            return Err(ValidationError::EmailInvalid);
        }
        Ok(Self(normalised))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The name shown for a user: trimmed, 1 to 100 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    pub const MAX_CHARS: usize = 100;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::DisplayName, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub email: Email,
    pub display_name: DisplayName,
    pub created_at: Timestamp,
    pub version: Version,
}

/// The shortest text a user lookup matches on, so a lookup can't list every user one letter at a time.
pub const USER_QUERY_MIN_CHARS: usize = 3;

/// A user lookup: trimmed, ASCII letters lowercased, at least [`USER_QUERY_MIN_CHARS`] characters.
///
/// Only ASCII is lowercased because SQLite's `LOWER` changes nothing else: a fully lowercased `öla` could never match
/// a stored `Ölaf` there, while `Öla` does. MySQL compares case-insensitively anyway.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserQuery(String);

impl UserQuery {
    pub const MAX_CHARS: usize = Email::MAX_CHARS;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let text = parse_required(raw, Field::Query, Self::MAX_CHARS)?.to_ascii_lowercase();
        if text.chars().count() < USER_QUERY_MIN_CHARS {
            return Err(ValidationError::TooShort {
                field: Field::Query,
                min: USER_QUERY_MIN_CHARS,
            });
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The query as the start of an email address, when it has the whole local part, the `@` and at least one
    /// character of the domain. Emails match only then, so a lookup can't collect addresses by guessing a few
    /// letters (`com`, `@gm`).
    pub fn as_email_prefix(&self) -> Option<&str> {
        let (local, domain) = self.0.split_once('@')?;
        (!local.is_empty() && !domain.is_empty()).then_some(self.0.as_str())
    }
}

pub const PASSWORD_MIN_CHARS: usize = 10;
/// Bounds the argon2 work an attacker can cause with one request.
pub const PASSWORD_MAX_CHARS: usize = 1024;

/// Checks a new password against the length rules. The password itself is never stored or logged.
pub fn validate_new_password(password: &str) -> Result<(), ValidationError> {
    let chars = password.chars().count();
    if chars < PASSWORD_MIN_CHARS {
        return Err(ValidationError::PasswordTooShort {
            min: PASSWORD_MIN_CHARS,
        });
    }
    if chars > PASSWORD_MAX_CHARS {
        return Err(ValidationError::PasswordTooLong {
            max: PASSWORD_MAX_CHARS,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn email_is_normalised() {
        let email = Email::parse("  Ada@Example.COM ").expect("valid");
        expect_that!(email.as_str(), eq("ada@example.com"));
    }

    #[gtest]
    fn email_without_at_is_rejected() {
        expect_that!(Email::parse("ada.example.com"), err(eq(&ValidationError::EmailInvalid)));
        expect_that!(Email::parse("a@b@c"), err(eq(&ValidationError::EmailInvalid)));
        expect_that!(Email::parse("@b"), err(eq(&ValidationError::EmailInvalid)));
    }

    #[gtest]
    fn user_query_needs_three_characters() {
        expect_that!(
            UserQuery::parse(" ad "),
            err(eq(&ValidationError::TooShort {
                field: Field::Query,
                min: USER_QUERY_MIN_CHARS
            }))
        );
        expect_that!(UserQuery::parse(" Ada ").map(|q| q.as_str().to_owned()), ok(eq("ada")));
        expect_that!(UserQuery::parse("Öla").map(|q| q.as_str().to_owned()), ok(eq("Öla")));
    }

    #[gtest]
    fn only_a_local_part_and_some_domain_match_emails() {
        let prefix = |raw: &str| {
            UserQuery::parse(raw)
                .ok()
                .and_then(|q| q.as_email_prefix().map(str::to_owned))
        };
        expect_that!(prefix("ada@e"), some(eq("ada@e")));
        expect_that!(prefix("Ada@Example.test"), some(eq("ada@example.test")));
        expect_that!(prefix("@gmail"), none());
        expect_that!(prefix("ada@"), none());
        expect_that!(prefix("com"), none());
    }

    #[gtest]
    fn short_password_is_rejected() {
        expect_that!(
            validate_new_password("short"),
            err(eq(&ValidationError::PasswordTooShort {
                min: PASSWORD_MIN_CHARS
            }))
        );
        expect_that!(validate_new_password("long enough password"), ok(eq(&())));
    }
}
