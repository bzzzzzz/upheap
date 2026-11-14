//! The Data page's figures (`GET /teams/{team}/data`).

use domain::data::{ClosedCounts, Period, RunCounts};
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};

/// How far back to look.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = DataDays)]
pub enum DataDaysDto {
    /// The last 7 days, a bucket per day.
    #[serde(rename = "7")]
    Days7,
    /// The last 30 days, a bucket per day. The default.
    #[default]
    #[serde(rename = "30")]
    Days30,
    /// The last 13 weeks, from Monday, a bucket per week.
    #[serde(rename = "90")]
    Days90,
}

impl From<DataDaysDto> for Period {
    fn from(days: DataDaysDto) -> Self {
        match days {
            DataDaysDto::Days7 => Self::Days7,
            DataDaysDto::Days30 => Self::Days30,
            DataDaysDto::Days90 => Self::Days90,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct DataQueryDto {
    /// Only this heap of the team, by its slug, archived or not. Omit for all the team's heaps.
    pub heap: Option<String>,
    /// How far back to look. Defaults to `30`.
    pub days: Option<DataDaysDto>,
}

/// How many items closed each way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = DataClosed)]
pub struct DataClosedDto {
    pub done: u32,
    pub wont_do: u32,
    pub became: u32,
}

impl From<ClosedCounts> for DataClosedDto {
    fn from(counts: ClosedCounts) -> Self {
        Self {
            done: counts.done,
            wont_do: counts.wont_do,
            became: counts.became,
        }
    }
}

/// How many runs ended each way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[schema(as = DataRuns)]
pub struct DataRunsDto {
    pub reached: u32,
    pub asked: u32,
    pub failed: u32,
    /// Taken back, handed back, or not ended yet.
    pub other: u32,
}

impl From<RunCounts> for DataRunsDto {
    fn from(counts: RunCounts) -> Self {
        Self {
            reached: counts.reached,
            asked: counts.asked,
            failed: counts.failed,
            other: counts.other,
        }
    }
}

/// What happened in a bucket, or in the whole period.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = DataFigures)]
pub struct DataFiguresDto {
    /// What the runs that started cost, in US dollars. A run with no cost adds nothing.
    pub spend_usd: f64,
    /// Runs that ended and reported no cost.
    pub runs_without_cost: u32,
    /// Items created.
    pub added: u32,
    /// Items closed, on their latest close.
    pub closed: DataClosedDto,
    /// Runs started, by how they ended.
    pub runs: DataRunsDto,
}

/// One day, or one week for `90`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = DataBucket)]
pub struct DataBucketDto {
    /// Ready to show: "14 Nov" for a day, "Week of 10 Nov" for a week.
    pub label: String,
    /// Whether the bucket is still going: today, or this week so far. Only the last is.
    pub partial: bool,
    #[serde(flatten)]
    pub figures: DataFiguresDto,
}

/// What agents cost and how items and runs came and went, over the period.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, ToSchema)]
#[schema(as = TeamData)]
pub struct TeamDataDto {
    /// The figures of the whole period.
    pub totals: DataFiguresDto,
    /// Every day or week of the period, oldest first, zeros included.
    pub buckets: Vec<DataBucketDto>,
}
