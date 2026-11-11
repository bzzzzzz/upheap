//! The agent's queue and its runs: what to work on next, claiming it, and how a run ends.
//!
//! The first item of the queue is `WorkItemDto`. Claiming it starts a run (`RunDto`), which an agent uses with the
//! `Upheap-Run` header until it reaches the turn's goal or finishes. Conversions from the service layer's views are in
//! `api`.

use domain::run::RunEnd;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{KindDto, StageDto};
use crate::turns::{GoalDto, ReachDto};
use crate::waiting::WaitingHeapDto;

/// An item an agent may work on: the item, its heap, and the turn it works to.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WorkItem)]
pub struct WorkItemDto {
    pub id: Uuid,
    /// Five characters, unique within the heap.
    #[schema(example = "k3x9a")]
    pub key: String,
    pub title: String,
    pub kind: KindDto,
    /// Where the item is now.
    pub stage: StageDto,
    pub heap: WaitingHeapDto,
    /// The stable state the turn goes to.
    pub goal: GoalDto,
    pub reach: ReachDto,
    /// What the person told the agent: plain text; `null` when they said nothing.
    pub line: Option<String>,
    /// How many runs the turn has had, ended or not. More than 0 means the work was started before.
    pub runs: u32,
    /// How many of the turn's runs in a row failed. At 3 the turn is stuck and not in the queue.
    pub failed_runs: u32,
}

/// The first item of the queue, if there is one.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = WorkNext)]
pub struct WorkNextDto {
    /// `null` when nothing in the given heaps waits for an agent.
    pub item: Option<WorkItemDto>,
    /// How many items in the given heaps wait for the person: with an open question, or with a stuck turn. An item
    /// counts once.
    pub waiting_count: u32,
}

/// How a run ended.
///
/// - `reached`: it said the turn's goal was reached, and the check passed.
/// - `asked`: it asked one or more questions, so the person has the item.
/// - `failed`: it ended without either, or nobody heard from it for 30 minutes.
/// - `taken_back`: the person took the turn back while the run was live. Not a failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = RunEnd)]
pub enum RunEndDto {
    Reached,
    Asked,
    Failed,
    TakenBack,
    /// An end this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<RunEnd> for RunEndDto {
    fn from(end: RunEnd) -> Self {
        match end {
            RunEnd::Reached => Self::Reached,
            RunEnd::Asked => Self::Asked,
            RunEnd::Failed => Self::Failed,
            RunEnd::TakenBack => Self::TakenBack,
        }
    }
}

/// A run: one go an agent has at an item's turn.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Run)]
pub struct RunDto {
    pub id: Uuid,
    /// The item the run holds.
    pub item: WorkItemDto,
    /// UTC milliseconds.
    pub started_at: i64,
    /// UTC milliseconds. When the run was last heard from: a heartbeat or a write made in it. After 30 minutes without
    /// either, the run lapses.
    pub heard_at: i64,
    /// The item's stage when the run started.
    pub from_stage: StageDto,
    /// UTC milliseconds; `null` while the run has not ended.
    pub ended_at: Option<i64>,
    /// `null` while the run has not ended.
    pub end: Option<RunEndDto>,
    /// What the run said it reached; `null` unless it ended as `reached`.
    pub result: Option<String>,
    pub links: Vec<String>,
    /// Why the run failed; `null` unless it ended as `failed`.
    pub failure: Option<String>,
    /// What the run cost in US dollars; `null` until `finish` sets it.
    pub cost_usd: Option<f64>,
}

/// A claim: the run started on the first item of the queue.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Claim)]
pub struct ClaimDto {
    /// `null` when nothing in the given heaps waits for an agent.
    pub run: Option<RunDto>,
    /// As in `WorkNext`.
    pub waiting_count: u32,
}

/// Claims the next item of the queue over some heaps.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ClaimRequest)]
pub struct ClaimRequestDto {
    /// One to 20 heaps, each `team/heap` by slug. A heap named twice counts once.
    pub heaps: Vec<String>,
}

/// Says a run reached the turn's goal.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ReachedRequest)]
pub struct ReachedRequestDto {
    /// What the run did, in a line or two: plain text, 1 to 2,000 characters.
    pub result: String,
    /// Up to 10 absolute `http` or `https` addresses of up to 2,000 characters.
    #[serde(default)]
    pub links: Vec<String>,
}

/// Ends a run that has not ended, and records what it cost.
#[derive(Debug, Default, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = FinishRequest)]
pub struct FinishRequestDto {
    /// What the run cost in US dollars: from 0 to 1,000,000. Set once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Why the run failed, for a run that ends without reaching the goal or asking: plain text, cut to 2,000
    /// characters. Ignored for a run that ended another way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<String>,
}
