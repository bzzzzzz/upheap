//! Response and request types shared by several resources, and their conversions from domain types.
//!
//! These are separate from domain types even where fields match, because the API changes for different reasons.

use domain::due::{describe_date, describe_due};
use domain::heap::Heap;
use domain::item::{Due, ItemSummary, Size, StateKind};
use domain::permission::ItemAction;
use domain::ranking::{Contributor, Factor};
use domain::tag::Tag;
use domain::team::Role;
use domain::{Timestamp, ValidationError};
use jiff::tz::TimeZone;
use serde::{Deserialize, Deserializer, Serialize};
use service::items::{ItemView, RankedEntry};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

/// Effort or impact.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
#[schema(as = Size)]
pub(crate) enum SizeDto {
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

/// Item state. `done` and `wont_do` are closed.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ItemState)]
pub(crate) enum StateDto {
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

/// An action the caller can take on an item. Clients show controls only for these.
#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ItemAction)]
pub(crate) enum ItemActionDto {
    Edit,
    ChangeState,
    Reopen,
}

impl From<ItemAction> for ItemActionDto {
    fn from(action: ItemAction) -> Self {
        match action {
            ItemAction::Edit => Self::Edit,
            ItemAction::ChangeState => Self::ChangeState,
            ItemAction::Reopen => Self::Reopen,
        }
    }
}

/// A tag on an item: enough to show it.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ItemTag)]
pub(crate) struct ItemTagDto {
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

/// Another item, named where this one refers to it.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ItemRef)]
pub(crate) struct ItemRefDto {
    pub id: Uuid,
    pub title: String,
    pub state: StateDto,
}

impl From<ItemSummary> for ItemRefDto {
    fn from(item: ItemSummary) -> Self {
        Self {
            id: *item.id.as_uuid(),
            title: item.title.as_str().to_owned(),
            state: item.state.kind().into(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Role)]
pub(crate) enum RoleDto {
    Admin,
    Member,
}

impl From<Role> for RoleDto {
    fn from(role: Role) -> Self {
        match role {
            Role::Admin => Self::Admin,
            Role::Member => Self::Member,
        }
    }
}

/// A due time, ready to show.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = Due)]
pub(crate) struct DueDto {
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
#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = DueInput)]
pub(crate) struct DueInputDto {
    /// `YYYY-MM-DD`.
    #[schema(example = "2027-03-12")]
    pub date: String,
    pub firm: bool,
}

impl DueInputDto {
    pub(crate) fn to_due(&self, tz: &TimeZone) -> Result<Due, ValidationError> {
        let date: jiff::civil::Date = self.date.parse().map_err(|_| ValidationError::DueDateInvalid)?;
        let at = Timestamp::at_end_of_day(date, tz).ok_or(ValidationError::DueDateInvalid)?;
        Ok(Due { at, firm: self.firm })
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = Item)]
pub(crate) struct ItemDto {
    pub id: Uuid,
    pub heap_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub state: StateDto,
    /// The item this one waits for. Set only on blocked items, and only when a blocker was picked. It can be
    /// closed: finishing a blocker doesn't unblock what waits for it.
    pub blocked_by: Option<ItemRefDto>,
    /// `null` when unset.
    pub effort: Option<SizeDto>,
    /// `null` when unset.
    pub impact: Option<SizeDto>,
    pub due: Option<DueDto>,
    /// By name.
    pub tags: Vec<ItemTagDto>,
    /// UTC milliseconds.
    pub created_at: i64,
    /// UTC milliseconds; set exactly when the item is closed.
    pub closed_at: Option<i64>,
    /// When it was closed, ready to show ("yesterday", "3 Nov").
    pub closed_label: Option<String>,
    /// Send back in `If-Match` when editing.
    pub version: u32,
    pub allowed_actions: Vec<ItemActionDto>,
}

/// One labelled reason for an item's position.
#[derive(Debug, Serialize, ToSchema)]
#[schema(as = Contributor)]
pub(crate) struct ContributorDto {
    pub factor: FactorDto,
    pub label: String,
    /// Share of the item's priority, 1 to 100.
    pub share_percent: u8,
}

#[derive(Clone, Copy, Debug, Serialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Factor)]
pub(crate) enum FactorDto {
    Age,
    Deadline,
    Value,
    /// Priority taken on from an item this one blocks.
    Blocks,
}

impl From<Contributor> for ContributorDto {
    fn from(contributor: Contributor) -> Self {
        let factor = match contributor.factor {
            Factor::Age => FactorDto::Age,
            Factor::Deadline => FactorDto::Deadline,
            Factor::Value => FactorDto::Value,
            Factor::Blocks => FactorDto::Blocks,
        };
        Self {
            factor,
            label: contributor.label,
            share_percent: contributor.share_percent,
        }
    }
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = RankedItem)]
pub(crate) struct RankedItemDto {
    /// 1-based position within its group.
    pub position: u32,
    /// The biggest reasons for the position, largest first.
    pub contributors: Vec<ContributorDto>,
    pub item: ItemDto,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = HeapSummary)]
pub(crate) struct HeapSummaryDto {
    pub id: Uuid,
    pub team_id: Uuid,
    pub name: String,
    pub archived: bool,
}

impl From<&Heap> for HeapSummaryDto {
    fn from(heap: &Heap) -> Self {
        Self {
            id: *heap.id.as_uuid(),
            team_id: *heap.team_id.as_uuid(),
            name: heap.name.as_str().to_owned(),
            archived: heap.is_archived(),
        }
    }
}

/// Renders an item with labels computed for `as_of` in `tz`.
pub(crate) fn to_item_dto(view: ItemView, as_of: Timestamp, tz: &TimeZone) -> ItemDto {
    let item = view.item;
    let closed_at = item.state.closed_at();
    ItemDto {
        id: *item.id.as_uuid(),
        heap_id: *item.heap_id.as_uuid(),
        title: item.title.as_str().to_owned(),
        description: item.description.map(|d| d.as_str().to_owned()),
        state: item.state.kind().into(),
        blocked_by: view.blocked_by.map(ItemRefDto::from),
        effort: item.effort.map(SizeDto::from),
        impact: item.impact.map(SizeDto::from),
        due: item.due.map(|due| {
            let view = describe_due(due, as_of, tz);
            let date = due.at.to_jiff().to_zoned(tz.clone()).date().to_string();
            DueDto {
                at: due.at.as_millis(),
                date,
                firm: due.firm,
                label: view.label,
                overdue: view.overdue,
            }
        }),
        tags: view.tags.iter().map(ItemTagDto::from).collect(),
        created_at: item.created_at.as_millis(),
        closed_at: closed_at.map(Timestamp::as_millis),
        closed_label: closed_at.map(|at| describe_date(at, as_of, tz)),
        version: item.version.get(),
        allowed_actions: view.allowed_actions.into_iter().map(ItemActionDto::from).collect(),
    }
}

pub(crate) fn to_ranked_dto(entry: RankedEntry, as_of: Timestamp, tz: &TimeZone) -> RankedItemDto {
    RankedItemDto {
        position: entry.position,
        contributors: entry.contributors.into_iter().map(ContributorDto::from).collect(),
        item: to_item_dto(entry.view, as_of, tz),
    }
}

/// The `If-Match` request header, documented once.
#[derive(IntoParams)]
#[into_params(parameter_in = Header)]
#[allow(dead_code)]
pub(crate) struct IfMatchHeader {
    /// The `version` of the item or tag being edited, for example `"3"`.
    #[param(rename = "If-Match")]
    if_match: String,
}

/// Deserializes a field that can be absent (leave unchanged) or a value, and refuses `null`, for fields where
/// "clear" is spelled differently (an empty list). Without this, `null` would silently mean "unchanged".
pub(crate) fn non_null<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)?
        .map(Some)
        .ok_or_else(|| serde::de::Error::custom("must not be null; send [] to clear"))
}

/// Deserializes a field that can be absent (leave unchanged), `null` (clear) or a value (set).
pub(crate) fn double_option<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}
