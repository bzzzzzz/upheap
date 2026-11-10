//! Opening an invite link, joining a team through it and creating an account through it, by the link's token. The
//! local build has no teams to join, so it leaves these out (the `accounts` feature).

use api_types::auth::RegisterRequest;
use api_types::invite_links::{
    InvitationActionDto, InvitationDto, InviteLinkStateDto, InviteRegistrationDto, InviteViewerDto,
    InviteViewerStatusDto,
};
use api_types::me::UserDto;
use api_types::teams::TeamDto;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;
use domain::invite_link::InviteToken;
use serde::Deserialize;
use service::ServiceError;
use service::auth::Registration;
use service::teams::{InvitationView, InviteViewer, MemberAdded};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{cookie_settings, signed_in_with};
use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, MaybeUser, SessionUser};
use crate::teams::to_team_dto;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_invite))
        .routes(routes!(join_by_invite))
        .routes(routes!(register_by_invite))
}

/// `{token}`, for documenting the path as well as reading it.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct InviteTokenParams {
    /// The invite link's secret: the last part of the link, `{origin}/invite/{token}`.
    token: String,
}

/// The token in the path. One that can't be a token names nothing, so it is `404` without reading the database.
fn parse_token(params: InviteTokenParams) -> Result<InviteToken, ServiceError> {
    InviteToken::parse(&params.token).ok_or(ServiceError::NotFound)
}

fn to_invitation_dto(view: InvitationView) -> InvitationDto {
    let viewer = view.viewer.map(|viewer| match viewer {
        InviteViewer::NotMember => (InviteViewerStatusDto::NotMember, None),
        InviteViewer::Member => (InviteViewerStatusDto::Member, Some(view.team.slug.as_str().to_owned())),
        InviteViewer::Removed => (InviteViewerStatusDto::Removed, None),
    });
    InvitationDto {
        team_name: view.team.name.as_str().to_owned(),
        created_by_name: view.created_by.display_name.as_str().to_owned(),
        state: InviteLinkStateDto::from(view.state),
        viewer: viewer.map(|(status, team_slug)| InviteViewerDto { status, team_slug }),
        allowed_actions: view
            .allowed_actions
            .into_iter()
            .map(InvitationActionDto::from)
            .collect(),
    }
}

/// Opens an invite link: the team's name, the maker's name and whether the link works, ended or was revoked. Anyone
/// holding the token may, signed in or not; with a session cookie or bearer token the answer also says where the
/// caller stands in the team (`viewer`). An expired or unknown session cookie counts as signed out.
#[utoipa::path(get, path = "/invites/{token}", tag = "teams", security((), ("session" = [])),
    params(InviteTokenParams),
    responses(
        (status = 200, body = InvitationDto),
        (status = 401, description = "`unauthenticated`: the bearer token is invalid, expired or revoked", body = Problem),
        (status = 404, description = "No such link", body = Problem),
    ))]
async fn open_invite(
    State(state): State<AppState>,
    MaybeUser(principal): MaybeUser,
    path: ApiPath<InviteTokenParams>,
) -> Result<Json<InvitationDto>, ApiError> {
    let token = parse_token(path.0)?;
    let view = state.services.open_invite(principal, &token).await?;
    Ok(Json(to_invitation_dto(view)))
}

/// Joins the link's team as a member. Takes the session cookie only, never a bearer token: joining is the person's
/// decision in their browser, not something a client holding a token can do for itself. The link must be working at
/// this moment, and must not have been made before the caller was removed from the team. Answers `200` with the team
/// unchanged when the caller is already in it, as member or admin, whatever the link's state. The link keeps
/// working for others.
#[utoipa::path(post, path = "/invites/{token}/join", tag = "teams", security(("session" = [])),
    params(InviteTokenParams),
    responses(
        (status = 201, description = "Joined", body = TeamDto),
        (status = 200, description = "Already in the team", body = TeamDto),
        (status = 401, description = "`unauthenticated`: no session cookie, or the request carried a bearer token", body = Problem),
        (status = 403, description = "`removed_from_team`: removed from the team after the link was made", body = Problem),
        (status = 404, description = "No such link", body = Problem),
        (status = 409, description = "`invite_link_ended` or `invite_link_revoked`", body = Problem),
    ))]
async fn join_by_invite(
    State(state): State<AppState>,
    SessionUser(principal): SessionUser,
    path: ApiPath<InviteTokenParams>,
) -> Result<(StatusCode, Json<TeamDto>), ApiError> {
    let token = parse_token(path.0)?;
    let (detail, outcome) = state.services.join_team_by_invite(principal, &token).await?;
    let status = match outcome {
        MemberAdded::Added => StatusCode::CREATED,
        MemberAdded::AlreadyMember => StatusCode::OK,
    };
    Ok((status, Json(to_team_dto(detail, &state.authentication))))
}

/// Creates an account through the link, and joins its team as a member in the same act: the account, its personal
/// team and heap, the membership and the session are all made, or none is. Works whether registration is open or
/// closed on the server. Takes the same body as `/auth/register` and checks it the same way, and answers `409`
/// `invite_link_ended` or `invite_link_revoked` when the link isn't working at this moment, even if the username is
/// also taken. Sets the session cookie, replacing any the caller had. The link keeps working for others.
#[utoipa::path(post, path = "/invites/{token}/register", tag = "auth", request_body = RegisterRequest,
    params(InviteTokenParams),
    responses(
        (status = 201, description = "Registered, joined and signed in; sets the session cookie", body = InviteRegistrationDto),
        (status = 404, description = "No such link", body = Problem),
        (status = 409, description = "`invite_link_ended`, `invite_link_revoked`, `email_taken` (`field` `email`), or `handle_taken` (`field` `username`)", body = Problem),
        (status = 422, description = "Invalid email, name or password, or `invalid_handle` with a `hint`", body = Problem),
    ))]
async fn register_by_invite(
    State(state): State<AppState>,
    path: ApiPath<InviteTokenParams>,
    ApiJson(body): ApiJson<RegisterRequest>,
) -> Result<Response, ApiError> {
    let token = parse_token(path.0)?;
    let registration = Registration {
        email: &body.email,
        username: &body.username,
        display_name: &body.display_name,
        password: &body.password,
    };
    let cookies = cookie_settings(&state)?;
    let (grant, detail) = state.services.register_through_invite(&token, registration).await?;
    let answer = InviteRegistrationDto {
        user: UserDto::from(&grant.user),
        team: to_team_dto(detail, &state.authentication),
    };
    Ok(signed_in_with(StatusCode::CREATED, &grant, cookies, answer))
}
