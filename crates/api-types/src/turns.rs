//! Turns: an item handed to an agent. The request type of the send endpoint, and the open turn an item carries. Both
//! endpoints answer with the whole item (`ItemDto`).

use domain::ValidationError;
use domain::turn::{Goal, Reach, Turn, TurnLine, TurnSource};
use domain::validation::Field;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common::UserRefDto;

/// How far a turn is meant to go.
///
/// - `one`: to the next stable state (`goal`), then back to the person.
/// - `all_the_way`: on through the stable states until the work is done. Until agents run turns it behaves as `one`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Reach)]
pub enum ReachDto {
    One,
    AllTheWay,
    /// A reach this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<Reach> for ReachDto {
    fn from(reach: Reach) -> Self {
        match reach {
            Reach::One => Self::One,
            Reach::AllTheWay => Self::AllTheWay,
        }
    }
}

impl TryFrom<ReachDto> for Reach {
    type Error = ValidationError;

    /// Fails on a reach this server doesn't know, which a request can carry.
    fn try_from(reach: ReachDto) -> Result<Self, Self::Error> {
        match reach {
            ReachDto::One => Ok(Self::One),
            ReachDto::AllTheWay => Ok(Self::AllTheWay),
            ReachDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::Reach }),
        }
    }
}

/// The stable state a turn goes to, fixed when it opens: a note as written goes to `with_context`, a note with
/// context and a task not yet defined go to `defined`, and a defined task goes to `done`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Goal)]
pub enum GoalDto {
    WithContext,
    Defined,
    Done,
    /// A goal this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<Goal> for GoalDto {
    fn from(goal: Goal) -> Self {
        match goal {
            Goal::WithContext => Self::WithContext,
            Goal::Defined => Self::Defined,
            Goal::Done => Self::Done,
        }
    }
}

/// How a turn came to be opened.
///
/// - `go_ahead`: a person sent the item.
/// - `prepare`: the heap is set so agents prepare new notes, and this one was added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = TurnSource)]
pub enum TurnSourceDto {
    GoAhead,
    Prepare,
    /// A source this client doesn't know: one the server added later. Only read, never sent, so it isn't in the
    /// API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<TurnSource> for TurnSourceDto {
    fn from(source: TurnSource) -> Self {
        match source {
            TurnSource::GoAhead => Self::GoAhead,
            TurnSource::Prepare => Self::Prepare,
        }
    }
}

/// An item's open turn: the item has been sent to an agent and not taken back.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Turn)]
pub struct TurnDto {
    pub reach: ReachDto,
    pub goal: GoalDto,
    /// What the person told the agent: plain text, at most 2,000 characters; `null` when they said nothing.
    pub line: Option<String>,
    pub source: TurnSourceDto,
    /// UTC milliseconds.
    pub opened_at: i64,
    /// Who sent the item, or added the note in a heap that prepares them. `null` when it can't be traced.
    pub opened_by: Option<UserRefDto>,
}

impl TurnDto {
    /// The open `turn`, with the user who opened it if the caller knows them.
    pub fn new(turn: &Turn, opened_by: Option<UserRefDto>) -> Self {
        Self {
            reach: turn.reach.into(),
            goal: turn.goal.into(),
            line: turn.line.as_ref().map(|line| line.as_str().to_owned()),
            source: turn.source.into(),
            opened_at: turn.opened_at.as_millis(),
            opened_by,
        }
    }
}

/// Sends an item to an agent.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = SendToAgentRequest)]
pub struct SendToAgentDto {
    pub reach: ReachDto,
    /// Something to tell the agent: plain text, at most 2,000 characters, line breaks allowed; blank or `null` means
    /// none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<String>,
}

impl SendToAgentDto {
    /// The reach and the line. Fails on a reach this server doesn't know, and on a line that is too long.
    pub fn to_turn_fields(&self) -> Result<(Reach, Option<TurnLine>), ValidationError> {
        let line = self.line.as_deref().map(TurnLine::parse).transpose()?.flatten();
        Ok((Reach::try_from(self.reach)?, line))
    }
}
