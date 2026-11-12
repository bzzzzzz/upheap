//! The agent's queue and its runs: what to work on next, claiming it, and how a run ends.
//!
//! An agent writes to its item with the `Upheap-Run` header, which the
//! extractor reads (`extract.rs`).

use api_types::waiting::ItemHeapDto;
use api_types::work::{
    ClaimDto, ClaimRequestDto, FinishRequestDto, ReachedRequestDto, RunDto, WorkItemDto, WorkNextDto,
};
use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use domain::RunId;
use domain::run::{RunLink, RunResult, cut_failure, parse_cost_usd};
use serde::Deserialize;
use service::ServiceError;
use service::work::{RunView, WorkItem};
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::error::Problem;
use crate::extract::{ApiJson, ApiPath, ApiQuery, CurrentUser};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(next_item))
        .routes(routes!(claim_run))
        .routes(routes!(heartbeat))
        .routes(routes!(reached))
        .routes(routes!(finish_run))
}

/// The heaps to look for work in.
#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
struct NextQuery {
    /// One to 20 heaps, each `team/heap` by slug, separated by commas. A heap named twice counts once.
    #[param(example = "ops/backlog,ops/infra")]
    #[serde(default)]
    heaps: String,
}

#[derive(Deserialize, IntoParams)]
#[into_params(parameter_in = Path)]
struct RunParams {
    /// The run's ID, from the claim.
    run: String,
}

/// `{run}`: a run, by its ID, as the caller's. Whether it exists and is theirs is the service's to say.
struct RunPath {
    principal: service::Principal,
    run_id: RunId,
}

impl FromRequestParts<AppState> for RunPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let CurrentUser(principal) = CurrentUser::from_request_parts(parts, state).await?;
        let ApiPath(params) = ApiPath::<RunParams>::from_request_parts(parts, state).await?;
        let run_id = Uuid::parse_str(&params.run)
            .map(RunId::from_uuid)
            .map_err(|_| ServiceError::NotFound)?;
        Ok(Self { principal, run_id })
    }
}

fn work_item_dto(work: &WorkItem) -> WorkItemDto {
    WorkItemDto {
        id: *work.item.id.as_uuid(),
        key: work.item.key.to_string(),
        title: work.item.title.as_str().to_owned(),
        kind: work.item.kind.into(),
        stage: work.item.stage.into(),
        heap: ItemHeapDto {
            id: *work.heap.id.as_uuid(),
            slug: work.heap.slug.to_string(),
            name: work.heap.name.as_str().to_owned(),
            team_slug: work.team.slug.to_string(),
            team_name: work.team.name.as_str().to_owned(),
        },
        goal: work.turn.goal.into(),
        reach: work.turn.reach.into(),
        line: work.turn.line.as_ref().map(|line| line.as_str().to_owned()),
        runs: work.runs,
        failed_runs: work.turn.failed_runs,
    }
}

fn run_dto(view: &RunView) -> RunDto {
    let run = &view.run;
    let ended = run.ended.as_ref();
    RunDto {
        id: *run.id.as_uuid(),
        item: work_item_dto(&view.work),
        started_at: run.started_at.as_millis(),
        heard_at: run.heard_at.as_millis(),
        from_stage: run.from_stage.into(),
        ended_at: ended.map(|ended| ended.at.as_millis()),
        end: ended.map(|ended| ended.end.into()),
        result: ended.and_then(|ended| ended.result.as_ref().map(|result| result.as_str().to_owned())),
        links: ended
            .map(|ended| ended.links.iter().map(|link| link.as_str().to_owned()).collect())
            .unwrap_or_default(),
        failure: ended.and_then(|ended| ended.failure.clone()),
        cost_usd: run.cost_micro_usd.map(|micro| micro as f64 / 1_000_000.0),
    }
}

/// The first item of the queue over some heaps, and how many items wait for the person. Read-only: nothing is
/// claimed. An item is in the queue when it is open and not blocked, in an active heap, has an open turn that isn't
/// stuck, has no open question, and has no live run. Items come by their score in their own heap's ranking, highest
/// first, then by heap name and the item's position; every heap weighs the same for now. Any member who may edit items
/// in every given heap may ask.
#[utoipa::path(get, path = "/work/next", tag = "work", security(("session" = [])),
    params(NextQuery),
    responses(
        (status = 200, body = WorkNextDto),
        (status = 400, description = "`invalid_query` or `invalid_header`", body = Problem),
        (status = 403, description = "`forbidden`: the caller can't edit items in a given heap", body = Problem),
        (status = 404, description = "A heap outside the caller's teams", body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
        (status = 422, description = "`invalid_fields` (`field` `heaps`): none, more than 20, or one that isn't `team/heap`", body = Problem),
    ))]
async fn next_item(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiQuery(query): ApiQuery<NextQuery>,
) -> Result<Json<WorkNextDto>, ApiError> {
    let heaps = split_heaps(&query.heaps);
    let next = state.services.next_for_agent(principal, &heaps).await?;
    Ok(Json(WorkNextDto {
        item: next.item.as_ref().map(work_item_dto),
        waiting_count: next.waiting_count,
    }))
}

/// `?heaps=` as a list: split on commas and trimmed. Nothing at all is an empty list.
fn split_heaps(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return Vec::new();
    }
    raw.split(',').map(|heap| heap.trim().to_owned()).collect()
}

