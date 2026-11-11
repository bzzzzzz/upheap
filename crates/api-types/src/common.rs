//! Response and request types shared by several resources, and their conversions from domain types.
//!
//! These are separate from domain types even where fields match, because the API changes for different reasons.

use domain::due::ClosedGroup;
use domain::heap::Heap;
use domain::item::{Due, ItemSummary, Kind, Need, Size, Stage, StateKind};
use domain::permission::{ItemAction, QuestionAction, StepAction};
use domain::question::{QuestionKind, QuestionState};
use domain::ranking::{Contributor, Factor, Gap, HeldBack};
use domain::step::{Step, StepProgress, StepState};
use domain::tag::Tag;
use domain::team::Role;
use domain::user::User;
use domain::validation::Field;
use domain::{Timestamp, ValidationError};
use jiff::tz::TimeZone;
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::turns::TurnDto;

/// Effort or impact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = Size)]
pub enum SizeDto {
    S,
    M,
    L,
    Xl,
}

impl From<Size> for SizeDto {
    fn from(size: Size) -> Self {
        match size {
            Size::S => Self::S,
            Size::M => Self::M,
            Size::L => Self::L,
            Size::Xl => Self::Xl,
        }
    }
}

impl From<SizeDto> for Size {
    fn from(size: SizeDto) -> Self {
        match size {
            SizeDto::S => Self::S,
            SizeDto::M => Self::M,
            SizeDto::L => Self::L,
            SizeDto::Xl => Self::Xl,
        }
    }
}

/// How much an item is needed: what happens if it never gets done, from least to most.
///
/// - `idea`: kept so it isn't forgotten; nothing is lost if it never happens.
/// - `nice_to_have`: wanted, but nobody is waiting for it.
/// - `expected`: someone is counting on it; skipping it lets them down.
/// - `critical`: it has to happen; skipping it does real damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Need)]
pub enum NeedDto {
    Idea,
    NiceToHave,
    Expected,
    Critical,
}

impl From<Need> for NeedDto {
    fn from(need: Need) -> Self {
        match need {
            Need::Idea => Self::Idea,
            Need::NiceToHave => Self::NiceToHave,
            Need::Expected => Self::Expected,
            Need::Critical => Self::Critical,
        }
    }
}

impl From<NeedDto> for Need {
    fn from(need: NeedDto) -> Self {
        match need {
            NeedDto::Idea => Self::Idea,
            NeedDto::NiceToHave => Self::NiceToHave,
            NeedDto::Expected => Self::Expected,
            NeedDto::Critical => Self::Critical,
        }
    }
}

/// What an item is. A note becomes a task in place; a task never goes back to a note.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Kind)]
pub enum KindDto {
    Note,
    Task,
    /// A kind this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<Kind> for KindDto {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Note => Self::Note,
            Kind::Task => Self::Task,
        }
    }
}

impl TryFrom<KindDto> for Kind {
    type Error = ValidationError;

    /// Fails on a kind this server doesn't know, which a request can carry.
    fn try_from(kind: KindDto) -> Result<Self, Self::Error> {
        match kind {
            KindDto::Note => Ok(Self::Note),
            KindDto::Task => Ok(Self::Task),
            KindDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::Kind }),
        }
    }
}

/// How far an item has been prepared, from least to most. Set by hand.
///
/// - `as_written`: just as it was put down.
/// - `with_context`: background has been added.
/// - `defined`: clear enough to start on. Only a task can be defined.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Stage)]
pub enum StageDto {
    AsWritten,
    WithContext,
    Defined,
    /// A stage this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<Stage> for StageDto {
    fn from(stage: Stage) -> Self {
        match stage {
            Stage::AsWritten => Self::AsWritten,
            Stage::WithContext => Self::WithContext,
            Stage::Defined => Self::Defined,
        }
    }
}

impl TryFrom<StageDto> for Stage {
    type Error = ValidationError;

    /// Fails on a stage this server doesn't know, which a request can carry.
    fn try_from(stage: StageDto) -> Result<Self, Self::Error> {
        match stage {
            StageDto::AsWritten => Ok(Self::AsWritten),
            StageDto::WithContext => Ok(Self::WithContext),
            StageDto::Defined => Ok(Self::Defined),
            StageDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::Stage }),
        }
    }
}

