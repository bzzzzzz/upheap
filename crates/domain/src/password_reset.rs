//! A password reset link: a one-time secret URL token the server's operator makes for one user, which lets whoever
//! holds it set a new password for an hour. Only the token's hash is stored, so the link is shown once, when made.
//!
//! The state is derived from the timestamps, never stored, and `Used`, `Cancelled` and `Ended` are final.

use std::fmt;

use crate::{Duration, Timestamp, UserId};

/// How long a link works after it was made.
pub const RESET_LINK_LIFETIME: Duration = Duration::from_minutes(60);

/// The length of a token: 256 random bits as unpadded base64url. `service::auth::random_token` makes them and the
/// store's `password_reset_links.token_hash` is the SHA-256 of one; a change here needs `random_token` to agree.
pub const RESET_TOKEN_LENGTH: usize = 43;

/// Why a link can't be used to set a password. The messages name no IDs: they reach whoever holds the link, signed in
/// or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResetLinkError {
    #[error("this reset link was already used")]
    Used,
    #[error("this reset link no longer works: a newer one was made, or the password was changed")]
    Cancelled,
    #[error("this reset link has ended")]
    Ended,
}

/// The secret in a reset link. Never log it.
#[derive(Clone, PartialEq, Eq)]
pub struct ResetToken(String);

impl ResetToken {
    /// A token just made by the service.
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// A token read from outside (a link's path): exactly [`RESET_TOKEN_LENGTH`] base64url characters, which is what
    /// 256 random bits as unpadded base64url come to. `None` for anything else, since it can't name a link, and the
    /// caller then needn't read the database.
    pub fn parse(raw: &str) -> Option<Self> {
        let valid =
            raw.len() == RESET_TOKEN_LENGTH && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        valid.then(|| Self(raw.to_owned()))
    }

    /// The secret itself, for the link the operator is given and for hashing it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ResetToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ResetToken(<redacted>)")
    }
}

/// Where a link is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetLinkState {
    Working,
    /// A password was set with it.
    Used,
    /// A newer link was made for the user, or the user changed their password while it was working.
    Cancelled,
    /// Its hour passed.
    Ended,
}

impl ResetLinkState {
    /// The error for a link that doesn't work, or `None` for a working one.
    pub const fn error(self) -> Option<ResetLinkError> {
        match self {
            Self::Working => None,
            Self::Used => Some(ResetLinkError::Used),
            Self::Cancelled => Some(ResetLinkError::Cancelled),
            Self::Ended => Some(ResetLinkError::Ended),
        }
    }
}

/// A stored reset link, without its token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetLink {
    pub user_id: UserId,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub used_at: Option<Timestamp>,
    pub cancelled_at: Option<Timestamp>,
}

impl ResetLink {
    /// When a link made at `now` stops working by itself. `None` if that is beyond the supported range.
    pub fn end_from(now: Timestamp) -> Option<Timestamp> {
        now.checked_add(RESET_LINK_LIFETIME)
    }

    /// The state at `now`, checked in this order: used, cancelled, ended, else working. So a link used and then past
    /// its hour stays used, and one cancelled and then past its hour stays cancelled.
    pub fn state(&self, now: Timestamp) -> ResetLinkState {
        if self.used_at.is_some() {
            ResetLinkState::Used
        } else if self.cancelled_at.is_some() {
            ResetLinkState::Cancelled
        } else if now >= self.expires_at {
            ResetLinkState::Ended
        } else {
            ResetLinkState::Working
        }
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn at(offset: Duration) -> Timestamp {
        Timestamp::EPOCH.checked_add(offset).expect("in range")
    }

    fn link() -> ResetLink {
        ResetLink {
            user_id: UserId::from_uuid(Uuid::from_u128(1)),
            created_at: Timestamp::EPOCH,
            expires_at: at(RESET_LINK_LIFETIME),
            used_at: None,
            cancelled_at: None,
        }
    }

    #[gtest]
    fn a_link_works_until_its_hour_is_over() {
        let hour = RESET_LINK_LIFETIME;
        expect_that!(link().state(Timestamp::EPOCH), eq(ResetLinkState::Working));
        expect_that!(
            link().state(at(Duration::from_millis(hour.as_millis() - 1))),
            eq(ResetLinkState::Working)
        );
        expect_that!(link().state(at(hour)), eq(ResetLinkState::Ended));
        expect_that!(ResetLink::end_from(Timestamp::EPOCH), some(eq(at(hour))));
    }

    #[gtest]
    fn a_used_link_is_used() {
        let used = ResetLink {
            used_at: Some(Timestamp::EPOCH),
            ..link()
        };
        expect_that!(used.state(Timestamp::EPOCH), eq(ResetLinkState::Used));
    }

    #[gtest]
    fn a_cancelled_link_is_cancelled() {
        let cancelled = ResetLink {
            cancelled_at: Some(Timestamp::EPOCH),
            ..link()
        };
        expect_that!(cancelled.state(Timestamp::EPOCH), eq(ResetLinkState::Cancelled));
    }

    #[gtest]
    fn used_and_cancelled_stay_what_they_are_after_the_hour() {
        let late = at(Duration::from_days(1));
        let used = ResetLink {
            used_at: Some(Timestamp::EPOCH),
            ..link()
        };
        let cancelled = ResetLink {
            cancelled_at: Some(Timestamp::EPOCH),
            ..link()
        };
        expect_that!(used.state(late), eq(ResetLinkState::Used));
        expect_that!(cancelled.state(late), eq(ResetLinkState::Cancelled));
    }

    #[gtest]
    fn used_beats_cancelled() {
        let both = ResetLink {
            used_at: Some(Timestamp::EPOCH),
            cancelled_at: Some(Timestamp::EPOCH),
            ..link()
        };
        expect_that!(both.state(Timestamp::EPOCH), eq(ResetLinkState::Used));
    }

    #[gtest]
    fn a_state_that_does_not_work_has_its_error() {
        expect_that!(ResetLinkState::Working.error(), none());
        expect_that!(ResetLinkState::Used.error(), some(eq(ResetLinkError::Used)));
        expect_that!(ResetLinkState::Cancelled.error(), some(eq(ResetLinkError::Cancelled)));
        expect_that!(ResetLinkState::Ended.error(), some(eq(ResetLinkError::Ended)));
    }

    #[gtest]
    fn a_token_is_43_base64url_characters() {
        let token = "a".repeat(RESET_TOKEN_LENGTH - 2) + "-_";
        expect_that!(
            ResetToken::parse(&token).map(|t| t.expose().to_owned()),
            some(eq(&token))
        );
        expect_that!(ResetToken::parse(&token[1..]), none());
        expect_that!(ResetToken::parse(&format!("{token}a")), none());
        expect_that!(ResetToken::parse(&format!("{} ", &token[1..])), none());
        expect_that!(ResetToken::parse(&format!("{}+", &token[1..])), none());
        expect_that!(ResetToken::parse(&format!("{}/", &token[1..])), none());
        expect_that!(ResetToken::parse(&format!("{}=", &token[1..])), none());
        expect_that!(ResetToken::parse(&format!("{}é", &token[2..])), none());
        expect_that!(ResetToken::parse(""), none());
    }

    #[gtest]
    fn the_token_does_not_show_in_debug_output() {
        expect_that!(
            format!("{:?}", ResetToken::new("secret".to_owned())),
            not(contains_substring("secret"))
        );
    }
}
