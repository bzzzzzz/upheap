//! The signed-in user, their teams and heaps.

use domain::user::User;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{HeapSummaryDto, RoleDto};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = User)]
pub struct UserDto {
    pub id: Uuid,
    /// Also the slug of the user's personal team. Never changes.
    pub username: String,
    pub email: String,
    pub display_name: String,
    /// Send back in `If-Match` when changing the display name.
    pub version: u32,
}

impl From<&User> for UserDto {
    fn from(user: &User) -> Self {
        Self {
            id: *user.id.as_uuid(),
            username: user.username.to_string(),
            email: user.email.as_str().to_owned(),
            display_name: user.display_name.as_str().to_owned(),
            version: user.version.get(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamSummary)]
pub struct TeamDto {
    pub id: Uuid,
    /// The team's path is `/teams/{slug}`.
    pub slug: String,
    pub name: String,
    /// Whether this is the caller's personal team, which is theirs alone.
    pub personal: bool,
    /// The caller's role in this team.
    pub role: RoleDto,
    /// Everyone in the team, the caller included.
    pub member_count: u32,
    /// By name, archived ones included.
    pub heaps: Vec<HeapOverviewDto>,
}

/// A heap of one of the caller's teams, with how many items it has open.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = HeapOverview)]
pub struct HeapOverviewDto {
    #[serde(flatten)]
    pub heap: HeapSummaryDto,
    /// Items the heap's list shows by default, before filters: open and in progress ones, as the list's
    /// `unfiltered_count` with the default view.
    pub open_count: u32,
}

/// An action the caller can take on their account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = MeAction)]
pub enum MeActionDto {
    /// End the session.
    SignOut,
    /// Start a new team.
    CreateTeam,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Me)]
pub struct MeDto {
    pub user: UserDto,
    pub teams: Vec<TeamDto>,
    /// Empty for the local build's one user, who can't sign out and has only the one team.
    pub allowed_actions: Vec<MeActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MePatchRequest)]
pub struct MePatchDto {
    /// 1 to 100 characters.
    pub display_name: String,
}