/// Item state. `done` and `wont_do` are closed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ItemState)]
pub enum StateDto {
    Open,
    InProgress,
    Blocked,
    Done,
    WontDo,
}

impl From<StateKind> for StateDto {
    fn from(state: StateKind) -> Self {
        match state {
            StateKind::Open => Self::Open,
            StateKind::InProgress => Self::InProgress,
            StateKind::Blocked => Self::Blocked,
            StateKind::Done => Self::Done,
            StateKind::WontDo => Self::WontDo,
        }
    }
}

impl From<StateDto> for StateKind {
    fn from(state: StateDto) -> Self {
        match state {
            StateDto::Open => Self::Open,
            StateDto::InProgress => Self::InProgress,
            StateDto::Blocked => Self::Blocked,
            StateDto::Done => Self::Done,
            StateDto::WontDo => Self::WontDo,
        }
    }
}

/// Which part of the closed list a closed item belongs in, in the request's timezone: `today` for the same calendar
/// day, `this_week` for 1 to 6 days before, and `earlier` for anything older.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ClosedGroup)]
pub enum ClosedGroupDto {
    Today,
    ThisWeek,
    Earlier,
    /// A group this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<ClosedGroup> for ClosedGroupDto {
    fn from(group: ClosedGroup) -> Self {
        match group {
            ClosedGroup::Today => Self::Today,
            ClosedGroup::ThisWeek => Self::ThisWeek,
            ClosedGroup::Earlier => Self::Earlier,
        }
    }
}

/// An action the caller can take on an item. Clients show controls only for these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ItemAction)]
pub enum ItemActionDto {
    Edit,
    ChangeState,
    Reopen,
    /// Make a note a task. Offered for an open note the caller may edit.
    MakeTask,
    /// Add a step to a task. Offered for an open task the caller may edit.
    AddStep,
    /// Ask a question or make a request on the item. Offered for an open item the caller may edit.
    AskQuestion,
    /// Send the item to an agent, on `POST …/turn`. Offered for an open item the caller may edit that no agent has.
    SendToAgent,
    /// Take the item back from the agent, on `DELETE …/turn`. Offered for an open item the caller may edit that has
    /// an open turn.
    TakeBack,
}

impl From<ItemAction> for ItemActionDto {
    fn from(action: ItemAction) -> Self {
        match action {
            ItemAction::Edit => Self::Edit,
            ItemAction::ChangeState => Self::ChangeState,
            ItemAction::Reopen => Self::Reopen,
            ItemAction::MakeTask => Self::MakeTask,
            ItemAction::AddStep => Self::AddStep,
            ItemAction::AskQuestion => Self::AskQuestion,
            ItemAction::SendToAgent => Self::SendToAgent,
            ItemAction::TakeBack => Self::TakeBack,
        }
    }
}

/// Where a step stands. A step can move from any state to any other.
///
/// - `open`: still to do.
/// - `ticked`: done.
/// - `dropped`: no longer wanted. The step stays, shown as dropped, and doesn't count in the item's progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = StepState)]
pub enum StepStateDto {
    Open,
    Ticked,
    Dropped,
    /// A state this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<StepState> for StepStateDto {
    fn from(state: StepState) -> Self {
        match state {
            StepState::Open => Self::Open,
            StepState::Ticked => Self::Ticked,
            StepState::Dropped => Self::Dropped,
        }
    }
}

impl TryFrom<StepStateDto> for StepState {
    type Error = ValidationError;

    /// Fails on a state this server doesn't know, which a request can carry.
    fn try_from(state: StepStateDto) -> Result<Self, Self::Error> {
        match state {
            StepStateDto::Open => Ok(Self::Open),
            StepStateDto::Ticked => Ok(Self::Ticked),
            StepStateDto::Dropped => Ok(Self::Dropped),
            StepStateDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::State }),
        }
    }
}

/// An action the caller can take on a step. Clients show controls only for these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = StepAction)]
pub enum StepActionDto {
    /// Change the step's title, context or state.
    Edit,
}

