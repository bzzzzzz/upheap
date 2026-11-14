//! The signed-in user's personal tokens: making, listing and revoking them. Only a session cookie may, so a token
//! can't make another.

use api_types::me::{PersonalTokenDto, PersonalTokenListDto, PersonalTokenMadeDto, PersonalTokenMakeDto};
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::due::{describe_ago, describe_date};
use domain::oauth::DeviceName;
use domain::validation::Field;
use domain::{PersonalTokenId, Timestamp, ValidationError};
use jiff::tz::TimeZone;
use serde::Deserialize;
use service::ServiceError;
use service::personal_tokens::PersonalToken;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, ClientTimeZone, SessionUser, TimeZoneHeader};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(
        list_personal_tokens,
        make_personal_token,
        revoke_personal_token
    ))
}

/// `{id}` in the token's path.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
struct PersonalTokenParams {
    /// The token's `id`, from the list.
    id: Uuid,
}

fn to_personal_token_dto(token: PersonalToken, as_of: Timestamp, tz: &TimeZone) -> PersonalTokenDto {
    PersonalTokenDto {
        id: *token.id.as_uuid(),
        name: token.name,
        created_at: token.created_at.as_millis(),
        created_label: describe_date(token.created_at, as_of, tz),
        expires_at: token.expires_at.map(Timestamp::as_millis),
        expires_label: token.expires_at.map(|at| describe_date(at, as_of, tz)),
        last_used_at: token.last_used_at.map(Timestamp::as_millis),
        last_used_label: token.last_used_at.map(|at| describe_ago(at, as_of, tz)),
    }
}

/// The caller's live personal tokens, newest first: not revoked, and not expired. Takes the session cookie only; a
/// request with a bearer token is refused with `401`.
#[utoipa::path(get, path = "/me/personal-tokens", tag = "me", security(("session" = [])), params(TimeZoneHeader),
    responses(
        (status = 200, body = PersonalTokenListDto),
        (status = 401, description = "`unauthenticated`", body = Problem),
    ))]
async fn list_personal_tokens(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    ClientTimeZone(tz): ClientTimeZone,
) -> Result<Json<PersonalTokenListDto>, ApiError> {
    let tokens = state.services.list_personal_tokens(principal).await?;
    let as_of = state.services.now();
    Ok(Json(PersonalTokenListDto {
        tokens: tokens
            .into_iter()
            .map(|token| to_personal_token_dto(token, as_of, &tz))
            .collect(),
    }))
}

/// Makes a personal token: a bearer token that never rotates, for a machine with no browser or keychain. It has all
/// of the caller's permissions, like a command-line sign-in. The `secret` is in this response only. Takes the
/// session cookie only, so a token can't make another.
#[utoipa::path(post, path = "/me/personal-tokens", tag = "me", security(("session" = [])), params(TimeZoneHeader),
    request_body = PersonalTokenMakeDto,
    responses(
        (status = 201, body = PersonalTokenMadeDto),
        (status = 401, description = "`unauthenticated`: no session cookie, or the request carried a bearer token", body = Problem),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 422, description = "`field_empty` or `invalid_fields` (`field` `name`)", body = Problem),
    ))]
async fn make_personal_token(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    ClientTimeZone(tz): ClientTimeZone,
    ApiJson(body): ApiJson<PersonalTokenMakeDto>,
) -> Result<(StatusCode, Json<PersonalTokenMadeDto>), ApiError> {
    let name = DeviceName::parse(&body.name)
        .map_err(|_| ServiceError::Invalid(ValidationError::FieldInvalid { field: Field::Name }))?
        .ok_or(ServiceError::Invalid(ValidationError::Empty { field: Field::Name }))?;
    let made = state
        .services
        .make_personal_token(principal, &name, body.expires_in.into())
        .await?;
    let as_of = state.services.now();
    Ok((
        StatusCode::CREATED,
        Json(PersonalTokenMadeDto {
            token: to_personal_token_dto(made.token, as_of, &tz),
            secret: made.secret.expose().to_owned(),
        }),
    ))
}

/// Revokes a personal token: it stops working at once. Succeeds when it was revoked or expired already. Takes the
/// session cookie only.
#[utoipa::path(delete, path = "/me/personal-tokens/{id}", tag = "me", security(("session" = [])),
    params(PersonalTokenParams),
    responses(
        (status = 204, description = "Revoked, or it was revoked or expired already"),
        (status = 401, description = "`unauthenticated`", body = Problem),
        (status = 403, description = "`csrf_header_missing`", body = Problem),
        (status = 404, description = "No such token, or it isn't the caller's", body = Problem),
    ))]
async fn revoke_personal_token(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    ApiPath(PersonalTokenParams { id }): ApiPath<PersonalTokenParams>,
) -> Result<StatusCode, ApiError> {
    state
        .services
        .revoke_personal_token(principal, PersonalTokenId::from_uuid(id))
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