/// Claims the next item of the queue over some heaps and starts a run on it, as `GET /work/next` chooses, under the
/// locks of every given heap. Before choosing, it writes every run on those heaps that nobody has heard from for 30
/// minutes as failed. The run belongs to the caller; another user who uses its ID gets `404`. `201` with the run, or
/// `200` with `run` `null` when nothing waits for an agent. A claim is not audited: runs are their own record.
#[utoipa::path(post, path = "/work/runs", tag = "work", security(("session" = [])),
    request_body = ClaimRequestDto,
    responses(
        (status = 201, description = "A run started", body = ClaimDto),
        (status = 200, description = "Nothing to claim: `run` is `null`", body = ClaimDto),
        (status = 400, description = "`invalid_body`", body = Problem),
        (status = 403, description = "`forbidden`, or from inside a run `run_forbidden`", body = Problem),
        (status = 404, description = "A heap outside the caller's teams", body = Problem),
        (status = 409, description = "`heap_archived`", body = Problem),
        (status = 422, description = "`invalid_fields` (`field` `heaps`)", body = Problem),
    ))]
async fn claim_run(
    State(state): State<AppState>,
    CurrentUser(principal): CurrentUser,
    ApiJson(body): ApiJson<ClaimRequestDto>,
) -> Result<(StatusCode, Json<ClaimDto>), ApiError> {
    let claim = state.services.claim_run(principal, &body.heaps).await?;
    let status = if claim.run.is_some() {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(ClaimDto {
            run: claim.run.as_ref().map(run_dto),
            waiting_count: claim.waiting_count,
        }),
    ))
}

/// Says the run is still going: it was heard from now. A run that nobody hears from for 30 minutes, by a heartbeat or
/// a write made in it, lapses.
#[utoipa::path(post, path = "/work/runs/{run}/heartbeat", tag = "work", security(("session" = [])),
    params(RunParams),
    responses(
        (status = 204, description = "Heard"),
        (status = 404, description = "No such run, or not the caller's", body = Problem),
        (status = 409, description = "`run_over`: the run has ended or lapsed, or its turn was taken back", body = Problem),
    ))]
async fn heartbeat(State(state): State<AppState>, path: RunPath) -> Result<StatusCode, ApiError> {
    state.services.heartbeat(path.principal, path.run_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Says the run reached the turn's goal. The item is checked as it is now: `with_context` needs an open item with a
/// description and no open question; `defined` needs an open task with a description, a step that isn't dropped and no
/// open question; `done` never passes until reviews exist. On a pass the run ends as `reached`, the turn ends, the
/// item's stage becomes the goal, and `turn_reached` is recorded. A turn sent all the way opens the next turn at once,
/// recorded as `turn_opened`. A miss is `not_reached` and names the first condition
/// missed; the run stays live, so the agent can fix the item and say it again.
#[utoipa::path(post, path = "/work/runs/{run}/reached", tag = "work", security(("session" = [])),
    params(RunParams), request_body = ReachedRequestDto,
    responses(
        (status = 200, body = RunDto),
        (status = 400, description = "`invalid_body`", body = Problem),
        (status = 404, description = "No such run, or not the caller's", body = Problem),
        (status = 409, description = "`run_over`: the run has ended or lapsed, or its turn was taken back", body = Problem),
        (status = 422, description = "`not_reached`, or invalid fields: a `result` that is empty or over 2,000 characters, more than 10 `links`, or a link that isn't an absolute `http` or `https` address", body = Problem),
    ))]
async fn reached(
    State(state): State<AppState>,
    path: RunPath,
    ApiJson(body): ApiJson<ReachedRequestDto>,
) -> Result<Json<RunDto>, ApiError> {
    let result = RunResult::parse(&body.result).map_err(ServiceError::from)?;
    let links = RunLink::parse_all(&body.links).map_err(ServiceError::from)?;
    let view = state
        .services
        .reached(path.principal, path.run_id, result, links)
        .await?;
    Ok(Json(run_dto(&view)))
}

/// Ends the run if it hasn't ended, and records what it cost; a loop calls it when the agent tool exits. A run that
/// asked questions ends as `asked`; any other ends as `failed`, with `failure` or a default text, and counts toward a
/// stuck turn. A run nobody heard from for 30 minutes ends as `failed` whatever is sent. A run that has ended keeps
/// its end, and only the cost is recorded. The cost is set once: sending it for a run that has one is `run_cost_recorded`.
#[utoipa::path(post, path = "/work/runs/{run}/finish", tag = "work", security(("session" = [])),
    params(RunParams), request_body = FinishRequestDto,
    responses(
        (status = 200, body = RunDto),
        (status = 400, description = "`invalid_body`", body = Problem),
        (status = 404, description = "No such run, or not the caller's", body = Problem),
        (status = 409, description = "`run_cost_recorded`: the cost was already set", body = Problem),
        (status = 422, description = "`invalid_fields` (`field` `cost_usd`): not a number from 0 to 1,000,000", body = Problem),
    ))]
async fn finish_run(
    State(state): State<AppState>,
    path: RunPath,
    ApiJson(body): ApiJson<FinishRequestDto>,
) -> Result<Json<RunDto>, ApiError> {
    let cost = body
        .cost_usd
        .map(parse_cost_usd)
        .transpose()
        .map_err(ServiceError::from)?;
    let failure = body.failure.as_deref().and_then(cut_failure);
    let view = state
        .services
        .finish_run(path.principal, path.run_id, cost, failure)
        .await?;
    Ok(Json(run_dto(&view)))
}