impl From<StepAction> for StepActionDto {
    fn from(action: StepAction) -> Self {
        match action {
            StepAction::Edit => Self::Edit,
        }
    }
}

/// One step of a task.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Step)]
pub struct StepDto {
    pub id: Uuid,
    /// The step's number within its item, given when it was added and never changed or reused. The step is named
    /// by its item's key and this number, as `k3x9a.3`, and its path is `…/items/{key}/steps/{number}`.
    pub number: u32,
    pub title: String,
    /// Plain text; `null` when there is none.
    pub context: Option<String>,
    pub state: StepStateDto,
    /// Send back in `If-Match` when editing the step. A step has its own version; writing it never changes the
    /// item's.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<StepActionDto>,
}

impl StepDto {
    /// `step` with the `actions` the caller may take on it.
    pub fn new(step: &Step, actions: &[StepAction]) -> Self {
        Self {
            id: *step.id.as_uuid(),
            number: step.number.get(),
            title: step.title.as_str().to_owned(),
            context: step.context.as_ref().map(|context| context.as_str().to_owned()),
            state: step.state.into(),
            version: step.version.get(),
            allowed_actions: actions.iter().copied().map(Into::into).collect(),
        }
    }
}

/// What is being asked.
///
/// - `question`: free text, or a pick among `choices`.
/// - `request`: something to do, replied to with Done (pick 0) or Can't be done (pick 1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = QuestionKind)]
pub enum QuestionKindDto {
    Question,
    Request,
    /// A kind this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<QuestionKind> for QuestionKindDto {
    fn from(kind: QuestionKind) -> Self {
        match kind {
            QuestionKind::Question => Self::Question,
            QuestionKind::Request => Self::Request,
        }
    }
}

impl TryFrom<QuestionKindDto> for QuestionKind {
    type Error = ValidationError;

    /// Fails on a kind this server doesn't know, which a request can carry.
    fn try_from(kind: QuestionKindDto) -> Result<Self, Self::Error> {
        match kind {
            QuestionKindDto::Question => Ok(Self::Question),
            QuestionKindDto::Request => Ok(Self::Request),
            QuestionKindDto::Unknown => Err(ValidationError::ValueUnknown { field: Field::Kind }),
        }
    }
}

/// Whether a question still waits for a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = QuestionState)]
pub enum QuestionStateDto {
    Open,
    Answered,
    /// Closing the item took the question back, unanswered. Reopening the item doesn't bring it back, and it takes
    /// no reply.
    Withdrawn,
    /// A state this client doesn't know: one the server added later. Only read, never sent, so it isn't in the API's
    /// schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<QuestionState> for QuestionStateDto {
    fn from(state: QuestionState) -> Self {
        match state {
            QuestionState::Open => Self::Open,
            QuestionState::Answered => Self::Answered,
            QuestionState::Withdrawn => Self::Withdrawn,
        }
    }
}

/// An action the caller can take on a question. Clients show controls only for these.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = QuestionAction)]
pub enum QuestionActionDto {
    /// Reply to the question. Offered while it is open, on an open item the caller may edit.
    Reply,
}

impl From<QuestionAction> for QuestionActionDto {
    fn from(action: QuestionAction) -> Self {
        match action {
            QuestionAction::Reply => Self::Reply,
        }
    }
}

/// The reply to a question.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = QuestionReply)]
pub struct QuestionReplyDto {
    /// Indices into the question's `choices`, ascending. Empty for a question without choices.
    pub picked: Vec<u32>,
    /// Plain text; `null` when there is none.
    pub comment: Option<String>,
    /// Who replied. `null` when it can't be traced.
    pub replied_by: Option<UserRefDto>,
    /// UTC milliseconds.
    pub replied_at: i64,
    /// When it was replied to, ready to show: "today", "2 days ago".
    pub replied_label: String,
}

