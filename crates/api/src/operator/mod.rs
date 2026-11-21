//! What an operator does about accounts: list them, turn a local one off and on, make a reset link and read an
//! account's history. Every endpoint answers `404 not_found` to a caller who isn't an operator, so the pages don't
//! show that operators exist (architecture.md, Authentication and authorization).

use api_types::me::{AccountHistoryDto, AccountHistoryQueryDto};
use api_types::operator::{
    AccountDeactivatedDto, AccountKindDto, OperatorAccountActionDto, OperatorAccountDto, OperatorAccountListDto,
    OperatorResetLinkDto, SignInsToEndDto,
};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::UserId;
use domain::due::describe_ago;
use jiff::tz::TimeZone;
use serde::Deserialize;
use service::operators::{Account, AccountAction, AccountKind, Deactivation, Operator};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::audit::parse_cursor;
use crate::error::Problem;
use crate::extract::{ApiPath, ApiQuery, ClientTimeZone, CurrentUser, TimeZoneHeader};
use crate::me::history::to_account_history_dto;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list_accounts))
        .routes(routes!(count_sign_ins))
        .routes(routes!(deactivate_account))
        .routes(routes!(reactivate_account))
        .routes(routes!(make_reset_link))
        .routes(routes!(get_account_history))
}

/// `{user_id}` in an account's path.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
struct AccountParams {
    /// The account's `id`, from the list.
    user_id: Uuid,
}

fn to_account_dto(account: Account, as_of: domain::Timestamp, tz: &TimeZone) -> OperatorAccountDto {
    OperatorAccountDto {
        id: *account.user.id.as_uuid(),
        username: account.user.username.to_string(),
        display_name: account.user.display_name.as_str().to_owned(),
        email: account.user.email.as_str().to_owned(),
        kind: match account.kind {
            AccountKind::Local => AccountKindDto::Local,
            AccountKind::Provider => AccountKindDto::Provider,
        },
        confirmed: account.confirmed,
        deactivated: account.deactivated,
        operator: account.operator,
        last_sign_in_at: account.last_sign_in_at.map(domain::Timestamp::as_millis),
        last_sign_in_label: account.last_sign_in_at.map(|at| describe_ago(at, as_of, tz)),
        allowed_actions: account
            .allowed_actions
            .iter()
            .map(|action| match action {
                AccountAction::MakeResetLink => OperatorAccountActionDto::MakeResetLink,
                AccountAction::Deactivate => OperatorAccountActionDto::Deactivate,
                AccountAction::Reactivate => OperatorAccountActionDto::Reactivate,
                AccountAction::History => OperatorAccountActionDto::History,
            })
            .collect(),
    }
}

/// Every account on the server, by display name ignoring case, then username. Not paged: a server has tens to a few
/// hundred accounts, so a client searches and filters the list itself. For operators only.
#[utoipa::path(get, path = "/operator/accounts", tag = "operator", security(("session" = [])), params(TimeZoneHeader),
    responses(
        (status = 200, body = OperatorAccountListDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "The caller isn't an operator", body = Problem),
    ))]
async fn list_accounts(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<OperatorAccountListDto>, ApiError> {
    let accounts = state.services.list_accounts(Operator::Account(principal)).await?;
    let as_of = state.services.now();
    Ok(Json(OperatorAccountListDto {
        accounts: accounts
            .into_iter()
            .map(|account| to_account_dto(account, as_of, &tz))
            .collect(),
    }))
}

/// How many live sign-ins and personal tokens turning the account off would end, to say so before asking. For
/// operators only.
#[utoipa::path(get, path = "/operator/accounts/{user_id}/sign-ins", tag = "operator", security(("session" = [])),
    params(AccountParams),
    responses(
        (status = 200, body = SignInsToEndDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "The caller isn't an operator, or there is no such account", body = Problem),
    ))]
async fn count_sign_ins(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(AccountParams { user_id }): ApiPath<AccountParams>,
) -> Result<Json<SignInsToEndDto>, ApiError> {
    let counts = state
        .services
        .count_sign_ins_to_end(principal, UserId::from_uuid(user_id))
        .await?;
    Ok(Json(SignInsToEndDto {
        sign_ins: counts.sign_ins,
        personal_tokens: counts.personal_tokens,
    }))
}

