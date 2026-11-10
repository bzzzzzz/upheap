//! A team's invite link: a secret URL token that lets whoever holds it join the team for a week, until an admin
//! revokes it. The token is stored readable (architecture.md, Data model), so admins can copy the link again.
//!
//! A link belongs to the team, not to the admin who made it: it keeps working when they leave or are demoted.

use std::fmt;

use crate::{Duration, InviteLinkId, TeamId, Timestamp, UserId, Version};

/// How long a link works after it was made. The end is derived, never stored.
pub const INVITE_LINK_LIFETIME: Duration = Duration::from_days(7);

/// Why a link can't be used to join its team. The messages name no IDs: they reach people who hold the link but
/// aren't in the team, who may not learn more of it than its name (product.md, Invite links).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum InviteLinkError {
    /// Its week passed.
    #[error("this invite link has ended")]
    Ended { link_id: InviteLinkId, team_id: TeamId },
    #[error("this invite link was revoked")]
    Revoked { link_id: InviteLinkId, team_id: TeamId },
    /// The person was removed from the team after the link was made.
    #[error("you were removed from this team after this invite link was made; ask one of its admins")]
    RemovedSinceMade { link_id: InviteLinkId, team_id: TeamId },
}

/// The secret in an invite link. Never log it.
#[derive(Clone, PartialEq, Eq)]
pub struct InviteToken(String);

/// The length of a token: 256 random bits as unpadded base64url. `service::auth::random_token` makes them and the
/// store's `invite_links.token` column is this wide; both are written out there, so a change here needs both.
pub const INVITE_TOKEN_LENGTH: usize = 43;

impl InviteToken {
    pub fn new(token: String) -> Self {
        Self(token)
    }

    /// A token read from outside (a link's path): exactly [`INVITE_TOKEN_LENGTH`] base64url characters, which is
    /// what 256 random bits as unpadded base64url come to. `None` for anything else, since it can't name a link.
    /// The length check also keeps trailing spaces out of the lookup, which MySQL's PAD SPACE comparison would
    /// otherwise treat as equal to the token without them.
    pub fn parse(raw: &str) -> Option<Self> {
        let valid = raw.len() == INVITE_TOKEN_LENGTH
            && raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        valid.then(|| Self(raw.to_owned()))
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

    /// Whether a person whose last removal from the team was at `last_removed_at` (`None` if never removed) may
    /// join through this link: only if the removal came strictly before the link was made. A tie goes to the
    /// removal. Leaving by choice is not a removal.
    pub fn admits_after_removal(&self, last_removed_at: Option<Timestamp>) -> bool {
        last_removed_at.is_none_or(|removed_at| removed_at < self.created_at)
    }

    /// Whether the link lets a person who is not in the team join at `now`: it must be working, and must not
    /// predate the person's last removal. A revoked link is reported as revoked even when it is also past its week
    /// or predates a removal.
    pub fn check_join(&self, now: Timestamp, last_removed_at: Option<Timestamp>) -> Result<(), InviteLinkError> {
        let (link_id, team_id) = (self.id, self.team_id);
        match self.state(now) {
            InviteLinkState::Revoked => Err(InviteLinkError::Revoked { link_id, team_id }),
            InviteLinkState::Ended => Err(InviteLinkError::Ended { link_id, team_id }),
            InviteLinkState::Working if self.admits_after_removal(last_removed_at) => Ok(()),
            InviteLinkState::Working => Err(InviteLinkError::RemovedSinceMade { link_id, team_id }),
        }
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

    fn made_at(created_at: Timestamp) -> InviteLink {
        InviteLink {
            created_at,
            ..link(None)
        }
    }

    #[gtest]
    fn a_working_link_admits_someone_never_removed() {
        expect_that!(link(None).check_join(Timestamp::EPOCH, None), ok(eq(())));
    }

    #[gtest]
    fn an_ended_link_refuses_the_join() {
        let result = link(None).check_join(at(INVITE_LINK_LIFETIME), None);
        expect_that!(result, err(matches_pattern!(InviteLinkError::Ended { .. })));
    }

    #[gtest]
    fn a_revoked_link_refuses_the_join() {
        let revoked = link(Some(Revocation {
            at: Timestamp::EPOCH,
            by: UserId::from_uuid(Uuid::from_u128(4)),
        }));
        let result = revoked.check_join(Timestamp::EPOCH, None);
        expect_that!(result, err(matches_pattern!(InviteLinkError::Revoked { .. })));
    }

    #[gtest]
    fn a_removal_before_the_link_was_made_does_not_matter() {
        let link = made_at(at(Duration::from_days(2)));
        let removed = Some(at(Duration::from_days(1)));
        expect_that!(link.admits_after_removal(removed), eq(true));
        expect_that!(link.check_join(link.created_at, removed), ok(eq(())));
    }

    #[gtest]
    fn a_removal_after_the_link_was_made_refuses_the_join() {
        let link = made_at(at(Duration::from_days(1)));
        let removed_at = at(Duration::from_days(2));
        expect_that!(link.admits_after_removal(Some(removed_at)), eq(false));
        expect_that!(
            link.check_join(removed_at, Some(removed_at)),
            err(matches_pattern!(InviteLinkError::RemovedSinceMade { .. }))
        );
    }

    #[gtest]
    fn a_removal_at_the_moment_the_link_was_made_refuses_the_join() {
        let link = made_at(at(Duration::from_days(1)));
        expect_that!(link.admits_after_removal(Some(link.created_at)), eq(false));
    }

    #[gtest]
    fn revoked_beats_a_removal() {
        let revoked = InviteLink {
            revocation: Some(Revocation {
                at: Timestamp::EPOCH,
                by: UserId::from_uuid(Uuid::from_u128(4)),
            }),
            ..made_at(at(Duration::from_days(1)))
        };
        let result = revoked.check_join(revoked.created_at, Some(at(Duration::from_days(2))));
        expect_that!(result, err(matches_pattern!(InviteLinkError::Revoked { .. })));
    }

    #[gtest]
    fn a_token_is_43_base64url_characters() {
        let token = "a".repeat(INVITE_TOKEN_LENGTH - 2) + "-_";
        expect_that!(
            InviteToken::parse(&token).map(|t| t.expose().to_owned()),
            some(eq(&token))
        );
        expect_that!(InviteToken::parse(&token[1..]), none());
        expect_that!(InviteToken::parse(&format!("{token}a")), none());
        expect_that!(InviteToken::parse(&format!("{} ", &token[1..])), none());
        expect_that!(InviteToken::parse(&format!("{}+", &token[1..])), none());
        expect_that!(InviteToken::parse(&format!("{}é", &token[2..])), none());
        expect_that!(InviteToken::parse(""), none());
    }

    #[gtest]
    fn the_token_does_not_show_in_debug_output() {
        expect_that!(format!("{:?}", link(None)), not(contains_substring("secret")));
    }
}
