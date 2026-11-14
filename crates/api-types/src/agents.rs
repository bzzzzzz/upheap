//! The Agents page: what agents finished that waits for the signed-in user (`GET /me/decisions`), and the history of
//! the runs that ended as reached (`GET /me/results`).

use domain::run::RunReached;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::turns::GoalDto;
use crate::waiting::{ItemHeapDto, WaitingItemDto};

/// What waits for the caller's decision: items an agent brought to a stable state, in their active heaps, most
/// important first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Decisions)]
pub struct DecisionsDto {
    /// How many rows wait. At most 500 are in `rows`; this is the true total.
    pub count: u32,
    /// Ordered by the item's score in its own heap times your priority for that heap, highest first; ties by heap
    /// name, then the item's position in its heap. A note that became other items takes the best score of what was
    /// made from it that is ranked, or 0.
    pub rows: Vec<DecisionRowDto>,
}

/// An item an agent brought to a stable state, which waits for the caller.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = DecisionRow)]
pub struct DecisionRowDto {
    /// The item, at its stage now. For a note that became other items, the note. It has no open turn.
    pub item: WaitingItemDto,
    pub heap: ItemHeapDto,
    /// How long ago the agent's turn ended, in calendar days in the client's time zone: "today", "yesterday",
    /// "4 days ago".
    pub arrived: String,
    /// What a note that became other items became, in words: "made 3 tasks". `null` on any other row.
    pub became_label: Option<String>,
}

/// What a run reached when it ended as reached. More can join.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = RunReached)]
pub enum RunReachedDto {
    /// The item is a note with context.
    WithContext,
    /// The item is a defined task.
    Defined,
    /// The note became other items or heaps, or a step.
    Became,
    /// A state this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<RunReached> for RunReachedDto {
    fn from(reached: RunReached) -> Self {
        match reached {
            RunReached::WithContext => Self::WithContext,
            RunReached::Defined => Self::Defined,
            RunReached::Became => Self::Became,
        }
    }
}

/// The page of the history to read.
#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ResultsQuery {
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
}

/// A page of the history: the runs that ended as reached in the last 30 days, newest first.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Results)]
pub struct ResultsDto {
    /// Up to 50 rows.
    pub rows: Vec<ResultRowDto>,
    /// Where the next page starts; `null` when there are no more runs in the last 30 days.
    pub next_cursor: Option<String>,
}

/// One run that ended as reached.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ResultRow)]
pub struct ResultRowDto {
    pub run_id: Uuid,
    /// UTC milliseconds: when the run ended.
    pub ended_at: i64,
    /// The local calendar date the run ended on in the client's time zone, `YYYY-MM-DD`: the key rows are grouped by.
    #[schema(example = "2026-11-12")]
    pub day: String,
    /// The date as a label: "today", "yesterday", "12 Nov".
    pub day_label: String,
    /// The local time the run ended at in the client's time zone, "HH:MM" on a 24-hour clock.
    #[schema(example = "14:02")]
    pub time: String,
    /// The item at its stage now.
    pub item: WaitingItemDto,
    pub heap: ItemHeapDto,
    /// The goal of the run's turn.
    pub goal: GoalDto,
    /// What the run reached; `null` for a run that ended before this was recorded.
    pub reached: Option<RunReachedDto>,
    /// What the run reached, in words: "with context", "defined", "defined (skipped with context)" when it went past
    /// the goal of its turn, "became". `null` when `reached` is.
    pub reached_label: Option<String>,
    /// What the run said it reached.
    pub result: String,
    pub links: Vec<RunLinkDto>,
}

/// A link a run left.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RunLink)]
pub struct RunLinkDto {
    /// The address.
    pub url: String,
    /// What to show for it: "pull request" for a pull or merge request, else the address without `https://`.
    pub label: String,
    /// Whether it is a pull or merge request.
    pub pull_request: bool,
}

impl RunLinkDto {
    pub fn new(url: &str) -> Self {
        let path = url
            .strip_prefix("https://")
            .or_else(|| url.strip_prefix("http://"))
            .map_or(url, |rest| rest.find('/').map_or("", |at| &rest[at..]));
        let pull_request = path.contains("/pull/") || path.contains("/merge_requests/");
        let label = if pull_request {
            "pull request".to_owned()
        } else {
            url.strip_prefix("https://").unwrap_or(url).to_owned()
        };
        Self {
            url: url.to_owned(),
            label,
            pull_request,
        }
    }
}