/// One question or request on an item.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Question)]
pub struct QuestionDto {
    /// The question's number within its item, given when it was asked and never changed or reused. The question is
    /// named by its item's key and this number, as `k3x9a.q2`, and its path is `…/items/{key}/questions/{number}`.
    pub number: u32,
    pub kind: QuestionKindDto,
    /// The short version: one line.
    pub text: String,
    /// The long version, plain text; `null` when there is none.
    pub details: Option<String>,
    /// The labels to pick from, in order; empty when the reply is free text. A request always has `Done` and
    /// `Can't be done`.
    pub choices: Vec<String>,
    /// Whether any number of choices may be picked, not just one.
    pub many: bool,
    /// The number of the item's step this is about, which may have been dropped since.
    pub step: Option<u32>,
    pub state: QuestionStateDto,
    /// Who asked. `null` when it can't be traced.
    pub asked_by: Option<UserRefDto>,
    /// UTC milliseconds.
    pub asked_at: i64,
    /// When it was asked, ready to show: "today", "2 days ago".
    pub asked_label: String,
    /// `null` unless the question is answered.
    pub reply: Option<QuestionReplyDto>,
    /// When closing the item withdrew the question, in UTC milliseconds; `null` unless the question is withdrawn.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub withdrawn_at: Option<i64>,
    /// Who closed the item and so withdrew the question. `null` unless the question is withdrawn, or when it can't be
    /// traced.
    #[serde(default)]
    #[schema(required = true)]
    pub withdrawn_by: Option<UserRefDto>,
    /// Send back in `If-Match` when replying. A question has its own version; writing it never changes the item's.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<QuestionActionDto>,
}

/// How far a task's steps have got.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = StepProgress)]
pub struct StepProgressDto {
    pub ticked: u32,
    /// Open and ticked steps. Dropped ones don't count.
    pub total: u32,
}

impl From<StepProgress> for StepProgressDto {
    fn from(progress: StepProgress) -> Self {
        Self {
            ticked: progress.ticked,
            total: progress.total,
        }
    }
}

/// A tag on an item: enough to show it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemTag)]
pub struct ItemTagDto {
    pub id: Uuid,
    pub name: String,
}

impl From<&Tag> for ItemTagDto {
    fn from(tag: &Tag) -> Self {
        Self {
            id: *tag.id.as_uuid(),
            name: tag.name.as_str().to_owned(),
        }
    }
}

/// An item's value for one custom parameter.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemParameter)]
pub struct ItemParameterDto {
    pub parameter_id: Uuid,
    pub name: String,
    /// Only closed items, and items reopened since, list archived parameters, and only when they have a value.
    pub parameter_archived: bool,
    /// `null` when unset, which ranks as the parameter's lowest-priority option.
    pub option: Option<ItemOptionDto>,
}

/// The option an item has for a parameter.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemOption)]
pub struct ItemOptionDto {
    pub id: Uuid,
    pub label: String,
    /// Closed items keep archived options; open items switch to the replacement.
    pub archived: bool,
}

/// Another item, named where this one refers to it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemRef)]
pub struct ItemRefDto {
    pub id: Uuid,
    /// Unique within the heap; the item's path is `…/items/{key}`.
    pub key: String,
    pub title: String,
    pub state: StateDto,
}

