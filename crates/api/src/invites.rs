//! Opening an invite link and joining a team through it, by the link's token. The local build has no teams to join,
//! so it leaves these out (the `accounts` feature).

use api_types::invite_links::{InvitationDto, InviteLinkStateDto, InviteViewerDto, InviteViewerStatusDto};
use api_types::teams::TeamDto;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use domain::invite_link::InviteToken;
use serde::Deserialize;
use service::ServiceError;
use service::teams::{InvitationView, InviteViewer, MemberAdded};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::Problem;
use crate::extract::{ApiPath, MaybeUser, SessionUser};
use crate::teams::to_team_dto;
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(open_invite))
        .routes(routes!(join_by_invite))
}

/// `{token}`, for documenting the path as well as reading it.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct InviteTokenParams {
    /// The invite link's secret: the last part of the link, `{origin}/invite/{token}`.
    token: String,
}

/// The length of a token: 256 random bits as unpadded base64url.
const TOKEN_LENGTH: usize = 43;

/// The token in the path. One that can't be a token names nothing, so it is `404` without reading the database.
fn parse_token(params: InviteTokenParams) -> Result<InviteToken, ServiceError> {
    let valid = params.token.len() == TOKEN_LENGTH
        && params
            .token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if valid {
        Ok(InviteToken::new(params.token))
    } else {
        Err(ServiceError::NotFound)
    }
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
