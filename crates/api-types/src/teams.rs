//! Teams, their members and adding heaps to them.

use domain::permission::{MemberAction, TeamAction};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::RoleDto;
use crate::heaps::HeapDto;

/// An action the caller can take on a team.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TeamAction)]
pub enum TeamActionDto {
    Rename,
    /// Not offered on a personal team, whose slug is its owner's username.
    ChangeSlug,
    /// Not offered on a personal team.
    AddMember,
    CreateHeap,
    Leave,
}

impl From<TeamAction> for TeamActionDto {
    fn from(action: TeamAction) -> Self {
        match action {
            TeamAction::Rename => Self::Rename,
            TeamAction::ChangeSlug => Self::ChangeSlug,
            TeamAction::AddMember => Self::AddMember,
            TeamAction::CreateHeap => Self::CreateHeap,
            TeamAction::Leave => Self::Leave,
        }
    }
}

/// An action the caller can take on one member.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = MemberAction)]
pub enum MemberActionDto {
    Promote,
    Demote,
    Remove,
}

impl From<MemberAction> for MemberActionDto {
    fn from(action: MemberAction) -> Self {
        match action {
            MemberAction::Promote => Self::Promote,
            MemberAction::Demote => Self::Demote,
            MemberAction::Remove => Self::Remove,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamMember)]
pub struct MemberDto {
    pub user_id: Uuid,
    /// The member's path is `/teams/{team}/members/{username}`.
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub role: RoleDto,
    /// UTC milliseconds.
    pub joined_at: i64,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<MemberActionDto>,
}

/// A team with its members and heaps, as the caller sees it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Team)]
pub struct TeamDto {
    pub id: Uuid,
    /// Unique among teams and usernames; the team's path is `/teams/{slug}`.
    pub slug: String,
    pub name: String,
    /// Whether this is the caller's personal team: theirs alone, with their username as its slug.
    pub personal: bool,
    /// Send back in `If-Match` when renaming.
    pub version: u32,
    /// The caller's role.
    pub role: RoleDto,
    /// In the order they joined.
    pub members: Vec<MemberDto>,
    /// By name, archived ones included.
    pub heaps: Vec<HeapDto>,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<TeamActionDto>,
}

/// A new team or heap.
#[derive(Deserialize, ToSchema)]
#[schema(as = CreateNamedRequest)]
pub struct CreateNamedDto {
    /// 1 to 100 characters.
    pub name: String,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Leave it out to have
    /// one made from the name: "My Team" gets `my-team`, or `my-team-2` if that is taken.
    pub slug: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = AddMemberRequest)]
pub struct AddMemberDto {
    /// From `GET /users`. New members join as `member`.
    pub user_id: Uuid,
}

/// A change to a team. Leave a field out to keep it.
#[derive(Deserialize, ToSchema)]
#[schema(as = TeamPatchRequest)]
pub struct TeamPatchDto {
    /// 1 to 100 characters.
    pub name: Option<String>,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Refused with
    /// `personal_team` on a personal team. Links with the old slug stop working.
    pub slug: Option<String>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MemberPatchRequest)]
pub struct MemberPatchDto {
    pub role: RoleDto,
}