impl From<ItemSummary> for ItemRefDto {
    fn from(item: ItemSummary) -> Self {
        Self {
            id: *item.id.as_uuid(),
            key: item.key.to_string(),
            title: item.title.as_str().to_owned(),
            state: item.state.kind().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Role)]
pub enum RoleDto {
    Admin,
    Member,
}

impl From<RoleDto> for Role {
    fn from(role: RoleDto) -> Self {
        match role {
            RoleDto::Admin => Self::Admin,
            RoleDto::Member => Self::Member,
        }
    }
}

impl From<Role> for RoleDto {
    fn from(role: Role) -> Self {
        match role {
            Role::Admin => Self::Admin,
            Role::Member => Self::Member,
        }
    }
}

/// A user, named where something refers to them: who made an audit entry, who added an item.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = UserRef)]
pub struct UserRefDto {
    pub id: Uuid,
    /// Their display name now.
    pub display_name: String,
}

impl From<&User> for UserRefDto {
    fn from(user: &User) -> Self {
        Self {
            id: *user.id.as_uuid(),
            display_name: user.display_name.as_str().to_owned(),
        }
    }
}

/// A due time, ready to show.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Due)]
pub struct DueDto {
    /// UTC milliseconds.
    pub at: i64,
    /// The calendar date in the request's timezone, `YYYY-MM-DD`, for date inputs.
    pub date: String,
    /// Exact deadline (`true`) or loose due time (`false`).
    pub firm: bool,
    /// "in a few weeks" for loose due times; "tomorrow" or "12 Mar" for deadlines, in the request's timezone.
    pub label: String,
    /// Only deadlines become overdue.
    pub overdue: bool,
}

/// A due time as entered: a calendar date, stored as the end of that day in the request's timezone.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = DueInput)]
pub struct DueInputDto {
    /// `YYYY-MM-DD`.
    #[schema(example = "2027-03-12")]
    pub date: String,
    pub firm: bool,
}

impl DueInputDto {
    /// The due time this input stands for in `tz`. Fails with `DueDateInvalid` for a date that isn't `YYYY-MM-DD`
    /// or is out of range.
    pub fn to_due(&self, tz: &TimeZone) -> Result<Due, ValidationError> {
        let date: jiff::civil::Date = self.date.parse().map_err(|_| ValidationError::DueDateInvalid)?;
        let at = Timestamp::at_end_of_day(date, tz).ok_or(ValidationError::DueDateInvalid)?;
        Ok(Due { at, firm: self.firm })
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Item)]
pub struct ItemDto {
    pub id: Uuid,
    /// Five characters, unique within the heap and never changed; the item's path is `…/items/{key}`.
    #[schema(example = "k3x9a")]
    pub key: String,
    pub heap_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub kind: KindDto,
    /// How far the item has been prepared. `null` when the item is closed, and always in a build without the stage
    /// (the static, single-user one).
    pub stage: Option<StageDto>,
    /// The stages the caller may set now, in table order: all three on an open task, the first two on an open note.
    /// Empty when the item is closed, when the caller can't edit it, and in a build without the stage.
    #[serde(deserialize_with = "crate::common::known_values")]
    pub stage_choices: Vec<StageDto>,
    pub state: StateDto,
    /// The item this one waits for. Set only on blocked items, and only when a blocker was picked. It can be
    /// closed: finishing a blocker doesn't unblock what waits for it.
    pub blocked_by: Option<ItemRefDto>,
    pub need: NeedDto,
    /// `null` when unset.
    pub effort: Option<SizeDto>,
    /// `null` when unset.
    pub impact: Option<SizeDto>,
    pub due: Option<DueDto>,
    /// By name.
    pub tags: Vec<ItemTagDto>,
    /// A value, or `null`, for every parameter of the heap in use, and the item's values of archived parameters,
    /// by parameter name.
    pub parameters: Vec<ItemParameterDto>,
    /// UTC milliseconds.
    pub created_at: i64,
    /// How long ago it was added, ready to show: "today", "yesterday", "4 days ago", "3 weeks ago", in the request's
    /// timezone.
    pub created_label: String,
    /// Who added it. `null` only for an item added before creators were recorded, whose creation couldn't be traced.
    pub created_by: Option<UserRefDto>,
    /// UTC milliseconds; set exactly when the item is closed.
    pub closed_at: Option<i64>,
    /// When it was closed, ready to show ("yesterday", "3 Nov").
    pub closed_label: Option<String>,
    /// Who closed it. `null` when the item is open, or was closed before who closed it was recorded.
    pub closed_by: Option<UserRefDto>,
    /// Which part of the closed list it belongs in. `null` when the item is open. Clients group consecutive items
    /// with the same group.
    pub closed_group: Option<ClosedGroupDto>,
    /// A task's steps in order, dropped ones included; empty for a note. `null` in the ranked list and the closed
    /// list, which carry only `step_progress`: read the item for its steps.
    pub steps: Option<Vec<StepDto>>,
    /// Ticked of open and ticked steps. `null` when the item has no open or ticked step. On every response,
    /// lists included.
    pub step_progress: Option<StepProgressDto>,
    /// The item's questions and requests in number order, answered ones included. `null` in the ranked list and the
    /// closed list: read the item for its questions.
    pub questions: Option<Vec<QuestionDto>>,
    /// Whether the item is open and has an open question, in an active heap: whoever asked it, it waits for the
    /// caller, and `GET /me/waiting` lists it. On every response, lists included; always `false` on a closed item
    /// and in an archived heap.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub waits_for_you: bool,
    /// The open turn: the item was sent to an agent and not taken back. `null` when there is none, and on a closed
    /// item. Taking the item back, starting it, closing it and archiving its heap end the turn.
    #[serde(default)]
    #[schema(required = true)]
    pub turn: Option<TurnDto>,
    /// Whether an agent has the item: it is open, its heap is active, it has an open turn, and it doesn't wait for
    /// the caller. Who has an item never changes its place in the list. On every response, lists included; always
    /// `false` on a closed item and in an archived heap.
    #[serde(default)]
    #[schema(required = true)]
    pub with_agent: bool,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    #[serde(deserialize_with = "crate::common::known_values")]
    pub allowed_actions: Vec<ItemActionDto>,
}

/// One labelled reason for an item's position.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = Contributor)]
pub struct ContributorDto {
    pub factor: FactorDto,
    pub label: String,
    /// Share of the item's priority, 1 to 100.
    pub share_percent: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Factor)]
