//! A team's invite link: a secret URL token that lets whoever holds it join the team for a week, until an admin
//! revokes it. The token is stored readable (architecture.md, Data model), so admins can copy the link again.
//!
//! A link belongs to the team, not to the admin who made it: it keeps working when they leave or are demoted.

use std::fmt;

use crate::{Duration, InviteLinkId, TeamId, Timestamp, UserId, Version};

/// How long a link works after it was made. The end is derived, never stored.
pub const INVITE_LINK_LIFETIME: Duration = Duration::from_days(7);

/// The secret in an invite link. Never log it.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteToken(String);

impl InviteToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// The secret itself, for the responses that show it and for storing it.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for InviteToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("InviteToken(<redacted>)")
    }
}

/// Who revoked a link, and when. One type, so a link can't be revoked without either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Revocation {
    pub at: Timestamp,
    pub by: UserId,
}

/// Where a link is in its life. `Ended` and `Revoked` are final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InviteLinkState {
    Working,
    /// Its week passed.
    Ended,
    Revoked,
}

impl InviteLinkState {
    /// The stable code used in audit diffs.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Working => "working",
            Self::Ended => "ended",
            Self::Revoked => "revoked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InviteLink {
    pub id: InviteLinkId,
    pub team_id: TeamId,
    pub token: InviteToken,
    pub created_by: UserId,
    pub created_at: Timestamp,
    pub revocation: Option<Revocation>,
    pub version: Version,
}

impl InviteLink {
    /// When the link stops working by itself. Stays at `created_at` if the end would leave the supported range.
    pub fn ends_at(&self) -> Timestamp {
        self.created_at
            .checked_add(INVITE_LINK_LIFETIME)
            .unwrap_or(self.created_at)
    }

    /// Revoked wins over ended: a link revoked and later past its week stays revoked.
    pub fn state(&self, now: Timestamp) -> InviteLinkState {
        if self.revocation.is_some() {
            InviteLinkState::Revoked
        } else if now < self.ends_at() {
            InviteLinkState::Working
        } else {
            InviteLinkState::Ended
        }
    }

    pub fn is_working(&self, now: Timestamp) -> bool {
        self.state(now) == InviteLinkState::Working
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn link(revocation: Option<Revocation>) -> InviteLink {
        InviteLink {
            id: InviteLinkId::from_uuid(Uuid::from_u128(1)),
            team_id: TeamId::from_uuid(Uuid::from_u128(2)),
            token: InviteToken::new("secret".to_owned()),
            created_by: UserId::from_uuid(Uuid::from_u128(3)),
            created_at: Timestamp::EPOCH,
            revocation,
            version: Version::INITIAL,
        }
    }

    fn at(offset: Duration) -> Timestamp {
        Timestamp::EPOCH.checked_add(offset).expect("in range")
    }

    #[gtest]
    fn a_link_works_until_its_week_is_over() {
        let week = INVITE_LINK_LIFETIME;
        expect_that!(
            link(None).state(at(Duration::from_millis(week.as_millis() - 1))),
            eq(InviteLinkState::Working)
        );
        expect_that!(link(None).state(at(week)), eq(InviteLinkState::Ended));
        expect_that!(link(None).ends_at(), eq(at(week)));
    }

    #[gtest]
    fn revoked_beats_ended() {
        let revoked = link(Some(Revocation {
            at: Timestamp::EPOCH,
            by: UserId::from_uuid(Uuid::from_u128(4)),
        }));
        expect_that!(revoked.state(Timestamp::EPOCH), eq(InviteLinkState::Revoked));
        expect_that!(revoked.state(at(Duration::from_days(8))), eq(InviteLinkState::Revoked));
        expect_that!(revoked.is_working(Timestamp::EPOCH), eq(false));
    }

    #[gtest]
    fn the_token_does_not_show_in_debug_output() {
        expect_that!(format!("{:?}", link(None)), not(contains_substring("secret")));
    }
}
