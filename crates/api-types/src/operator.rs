//! What an operator reads and does about accounts.

use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

/// How an account signs in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = AccountKind)]
pub enum AccountKindDto {
    /// A local password. An operator can turn it off and on, and make a reset link.
    Local,
    /// An identity at the sign-in provider, which owns the account's state.
    Provider,
}

/// What the caller may do about an account in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = OperatorAccountAction)]
pub enum OperatorAccountActionDto {
    /// `POST .../reset-link`: a local, active account, on a server with password sign-in.
    MakeResetLink,
    /// `POST .../deactivate`: a local, active account that isn't the caller's own.
    Deactivate,
    /// `POST .../reactivate`: a local account an operator turned off, not the caller's own.
    Reactivate,
    /// `GET .../history`: every account.
    History,
}

/// An account in the operator's list.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OperatorAccount)]
pub struct OperatorAccountDto {
    pub id: Uuid,
    pub username: String,
    pub display_name: String,
    pub email: String,
    pub kind: AccountKindDto,
    /// Whether the address is confirmed.
    pub confirmed: bool,
    /// Whether an operator turned a local account off, or the provider turned a provider account off.
    pub deactivated: bool,
    /// Whether the account is an operator.
    pub operator: bool,
    /// UTC milliseconds: when a session was last opened for the account. `null` until one is.
    pub last_sign_in_at: Option<i64>,
    /// How long ago, in the request's timezone: "3 days ago". `null` when the account never signed in.
    pub last_sign_in_label: Option<String>,
    pub allowed_actions: Vec<OperatorAccountActionDto>,
}

/// Every account on the server, by display name ignoring case, then username.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OperatorAccountList)]
pub struct OperatorAccountListDto {
    pub accounts: Vec<OperatorAccountDto>,
}

/// What turning an account off would end.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = SignInsToEnd)]
pub struct SignInsToEndDto {
    /// Live web sessions and sign-ins of clients such as the CLI.
    pub sign_ins: u32,
    pub personal_tokens: u32,
}

/// What turning an account off did.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AccountDeactivated)]
pub struct AccountDeactivatedDto {
    /// How many sign-ins ended: web sessions, sign-ins of clients and personal tokens. 0 for an account that was off
    /// already.
    pub ended_sign_ins: u32,
}

/// A reset link just made. This is the only time it is shown.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = OperatorResetLink)]
pub struct OperatorResetLinkDto {
    /// The whole link: `{base_url}/reset/{token}`. Works once, for an hour, and cancels the account's earlier link.
    pub url: String,
    /// UTC milliseconds.
    pub expires_at: i64,
}