pub enum FactorDto {
    /// How much the item is needed.
    Need,
    Age,
    Deadline,
    Value,
    /// A custom parameter.
    Parameter,
    /// Priority taken on from an item this one blocks.
    Blocks,
    /// A factor this client doesn't know: one the server added later. Clients show the contributor's `label`, which
    /// the server writes, so they needn't know every factor. Only read, never sent, so it isn't in the API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<Contributor> for ContributorDto {
    fn from(contributor: Contributor) -> Self {
        let factor = match contributor.factor {
            Factor::Need => FactorDto::Need,
            Factor::Age => FactorDto::Age,
            Factor::Deadline => FactorDto::Deadline,
            Factor::Value => FactorDto::Value,
            Factor::Parameter => FactorDto::Parameter,
            Factor::Blocks => FactorDto::Blocks,
        };
        Self {
            factor,
            label: contributor.label,
            share_percent: contributor.share_percent,
        }
    }
}

/// One thing keeping an item from ranking higher, which the person could change.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = HeldBack)]
pub struct HeldBackDto {
    pub gap: HeldBackGapDto,
    /// Ready to show, for example "No due date".
    pub label: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = HeldBackGap)]
pub enum HeldBackGapDto {
    /// The need is an idea or nice to have.
    Need,
    /// There is no due date.
    Due,
    /// Effort or impact is unset.
    Size,
    /// A gap this client doesn't know: one the server added later. Clients show the entry's `label`, which the
    /// server writes. Only read, never sent, so it isn't in the API's schema.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl From<HeldBack> for HeldBackDto {
    fn from(held_back: HeldBack) -> Self {
        let gap = match held_back.gap {
            Gap::Need => HeldBackGapDto::Need,
            Gap::Due => HeldBackGapDto::Due,
            Gap::Size => HeldBackGapDto::Size,
        };
        Self {
            gap,
            label: held_back.label,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RankedItem)]
pub struct RankedItemDto {
    /// 1-based position within its group.
    pub position: u32,
    /// The biggest reasons for the position, largest first.
    pub contributors: Vec<ContributorDto>,
    /// What keeps the item from ranking higher, in a fixed order: need, due date, sizes. Empty for the top item.
    /// Only things a person could change, and only while the heap's weight for them is above 0.
    pub held_back_by: Vec<HeldBackDto>,
    pub item: ItemDto,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = HeapSummary)]
pub struct HeapSummaryDto {
    pub id: Uuid,
    pub team_id: Uuid,
    /// Unique within the team; the heap's path is `/teams/{team}/heaps/{slug}`.
    pub slug: String,
    pub name: String,
    pub archived: bool,
}

