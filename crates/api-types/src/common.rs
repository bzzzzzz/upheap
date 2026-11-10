//! Response and request types shared by several resources, and their conversions from domain types.
//!
//! These are separate from domain types even where fields match, because the API changes for different reasons.

use domain::due::ClosedGroup;
use domain::heap::Heap;
use domain::item::{Due, ItemSummary, Kind, Need, Size, Stage, StateKind};
use domain::permission::ItemAction;
use domain::ranking::{Contributor, Factor};
use domain::tag::Tag;
use domain::team::Role;
use domain::user::User;
use domain::{Timestamp, ValidationError};
use jiff::tz::TimeZone;
use serde::{Deserialize, Deserializer, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

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
}

impl From<Kind> for KindDto {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Note => Self::Note,
            Kind::Task => Self::Task,
        }
    }
}

impl From<KindDto> for Kind {
    fn from(kind: KindDto) -> Self {
        match kind {
            KindDto::Note => Self::Note,
            KindDto::Task => Self::Task,
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

impl From<StageDto> for Stage {
    fn from(stage: StageDto) -> Self {
        match stage {
            StageDto::AsWritten => Self::AsWritten,
            StageDto::WithContext => Self::WithContext,
            StageDto::Defined => Self::Defined,
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
}

impl From<ItemAction> for ItemActionDto {
    fn from(action: ItemAction) -> Self {
        match action {
            ItemAction::Edit => Self::Edit,
            ItemAction::ChangeState => Self::ChangeState,
            ItemAction::Reopen => Self::Reopen,
            ItemAction::MakeTask => Self::MakeTask,
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

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RankedItem)]
pub struct RankedItemDto {
    /// 1-based position within its group.
    pub position: u32,
    /// The biggest reasons for the position, largest first.
    pub contributors: Vec<ContributorDto>,
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
    use crate::me::{MeActionDto, MeDto};

    /// A ranked item as a later server might send it: an action and a factor this client has never heard of.
    fn ranked_item_from_a_later_server() -> serde_json::Value {
        json!({
            "position": 1,
            "contributors": [
                { "factor": "momentum", "label": "Picked up again", "share_percent": 60 },
                { "factor": "need", "label": "Critical", "share_percent": 40 },
            ],
            "item": {
                "id": Uuid::nil(), "key": "k3x9a", "heap_id": Uuid::nil(), "title": "Rotate the certs",
                "description": null, "kind": "note", "stage": null, "stage_choices": [], "state": "done", "blocked_by": null, "need": "critical", "effort": null,
                "impact": null, "due": null, "tags": [], "parameters": [], "created_at": 0, "created_label": "today",
                "created_by": null, "closed_at": 0, "closed_label": "today", "closed_by": null,
                "closed_group": "last_month", "version": 1, "allowed_actions": ["edit", "archive", "reopen"],
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
        expect_that!(ranked.item.closed_group, some(eq(ClosedGroupDto::Unknown)));
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
