//! The signed-in user, their teams and heaps.

use domain::personal_token::TokenExpiry;
use domain::user::User;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
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
    /// Change the password (`POST /me/password`). Not for a user who signs in through a provider, who has none.
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
    /// How many other sign-ins the change ended that were still live: other browsers, clients such as the
    /// command line, and personal tokens. The browser it was made from stays signed in on a new session, and isn't counted.
    pub ended_sign_ins: u32,
}

/// Which page of the account history to read.
#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct AccountHistoryQueryDto {
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1 to 200. Defaults to 50.
    pub limit: Option<u64>,
}

/// What happened to the account.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AccountEventKind)]
pub enum AccountEventKindDto {
    /// The user changed their password in Account.
    PasswordChanged,
    /// The operator made a reset link for the user.
    PasswordResetLinkMade,
    /// A password was set with a reset link.
    PasswordReset,
    /// A kind this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

/// One password event in the account's history.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AccountEvent)]
pub struct AccountEventDto {
    pub id: Uuid,
    pub kind: AccountEventKindDto,
    /// One sentence worded by the server, without a name: "Password changed. 2 other sign-ins ended."
    pub description: String,
    /// UTC milliseconds.
    pub created_at: i64,
    /// When it happened, in the request's timezone, as the audit log words it: "9 Nov 2026, 14:05".
    pub created_label: String,
}

/// A page of the caller's account history, newest first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AccountHistory)]
pub struct AccountHistoryDto {
    pub entries: Vec<AccountEventDto>,
    /// Pass as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
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

/// One live personal token. The secret is never shown again after it is made.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PersonalToken)]
pub struct PersonalTokenDto {
    /// Names the token in `DELETE /me/personal-tokens/{id}`.
    pub id: Uuid,
    /// What the person called it.
    pub name: String,
    /// UTC milliseconds.
    pub created_at: i64,
    /// The day it was made, in the request's timezone: "today", "yesterday", "3 Nov", or "3 Nov 2025".
    pub created_label: String,
    /// UTC milliseconds. `null` for a token that never expires.
    pub expires_at: Option<i64>,
    /// The day it stops working, worded like `created_label`. `null` for a token that never expires.
    pub expires_label: Option<String>,
    /// UTC milliseconds, precise to a minute. `null` until it is first used.
    pub last_used_at: Option<i64>,
    /// How long ago it was last used, in calendar days in the request's timezone: "today", "2 days ago". `null`
    /// until it is first used.
    pub last_used_label: Option<String>,
}

/// How long a new personal token lasts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = PersonalTokenExpiry)]
pub enum PersonalTokenExpiryDto {
    #[serde(rename = "30_days")]
    Days30,
    #[default]
    #[serde(rename = "90_days")]
    Days90,
    /// 365 days.
    #[serde(rename = "1_year")]
    Year1,
    #[serde(rename = "never")]
    Never,
}

impl From<PersonalTokenExpiryDto> for TokenExpiry {
    fn from(expiry: PersonalTokenExpiryDto) -> Self {
        match expiry {
            PersonalTokenExpiryDto::Days30 => Self::Days30,
            PersonalTokenExpiryDto::Days90 => Self::Days90,
            PersonalTokenExpiryDto::Year1 => Self::Year1,
            PersonalTokenExpiryDto::Never => Self::Never,
        }
    }
}

/// What the signed-in user gives to make a personal token.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PersonalTokenMake)]
pub struct PersonalTokenMakeDto {
    /// What to call it, to tell it from the others: at most 64 characters, trimmed, without control characters.
    pub name: String,
    /// How long it lasts. Defaults to `90_days`.
    #[serde(default)]
    pub expires_in: PersonalTokenExpiryDto,
}

/// A personal token just made.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PersonalTokenMade)]
pub struct PersonalTokenMadeDto {
    pub token: PersonalTokenDto,
    /// The secret, `uph_` and a random value, to give as `Authorization: Bearer`. Shown in this response only.
    pub secret: String,
}

/// The caller's live personal tokens, newest first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PersonalTokenList)]
pub struct PersonalTokenListDto {
    pub tokens: Vec<PersonalTokenDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Me)]
pub struct MeDto {
    pub user: UserDto,
    pub teams: Vec<TeamDto>,
    /// What the user may do with their account, such as signing out and making teams.
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<MeActionDto>,
}

#[derive(Deserialize, ToSchema)]
#[schema(as = MePatchRequest)]
pub struct MePatchDto {
    /// 1 to 100 characters.
    pub display_name: String,
}