impl From<&Heap> for HeapSummaryDto {
    fn from(heap: &Heap) -> Self {
        Self {
            id: *heap.id.as_uuid(),
            team_id: *heap.team_id.as_uuid(),
            slug: heap.slug.to_string(),
            name: heap.name.as_str().to_owned(),
            archived: heap.is_archived(),
        }
    }
}

/// Deserializes a list of a closed set, such as `allowed_actions`, leaving out the values this client doesn't know.
///
/// Adding an action is an additive change (design/engineering-principles.md, API, The contract §4), so a client
/// built before the action existed reads the list without it, and offers only what it knows.
/// Responses only: the server never reads these lists, so what it accepts doesn't change.
pub(crate) fn known_values<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value<T> {
        Known(T),
        Unknown(serde::de::IgnoredAny),
    }
    let values = Vec::<Value<T>>::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|value| match value {
            Value::Known(known) => Some(known),
            Value::Unknown(_) => None,
        })
        .collect())
}

/// Deserializes a field that can be absent (leave unchanged) or a value, and refuses `null`, for fields where
/// "clear" is spelled differently (an empty list). Without this, `null` would silently mean "unchanged".
pub(crate) fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    refuse_null(deserializer, "must not be null; send [] to clear")
}

/// As [`non_null`], for a field that can't be cleared at all, such as need.
pub(crate) fn never_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    refuse_null(
        deserializer,
        "must not be null; it can't be cleared, so leave it out to keep it",
    )
}

fn refuse_null<'de, D, T>(deserializer: D, message: &'static str) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom(message))
}