/// Turns a local account off: it can't sign in, and every sign-in it has ends (web sessions, sign-ins of clients and
/// personal tokens). Its items, teams and memberships stay. An account that is off already answers with
/// `ended_sign_ins` 0 and records nothing. For operators only.
#[utoipa::path(post, path = "/operator/accounts/{user_id}/deactivate", tag = "operator", security(("session" = [])),
    params(AccountParams),
    responses(
        (status = 200, body = AccountDeactivatedDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "The caller isn't an operator, or there is no such account", body = Problem),
        (status = 409, description = "`account_provider_managed`: the sign-in provider owns the account's state; `own_account`: the caller's own account", body = Problem),
    ))]
async fn deactivate_account(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(AccountParams { user_id }): ApiPath<AccountParams>,
) -> Result<Json<AccountDeactivatedDto>, ApiError> {
    let done = state
        .services
        .deactivate_account(Operator::Account(principal), UserId::from_uuid(user_id))
        .await?;
    Ok(Json(AccountDeactivatedDto {
        ended_sign_ins: match done {
            Deactivation::Ended(ended) => ended,
            Deactivation::AlreadyOff => 0,
        },
    }))
}

/// Turns a local account back on. The sign-ins that ended stay ended. An account that isn't off answers `204` and
/// records nothing. For operators only.
#[utoipa::path(post, path = "/operator/accounts/{user_id}/reactivate", tag = "operator", security(("session" = [])),
    params(AccountParams),
    responses(
        (status = 204, description = "Turned back on, or wasn't off"),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "The caller isn't an operator, or there is no such account", body = Problem),
        (status = 409, description = "`account_provider_managed`: the sign-in provider owns the account's state", body = Problem),
    ))]
async fn reactivate_account(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(AccountParams { user_id }): ApiPath<AccountParams>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .reactivate_account(Operator::Account(principal), UserId::from_uuid(user_id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Makes a password reset link for the account, which works once, for an hour, and cancels the account's earlier
/// link. The link is in this response only. For operators only.
#[utoipa::path(post, path = "/operator/accounts/{user_id}/reset-link", tag = "operator", security(("session" = [])),
    params(AccountParams),
    responses(
        (status = 200, body = OperatorResetLinkDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 403, description = "`password_sign_in_off`; `account_turned_off`: an operator turned the account off", body = Problem),
        (status = 404, description = "The caller isn't an operator, or there is no such account", body = Problem),
        (status = 409, description = "`account_provider_managed`: the sign-in provider owns the account's state", body = Problem),
    ))]
async fn make_reset_link(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiPath(AccountParams { user_id }): ApiPath<AccountParams>,
) -> Result<Json<OperatorResetLinkDto>, ApiError> {
    let made = state
        .services
        .create_operator_reset_link(principal, UserId::from_uuid(user_id))
        .await?;
    Ok(Json(OperatorResetLinkDto {
        url: format!("{}/reset/{}", state.services.base_url(), made.token.expose()),
        expires_at: made.expires_at.as_millis(),
    }))
}

/// The account's history, newest first, as the person's own (`/me/history`) but worded for an operator: who turned
/// the account off or made a reset link is named. For operators only.
#[utoipa::path(get, path = "/operator/accounts/{user_id}/history", tag = "operator", security(("session" = [])),
    params(AccountParams, AccountHistoryQueryDto, TimeZoneHeader),
    responses(
        (status = 200, body = AccountHistoryDto),
        (status = 400, description = "`invalid_cursor`", body = Problem),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 404, description = "The caller isn't an operator, or there is no such account", body = Problem),
    ))]
async fn get_account_history(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
    ApiPath(AccountParams { user_id }): ApiPath<AccountParams>,
    query: Result<ApiQuery<AccountHistoryQueryDto>, ApiError>,
) -> Result<Json<AccountHistoryDto>, ApiError> {
    // The role first, so a non-operator gets 404 for a bad query too.
    state.services.require_operator(principal).await?;
    let ApiQuery(query) = query?;
    let after = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let page = state
        .services
        .list_operator_account_history(
            principal,
            UserId::from_uuid(user_id),
            after,
            query.limit.unwrap_or(50),
            &tz,
        )
        .await?;
    Ok(Json(to_account_history_dto(page)))
}
