//! An email change link: a one-time secret URL token mailed to the new address a user asked to use, which changes
//! their address when it is applied, for a day. Only the token's hash is stored, so the link is shown only in the
//! mail. The token is a [`ResetToken`](crate::password_reset::ResetToken), and the state is a
//! [`ResetLinkState`], as for a password reset link.
//!
//! The state is derived from the timestamps, never stored, and `Used`, `Cancelled` and `Ended` are final.

use crate::password_reset::ResetLinkState;
use crate::user::Email;
use crate::{Duration, Timestamp, UserId};

/// How long a link works after it was asked for.
pub const EMAIL_CHANGE_LINK_LIFETIME: Duration = Duration::from_hours(24);

/// How long an email confirmation link works after it was mailed.
pub const EMAIL_CONFIRMATION_LINK_LIFETIME: Duration = Duration::from_hours(24);

/// When a confirmation link mailed at `now` stops working by itself. `None` if that is beyond the supported range.
pub fn confirmation_link_end_from(now: Timestamp) -> Option<Timestamp> {
    now.checked_add(EMAIL_CONFIRMATION_LINK_LIFETIME)
}

/// Why a link can't be used to change an address. The messages name no IDs: they reach whoever holds the link.
/// Email confirmation links use it too, for the same three reasons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EmailChangeLinkError {
    #[error("This link was already used.")]
    Used,
    #[error("This link no longer works: a newer one was asked for.")]
    Cancelled,
    #[error("This link has ended.")]
    Ended,
}

impl ResetLinkState {
    /// The error for an email change link that doesn't work, or `None` for a working one.
    pub const fn email_change_error(self) -> Option<EmailChangeLinkError> {
        match self {
            Self::Working => None,
            Self::Used => Some(EmailChangeLinkError::Used),
            Self::Cancelled => Some(EmailChangeLinkError::Cancelled),
            Self::Ended => Some(EmailChangeLinkError::Ended),
        }
    }
}

/// A stored email change link, without its token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmailChangeLink {
    pub user_id: UserId,
    /// The address the link changes the user's to, and the one it was mailed to.
    pub new_email: Email,
    pub created_at: Timestamp,
    pub expires_at: Timestamp,
    pub used_at: Option<Timestamp>,
    pub cancelled_at: Option<Timestamp>,
}

impl EmailChangeLink {
    /// When a link asked for at `now` stops working by itself. `None` if that is beyond the supported range.
    pub fn end_from(now: Timestamp) -> Option<Timestamp> {
        now.checked_add(EMAIL_CHANGE_LINK_LIFETIME)
    }

    /// The state at `now`, checked in this order: used, cancelled, ended, else working.
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

    fn link() -> EmailChangeLink {
        EmailChangeLink {
            user_id: UserId::from_uuid(Uuid::from_u128(1)),
            new_email: Email::parse("new@example.com").expect("an address"),
            created_at: Timestamp::EPOCH,
            expires_at: at(EMAIL_CHANGE_LINK_LIFETIME),
            used_at: None,
            cancelled_at: None,
        }
    }

    #[gtest]
    fn a_link_works_until_its_day_is_over() {
        let day = EMAIL_CHANGE_LINK_LIFETIME;
        expect_that!(link().state(Timestamp::EPOCH), eq(ResetLinkState::Working));
        expect_that!(
            link().state(at(Duration::from_millis(day.as_millis() - 1))),
            eq(ResetLinkState::Working)
        );
        expect_that!(link().state(at(day)), eq(ResetLinkState::Ended));
        expect_that!(EmailChangeLink::end_from(Timestamp::EPOCH), some(eq(at(day))));
    }

    #[gtest]
    fn used_and_cancelled_stay_what_they_are_after_the_day() {
        let late = at(Duration::from_days(2));
        let used = EmailChangeLink {
            used_at: Some(Timestamp::EPOCH),
            cancelled_at: Some(Timestamp::EPOCH),
            ..link()
        };
        let cancelled = EmailChangeLink {
            cancelled_at: Some(Timestamp::EPOCH),
            ..link()
        };
        expect_that!(used.state(late), eq(ResetLinkState::Used));
        expect_that!(cancelled.state(late), eq(ResetLinkState::Cancelled));
    }

    #[gtest]
    fn a_state_that_does_not_work_has_its_error() {
        expect_that!(ResetLinkState::Working.email_change_error(), none());
        expect_that!(
            ResetLinkState::Used.email_change_error(),
            some(eq(EmailChangeLinkError::Used))
        );
        expect_that!(
            ResetLinkState::Cancelled.email_change_error(),
            some(eq(EmailChangeLinkError::Cancelled))
        );
        expect_that!(
            ResetLinkState::Ended.email_change_error(),
            some(eq(EmailChangeLinkError::Ended))
        );
    }
}
