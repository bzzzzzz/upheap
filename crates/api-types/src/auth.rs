//! Registration and sign-in for local accounts.

use domain::password_reset::ResetLinkState;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email: String,
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Becomes the slug of
    /// the personal team, and can't be changed later.
    #[schema(example = "eugene")]
    pub username: String,
    pub display_name: String,
    pub password: String,
}

#[derive(Deserialize, ToSchema)]
pub struct SignInRequest {
    /// An email address, or a username. `email` is still accepted in its place.
    #[serde(alias = "email")]
    pub login: String,
    pub password: String,
}

/// Where a password reset link is in its life. `used`, `cancelled` and `ended` are final.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = PasswordResetState)]
pub enum PasswordResetStateDto {
    /// Not used, not cancelled, and less than 60 minutes old.
    Working,
    /// A password was set with it.
    Used,
    /// A newer link was made for the account, or its password was changed while this one worked.
    Cancelled,
    /// 60 minutes old, and neither used nor cancelled before that.
    Ended,
}

impl From<ResetLinkState> for PasswordResetStateDto {
    fn from(state: ResetLinkState) -> Self {
        match state {
            ResetLinkState::Working => Self::Working,
            ResetLinkState::Used => Self::Used,
            ResetLinkState::Cancelled => Self::Cancelled,
            ResetLinkState::Ended => Self::Ended,
        }
    }
}

/// A password reset link, as whoever holds it sees it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PasswordReset)]
pub struct PasswordResetDto {
    pub state: PasswordResetStateDto,
    /// The account the link is for, as it is now. Only while the link works: an old link found later in a chat
    /// mustn't keep naming the account.
    pub account: Option<PasswordResetAccountDto>,
}

/// The account a working reset link is for.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PasswordResetAccount)]
pub struct PasswordResetAccountDto {
    pub display_name: String,
    pub username: String,
}

/// Whether this server mails password reset links.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = PasswordResetOptions)]
pub struct PasswordResetOptionsDto {
    /// `true` when the server has mail set up, so a signed-out user can ask for a link by email.
    pub by_mail: bool,
}

/// The address a signed-out user asks a reset link for.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RequestPasswordReset)]
pub struct RequestPasswordResetDto {
    /// The email address of the account. Any case.
    pub email: String,
}

/// What whoever holds a working reset link gives to set a new password.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SetPasswordRequest)]
pub struct SetPasswordRequestDto {
    /// The new password, named as in registration so a length problem points at the same field. At least 10
    /// characters. May equal the current one.
    pub password: String,
}

/// Where to send the browser to end the person's session at the identity provider too.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct SignedOut {
    /// The provider's end session endpoint, with the ID token hint, the client and where to come back to (`/login`).
    pub provider_logout_url: String,
}

/// The OpenID Connect provider people may sign in through on this server.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OidcProvider)]
pub struct OidcProviderDto {
    /// What the sign-in button names, such as `Acme SSO`.
    pub label: String,
}

/// Who the provider vouched for, shown on the username step.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OidcPending)]
pub struct OidcPendingDto {
    pub display_name: String,
    pub email: String,
}

/// The username a person picks to finish a first sign-in through the provider.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
#[schema(as = OidcRegisterRequest)]
pub struct OidcRegisterRequest {
    /// Lowercase letters, digits and single dashes, starting with a letter, 2 to 39 characters. Becomes the slug of
    /// the personal team, and can't be changed later.
    #[schema(example = "eugene")]
    pub username: String,
}

/// A user made through the provider, signed in.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OidcRegistered)]
pub struct OidcRegisteredDto {
    pub user: crate::me::UserDto,
    /// The app path the sign-in started from, to go to next, if it started from one.
    pub redirect: Option<String>,
}
