//! A team's invite links.

use domain::invite_link::InviteLinkState;
use domain::permission::InviteLinkAction;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::UserRefDto;

/// Where an invite link is in its life. `ended` and `revoked` are final.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = InviteLinkState)]
pub enum InviteLinkStateDto {
    /// Not revoked, and less than a week old.
    Working,
    /// A week old, and not revoked before that.
    Ended,
    Revoked,
}

impl From<InviteLinkState> for InviteLinkStateDto {
    fn from(state: InviteLinkState) -> Self {
        match state {
            InviteLinkState::Working => Self::Working,
            InviteLinkState::Ended => Self::Ended,
            InviteLinkState::Revoked => Self::Revoked,
        }
    }
}

/// An action the caller can take on an invite link.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = InviteLinkAction)]
pub enum InviteLinkActionDto {
    /// Offered while the link is working.
    Revoke,
}

impl From<InviteLinkAction> for InviteLinkActionDto {
    fn from(action: InviteLinkAction) -> Self {
        match action {
            InviteLinkAction::Revoke => Self::Revoke,
        }
    }
}

/// An invite link: whoever holds its token can join the team until it ends or is revoked.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = InviteLink)]
pub struct InviteLinkDto {
    pub id: Uuid,
    /// The secret, only while the link is `working`; `null` once it has ended or been revoked. A client builds the
    /// link to share from it: `{origin}/invite/{token}`.
    pub token: Option<String>,
    pub state: InviteLinkStateDto,
    /// Who made it, by their display name now, even if they have left the team.
    pub created_by: UserRefDto,
    /// UTC milliseconds.
    pub created_at: i64,
    /// UTC milliseconds: a week after `created_at`.
    pub ends_at: i64,
    /// `ends_at` as a date in the request's timezone: "today", "tomorrow" or "16 Nov".
    pub ends_label: String,
    pub revoked_by: Option<UserRefDto>,
    /// UTC milliseconds.
    pub revoked_at: Option<i64>,
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<InviteLinkActionDto>,
}

/// A team's invite links, newest first.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = InviteLinkList)]
pub struct InviteLinkListDto {
    pub links: Vec<InviteLinkDto>,
}

/// What someone holding an invite link sees of it. Nothing else of the team or the maker is given: no IDs, no
/// members, no maker username or email.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Invitation)]
pub struct InvitationDto {
    /// The team's name now.
    pub team_name: String,
    /// The display name of whoever made the link, now, even if they have left the team.
    pub created_by_name: String,
    /// Where the link is in its life. `ended` and `revoked` are shown too, so the page can say which.
    pub state: InviteLinkStateDto,
    /// The caller's standing in the team; `null` when the request has no signed-in user.
    pub viewer: Option<InviteViewerDto>,
}

/// A signed-in person's standing in the team a link is for.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = InviteViewer)]
pub struct InviteViewerDto {
    pub status: InviteViewerStatusDto,
    /// The team's slug, so the client can take a member to the team; `null` unless `status` is `member`.
    pub team_slug: Option<String>,
}

/// Where a signed-in person stands in the team a link is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = InviteViewerStatus)]
pub enum InviteViewerStatusDto {
    /// Not in the team, and free to join if the link works.
    NotMember,
    /// In the team, as a member or an admin, whatever the link's state.
    Member,
    /// Not in the team, and removed from it after the link was made, so the link won't admit them.
    Removed,
}