/// Deserializes a field that can be absent (leave unchanged), `null` (clear) or a value (set).
pub(crate) fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::audit::{AuditActionDto, AuditLogDto};
    use crate::me::{MeActionDto, MeDto};
    use crate::turns::{GoalDto, ReachDto, TurnSourceDto};

    /// A ranked item as a later server might send it: an action and a factor this client has never heard of.
    fn question_from_a_later_server() -> serde_json::Value {
        json!([{
            "number": 1, "kind": "review", "text": "Look", "details": null, "choices": [], "many": false,
            "step": null, "state": "parked", "asked_by": null, "asked_at": 0, "asked_label": "today",
            "reply": null, "version": 1, "allowed_actions": ["reply", "withdraw"],
        }])
    }

    fn ranked_item_from_a_later_server() -> serde_json::Value {
        json!({
            "position": 1,
            "contributors": [
                { "factor": "momentum", "label": "Picked up again", "share_percent": 60 },
                { "factor": "need", "label": "Critical", "share_percent": 40 },
            ],
            "held_back_by": [
                { "gap": "momentum", "label": "Too calm" },
                { "gap": "due", "label": "No due date" },
            ],
            "item": {
                "id": Uuid::nil(), "key": "k3x9a", "heap_id": Uuid::nil(), "title": "Rotate the certs",
                "description": null, "kind": "reminder", "stage": "reviewed", "stage_choices": [], "state": "done", "blocked_by": null, "need": "critical", "effort": null,
                "impact": null, "due": null, "tags": [], "parameters": [], "created_at": 0, "created_label": "today",
                "created_by": null, "closed_at": 0, "closed_label": "today", "closed_by": null,
                "closed_group": "last_month",
                "turn": { "reach": "until_done", "goal": "reviewed", "line": null, "source": "schedule", "opened_at": 0, "opened_by": null },
                "version": 1, "allowed_actions": ["edit", "archive", "reopen"],
                "steps": [{
                    "id": Uuid::nil(), "number": 1, "title": "Run in staging", "context": null, "state": "parked",
                    "version": 1, "allowed_actions": ["edit", "reorder"],
                }],
                "step_progress": null,
                "questions": question_from_a_later_server(),
            },
        })
    }

    #[gtest]
    fn values_a_later_server_added_dont_stop_a_client_reading_the_list() -> googletest::Result<()> {
        let ranked: RankedItemDto = serde_json::from_value(ranked_item_from_a_later_server())?;

        expect_that!(
            ranked.item.allowed_actions,
            elements_are![eq(&ItemActionDto::Edit), eq(&ItemActionDto::Reopen)]
        );
        expect_that!(ranked.contributors[0].factor, eq(FactorDto::Unknown));
        expect_that!(ranked.contributors[0].label, eq("Picked up again"));
        expect_that!(ranked.contributors[1].factor, eq(FactorDto::Need));
        expect_that!(ranked.held_back_by[0].gap, eq(HeldBackGapDto::Unknown));
        expect_that!(ranked.held_back_by[1].gap, eq(HeldBackGapDto::Due));
        expect_that!(ranked.item.closed_group, some(eq(ClosedGroupDto::Unknown)));
        expect_that!(ranked.item.kind, eq(KindDto::Unknown));
        expect_that!(ranked.item.stage, some(eq(StageDto::Unknown)));
        let turn = ranked.item.turn.or_fail()?;
        expect_that!(turn.reach, eq(ReachDto::Unknown));
        expect_that!(turn.goal, eq(GoalDto::Unknown));
        expect_that!(turn.source, eq(TurnSourceDto::Unknown));
        let steps = ranked.item.steps.or_fail()?;
        expect_that!(steps[0].state, eq(StepStateDto::Unknown));
        expect_that!(steps[0].allowed_actions, elements_are![eq(&StepActionDto::Edit)]);
        let questions = ranked.item.questions.or_fail()?;
        expect_that!(questions[0].kind, eq(QuestionKindDto::Unknown));
        expect_that!(questions[0].state, eq(QuestionStateDto::Unknown));
        expect_that!(
            questions[0].allowed_actions,
            elements_are![eq(&QuestionActionDto::Reply)]
        );
        Ok(())
    }

    #[gtest]
    fn an_audit_action_a_later_server_added_doesnt_stop_a_client_reading_the_history() -> googletest::Result<()> {
        let entry = |action: &str| {
            json!({
                "id": Uuid::nil(), "created_at": 0, "created_label": "today",
                "actor": { "id": Uuid::nil(), "display_name": "Ada" },
                "action": action, "phrase": "did something", "names_entity": true,
                "entity": { "type": "item", "id": Uuid::nil(), "label": "Rotate the certs" },
                "changes": [], "diff": {},
            })
        };
        let log: AuditLogDto = serde_json::from_value(json!({
            "entries": [entry("question_asked"), entry("item_pinned")],
            "next_cursor": null,
        }))?;

        expect_that!(log.entries[0].action, eq(AuditActionDto::QuestionAsked));
        expect_that!(log.entries[1].action, eq(AuditActionDto::Unknown));
        expect_that!(log.entries[1].phrase, eq("did something"));
        Ok(())
    }

    #[gtest]
    fn an_account_action_a_later_server_added_is_left_out() -> googletest::Result<()> {
        let me: MeDto = serde_json::from_value(json!({
            "user": { "id": Uuid::nil(), "username": "ada", "email": "ada@example.test", "display_name": "Ada", "version": 1 },
            "teams": [],
            "allowed_actions": ["sign_out", "export_data", "create_team"],
        }))?;

        expect_that!(
            me.allowed_actions,
            elements_are![eq(&MeActionDto::SignOut), eq(&MeActionDto::CreateTeam)]
        );
        Ok(())
    }

    #[gtest]
    fn values_a_client_doesnt_know_are_never_sent() {
        // The server never makes one, and a client that read one can't send it back as if it were a real value.
        expect_that!(serde_json::to_value(FactorDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(ClosedGroupDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(KindDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(StageDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(StepStateDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(QuestionKindDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(QuestionStateDto::Unknown), err(anything()));
        expect_that!(serde_json::to_value(AuditActionDto::Unknown), err(anything()));
    }

    #[gtest]
    fn a_request_still_refuses_a_value_it_doesnt_know() {
        // Leniency is for reading responses: the enums the server reads from requests stay strict.
        expect_that!(serde_json::from_value::<NeedDto>(json!("urgent")), err(anything()));
        expect_that!(serde_json::from_value::<SizeDto>(json!("xxl")), err(anything()));
        expect_that!(serde_json::from_value::<StateDto>(json!("archived")), err(anything()));
        expect_that!(serde_json::from_value::<RoleDto>(json!("owner")), err(anything()));
    }
}
