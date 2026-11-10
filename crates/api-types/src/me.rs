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
    /// List and end the sign-ins of command-line and other clients (`/me/sign-ins`).
    ManageSignIns,
    /// Change the password (`POST /me/password`).
    ChangePassword,
}

/// What the signed-in user gives to change their password.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ChangePasswordRequest)]
pub struct ChangePasswordRequestDto {
    /// The password now in use, to show it is the user and not someone at their open browser.
    pub current_password: String,
    /// The new password, named as in registration so a length problem points at the same field. At least 10
    /// characters. May equal the current one.
    pub password: String,
}

/// What a password change did.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[schema(as = PasswordChanged)]
pub struct PasswordChangedDto {
    /// How many other sign-ins the change ended that were still live: other browsers, and clients such as the
    /// command line. The sign-in it was made from stays.
    pub ended_sign_ins: u32,
}

/// One live sign-in of a client such as the CLI: the tokens it holds can still be used.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ClientSignIn)]
pub struct ClientSignInDto {
    /// Names the sign-in in `DELETE /me/sign-ins/{id}`.
    pub id: Uuid,
    /// The client: `upheap-cli`.
    pub client_id: String,
    /// What to call the client, such as "Upheap CLI".
    pub client_name: String,
    /// What to call this sign-in: the device name, else the client's `client_name`.
    pub label: String,
    /// What the client called the machine, such as its hostname. The client's claim and only a label. `null` for a
    /// sign-in that sent none.
    pub device_name: Option<String>,
    /// UTC milliseconds.
    pub signed_in_at: i64,
    /// The day it was signed in, in the request's timezone: "today", "yesterday", "3 Nov", or "3 Nov 2025".
    pub signed_in_label: String,
    /// UTC milliseconds: when the sign-in last got new tokens.
    pub last_used_at: i64,
    /// How long ago it last got new tokens, in calendar days in the request's timezone: "today", "2 days ago".
    pub last_used_label: String,
}

/// The caller's live sign-ins, the most recently used first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ClientSignInList)]
pub struct ClientSignInListDto {
    pub sign_ins: Vec<ClientSignInDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Me)]
pub struct MeDto {
    pub user: UserDto,
    pub teams: Vec<TeamDto>,
    /// Empty for the local build's one user, who can't sign out and has only the one team.
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<MeActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MePatchRequest)]
pub struct MePatchDto {
    /// 1 to 100 characters.
    pub display_name: String,
}
