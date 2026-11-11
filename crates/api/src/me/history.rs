//! The signed-in user's account history: their password changes and resets.

use api_types::me::{AccountEventDto, AccountEventKindDto, AccountHistoryDto, AccountHistoryQueryDto};
use axum::Json;
use axum::extract::State;
use service::account_history::{AccountEvent, AccountEventKind, AccountHistoryPage};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::audit::{format_cursor, parse_cursor};
use crate::error::Problem;
use crate::extract::{ApiQuery, ClientTimeZone, CurrentUser, TimeZoneHeader};
use crate::oauth::require_accounts;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_history))
}

fn to_kind_dto(kind: AccountEventKind) -> AccountEventKindDto {
    match kind {
        AccountEventKind::PasswordChanged => AccountEventKindDto::PasswordChanged,
        AccountEventKind::PasswordResetLinkMade => AccountEventKindDto::PasswordResetLinkMade,
        AccountEventKind::PasswordReset => AccountEventKindDto::PasswordReset,
    }
}

fn to_account_event_dto(event: AccountEvent) -> AccountEventDto {
    AccountEventDto {
        id: event.id,
        kind: to_kind_dto(event.kind),
        description: event.description,
        created_at: event.created_at.as_millis(),
        created_label: event.created_label,
    }
}

fn to_account_history_dto(page: AccountHistoryPage) -> AccountHistoryDto {
    AccountHistoryDto {
        entries: page.entries.into_iter().map(to_account_event_dto).collect(),
        next_cursor: page.next.map(format_cursor),
    }
}

/// The caller's account history, newest first: their password changes, reset links made for them by whoever runs
/// the server, and passwords set with a reset link. Each entry carries a sentence worded by the server. Only the
/// caller's own history can be read. Read-only, so a command-line sign-in may read it too.
#[utoipa::path(get, path = "/me/history", tag = "me", security(("session" = [])),
    params(AccountHistoryQueryDto, TimeZoneHeader),
    responses(
        (status = 200, body = AccountHistoryDto),
        (status = 400, description = "`invalid_cursor`", body = Problem),
        (status = 401, description = "`unauthenticated`", body = Problem),
    ))]
async fn get_history(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<AccountHistoryQueryDto>,
) -> Result<Json<AccountHistoryDto>, ApiError> {
    require_accounts(&state)?;
    let after = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let page = state
        .services
        .list_account_history(principal, after, query.limit.unwrap_or(50), &tz)
        .await?;
    Ok(Json(to_account_history_dto(page)))
}
