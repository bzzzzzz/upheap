//! The handles in a path (`/teams/{team}/heaps/{heap}/items/{item}`), found for the caller (design/architecture.md, Handles).
//!
//! Each extractor authenticates first, so a caller who isn't signed in gets `401` before anything else. A handle
//! that isn't well formed names nothing, so it is `404` like one nobody has.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
#[cfg(feature = "accounts")]
use domain::InviteLinkId;
#[cfg(feature = "accounts")]
use domain::UserId;
use domain::handle::{Handle, ItemKey};
use domain::step::StepNumber;
use domain::validation::Field;
use domain::{ItemId, TeamId};
use serde::Deserialize;
use service::handles::HeapRef;
use service::{Principal, ServiceError};
use utoipa::IntoParams;
#[cfg(feature = "accounts")]
use uuid::Uuid;

use crate::extract::{ApiPath, CurrentUser};
use crate::{ApiError, AppState};

/// `{team}`, for documenting paths as well as reading them.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct TeamParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct HeapParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
    /// The heap's slug, unique within the team.
    #[param(example = "backlog")]
    heap: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct ItemParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
    /// The heap's slug, unique within the team.
    #[param(example = "backlog")]
    heap: String,
    /// The item's key, unique within the heap.
    #[param(example = "k3x9a")]
    item: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct StepParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
    /// The heap's slug, unique within the team.
    #[param(example = "backlog")]
    heap: String,
    /// The item's key, unique within the heap.
    #[param(example = "k3x9a")]
    item: String,
    /// The step's number within its item: a positive integer, given when the step was added.
    #[param(example = 3)]
    step: String,
}

#[cfg(feature = "accounts")]
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct MemberParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
    /// The user's username, which needn't be a member's.
    #[param(example = "eugene")]
    username: String,
}

#[cfg(feature = "accounts")]
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
pub(crate) struct InviteLinkParams {
    /// The team's slug.
    #[param(example = "my-team")]
    team: String,
    /// The invite link's `id`, from the list.
    id: String,
}

/// `{team}`: a team the caller belongs to.
pub(crate) struct TeamPath {
    pub principal: Principal,
    pub team_id: TeamId,
}

impl FromRequestParts<AppState> for TeamPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<TeamParams>::from_request_parts(parts, state).await?;
        let team_id = state.services.resolve_team(principal, &handle(&params.team)?).await?;
        Ok(Self { principal, team_id })
    }
}

/// `{team}/heaps/{heap}`: a heap of a team the caller belongs to.
pub(crate) struct HeapPath {
    pub principal: Principal,
    pub heap: HeapRef,
}

impl FromRequestParts<AppState> for HeapPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<HeapParams>::from_request_parts(parts, state).await?;
        let heap = state
            .services
            .resolve_heap(principal, &handle(&params.team)?, &handle(&params.heap)?)
            .await?;
        Ok(Self { principal, heap })
    }
}

/// `{team}/heaps/{heap}/items/{item}`: an item, by its key, of a heap of a team the caller belongs to.
pub(crate) struct ItemPath {
    pub principal: Principal,
    pub item_id: ItemId,
}

impl FromRequestParts<AppState> for ItemPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<ItemParams>::from_request_parts(parts, state).await?;
        let key = ItemKey::parse(&params.item).map_err(|_| ServiceError::NotFound)?;
        let item_id = state
            .services
            .resolve_item(principal, &handle(&params.team)?, &handle(&params.heap)?, key)
            .await?;
        Ok(Self { principal, item_id })
    }
}

/// `{team}/heaps/{heap}/items/{item}/steps/{step}`: a step, by its number, of an item as [`ItemPath`] finds it. The
/// step itself isn't looked up here: whether the item has it is the service's to say, under the heap's lock.
pub(crate) struct StepPath {
    pub principal: Principal,
    pub item_id: ItemId,
    pub number: StepNumber,
}

impl FromRequestParts<AppState> for StepPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<StepParams>::from_request_parts(parts, state).await?;
        let key = ItemKey::parse(&params.item).map_err(|_| ServiceError::NotFound)?;
        let item_id = state
            .services
            .resolve_item(principal, &handle(&params.team)?, &handle(&params.heap)?, key)
            .await?;
        let number = StepNumber::parse(&params.step).ok_or(ServiceError::NotFound)?;
        Ok(Self {
            principal,
            item_id,
            number,
        })
    }
}

/// `{team}/members/{username}`: a team the caller belongs to, and a user, who may or may not be a member. Only team
/// management names members, and it needs accounts.
#[cfg(feature = "accounts")]
pub(crate) struct MemberPath {
    pub principal: Principal,
    pub team_id: TeamId,
    pub user_id: UserId,
}

#[cfg(feature = "accounts")]
impl FromRequestParts<AppState> for MemberPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<MemberParams>::from_request_parts(parts, state).await?;
        let (team_id, user_id) = state
            .services
            .resolve_member(principal, &handle(&params.team)?, &handle(&params.username)?)
            .await?;
        Ok(Self {
            principal,
            team_id,
            user_id,
        })
    }
}

/// `{team}/invite-links/{id}`: a team the caller belongs to, and the id of one of its invite links, which may not
/// exist. Only team management names invite links, and it needs accounts.
#[cfg(feature = "accounts")]
pub(crate) struct InviteLinkPath {
    pub principal: Principal,
    pub team_id: TeamId,
    pub link_id: InviteLinkId,
}

#[cfg(feature = "accounts")]
impl FromRequestParts<AppState> for InviteLinkPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<InviteLinkParams>::from_request_parts(parts, state).await?;
        let team_id = state.services.resolve_team(principal, &handle(&params.team)?).await?;
        let link_id = Uuid::parse_str(&params.id)
            .map(InviteLinkId::from_uuid)
            .map_err(|_| ServiceError::NotFound)?;
        Ok(Self {
            principal,
            team_id,
            link_id,
        })
    }
}

fn handle(raw: &str) -> Result<Handle, ServiceError> {
    Handle::parse(raw, Field::Slug).map_err(|_| ServiceError::NotFound)
}
