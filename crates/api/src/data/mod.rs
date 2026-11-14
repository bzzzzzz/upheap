//! The Data page's figures for a team, or for one of its heaps.

use api_types::data::{DataBucketDto, DataFiguresDto, DataQueryDto, TeamDataDto};
use axum::Json;
use axum::extract::State;
use domain::data::{Bucket, DataFigures, Figures};
use domain::handle::Handle;
use domain::validation::Field;
use service::ServiceError;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::dto::micro_usd_to_usd;
use crate::error::Problem;
use crate::extract::{ApiQuery, ClientTimeZone, TimeZoneHeader};
use crate::paths::{TeamParams, TeamPath};
use crate::{ApiError, AppState};

pub(crate) fn routes() -> OpenApiRouter<AppState> {
    OpenApiRouter::new().routes(routes!(get_team_data))
}

fn to_figures_dto(figures: Figures) -> DataFiguresDto {
    DataFiguresDto {
        spend_usd: micro_usd_to_usd(figures.spend_micro_usd),
        runs_without_cost: figures.runs_without_cost,
        added: figures.added,
        closed: figures.closed.into(),
        runs: figures.runs.into(),
    }
}

fn to_bucket_dto(bucket: Bucket) -> DataBucketDto {
    DataBucketDto {
        label: bucket.label,
        partial: bucket.partial,
        figures: to_figures_dto(bucket.figures),
    }
}

fn to_team_data_dto(data: DataFigures) -> TeamDataDto {
    TeamDataDto {
        totals: to_figures_dto(data.totals),
        buckets: data.buckets.into_iter().map(to_bucket_dto).collect(),
    }
}

/// What agents spent, how many items were added and closed, and how agent runs ended, by day (by week for 90 days),
/// for all of a team's heaps or one of them. Any member of the team may read it.
#[utoipa::path(get, path = "/teams/{team}/data", tag = "data", security(("session" = [])),
    params(TeamParams, DataQueryDto, TimeZoneHeader),
    responses((status = 200, body = TeamDataDto), (status = 404, body = Problem)))]
async fn get_team_data(
    State(state): State<AppState>,
    path: TeamPath,
    ClientTimeZone(tz): ClientTimeZone,
    ApiQuery(query): ApiQuery<DataQueryDto>,
) -> Result<Json<TeamDataDto>, ApiError> {
    // A slug that isn't well formed names no heap, so it is `404` like one the team doesn't have.
    let heap = query
        .heap
        .as_deref()
        .map(|slug| Handle::parse(slug, Field::Slug).map_err(|_| ServiceError::NotFound))
        .transpose()?;
    let data = state
        .services
        .load_team_data(
            path.principal,
            path.team_id,
            heap.as_ref(),
            query.days.unwrap_or_default().into(),
            &tz,
        )
        .await?;
    Ok(Json(to_team_data_dto(data)))
}
