//! The ranked list, closed items, and adding and editing items.

use std::collections::{BTreeMap, BTreeSet};

use domain::item::{Age, Description, Due, ItemPatch, Kind, NewItem, NewOrigin, Stage, StateKind, Title};
use domain::list_view::ListView;
use domain::tag::parse_tag_set;
use domain::{ItemId, OptionId, ParameterId, TagId, ValidationError};
use jiff::tz::TimeZone;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;

use crate::common::{
    DueInputDto, HeapSummaryDto, ItemDto, ItemRefDto, KindDto, NeedDto, RankedItemDto, SizeDto, StageDto, StateDto,
    double_option, never_null, non_null,
};
use crate::list_view::{BlockedLayoutDto, ListViewInputDto, RemindersModeDto, SortDto, StateModeDto};
use crate::waiting::ItemHeapDto;

/// The list view. Every parameter is optional; leaving them all out gives the default view: in-progress items
/// pinned, open items shown, blocked items hidden, by priority.
#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct RankedQuery {
    /// The `as_of` from an earlier response, to keep the order stable. Omit to rank at the current time.
    pub as_of: Option<i64>,
    /// Where open items go. Defaults to `shown`.
    pub open: Option<StateModeDto>,
    /// Where in-progress items go. Defaults to `pinned`.
    pub in_progress: Option<StateModeDto>,
    /// Where blocked items go. Defaults to `hidden`.
    pub blocked: Option<StateModeDto>,
    /// Whether shown blocked items get their own group or are mixed in with the open items. Defaults to `grouped`.
    pub blocked_layout: Option<BlockedLayoutDto>,
    /// Whether the response lists the heap's reminders to come in `reminders`. Defaults to `hidden`.
    pub reminders: Option<RemindersModeDto>,
    /// What each group is sorted by. Ties keep priority order. Defaults to `priority`.
    pub sort: Option<SortDto>,
    /// The parameter to sort by, a parameter of the same heap. Send it exactly when `sort` is `parameter`.
    pub sort_parameter: Option<Uuid>,
    /// Keep items with any of these tags, comma-separated. Tags of the same heap.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<Uuid>>, style = Form, explode = false)]
    pub tag: Vec<Uuid>,
    /// Keep items with any of these efforts, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub effort: Vec<SizeDto>,
    /// Keep items with any of these impacts, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub impact: Vec<SizeDto>,
    /// Keep items with any of these needs, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<NeedDto>>, style = Form, explode = false)]
    pub need: Vec<NeedDto>,
    /// Options of the same heap's parameters, comma-separated. For each parameter they belong to, keep items with
    /// any of its options listed. An archived option keeps the items that switched to its replacement.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<Uuid>>, style = Form, explode = false)]
    pub param: Vec<Uuid>,
}

impl RankedQuery {
    /// The view the query asks for, with defaults for what it leaves out. Fails when `sort_parameter` doesn't go
    /// with `sort`.
    pub fn to_view(&self) -> Result<ListView, ValidationError> {
        ListViewInputDto {
            open: self.open,
            in_progress: self.in_progress,
            blocked: self.blocked,
            blocked_layout: self.blocked_layout,
            reminders: self.reminders,
            sort: self.sort,
            sort_parameter: self.sort_parameter,
            tag: self.tag.clone(),
            effort: self.effort.clone(),
            impact: self.impact.clone(),
            need: self.need.clone(),
            param: self.param.clone(),
        }
        .to_view()
    }
}

/// The view of the list across all heaps (`GET /me/items`). Every parameter is optional; leaving them all out gives
/// the default view: in-progress items pinned, open items shown, blocked items hidden, by priority. The list has no
/// single heap, so it takes no `tag`, `param`, `sort` or `sort_parameter`, and refuses any parameter it doesn't know.
#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
#[serde(deny_unknown_fields)]
pub struct AcrossHeapsQuery {
    /// The `as_of` from an earlier response, to keep the order stable. Omit to rank at the current time.
    pub as_of: Option<i64>,
    /// Where open items go. Defaults to `shown`.
    pub open: Option<StateModeDto>,
    /// Where in-progress items go. Defaults to `pinned`.
    pub in_progress: Option<StateModeDto>,
    /// Where blocked items go. Defaults to `hidden`.
    pub blocked: Option<StateModeDto>,
    /// Whether shown blocked items get their own group or are mixed in with the open items. Defaults to `grouped`.
    pub blocked_layout: Option<BlockedLayoutDto>,
    /// Keep items with any of these efforts, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub effort: Vec<SizeDto>,
    /// Keep items with any of these impacts, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<SizeDto>>, style = Form, explode = false)]
    pub impact: Vec<SizeDto>,
    /// Keep items with any of these needs, comma-separated.
    #[serde(
        default,
        deserialize_with = "comma_separated",
        serialize_with = "comma_joined",
        skip_serializing_if = "Vec::is_empty"
    )]
    #[param(value_type = Option<Vec<NeedDto>>, style = Form, explode = false)]
    pub need: Vec<NeedDto>,
}

impl AcrossHeapsQuery {
    /// The view the query asks for, with defaults for what it leaves out: sorted by priority, with no tag or
    /// parameter filter.
    pub fn to_view(&self) -> Result<ListView, ValidationError> {
        ListViewInputDto {
            open: self.open,
            in_progress: self.in_progress,
            blocked: self.blocked,
            blocked_layout: self.blocked_layout,
            reminders: None,
            sort: None,
            sort_parameter: None,
            tag: Vec::new(),
            effort: self.effort.clone(),
            impact: self.impact.clone(),
            need: self.need.clone(),
            param: Vec::new(),
        }
        .to_view()
    }
}

/// The non-closed items of all the caller's heaps in one ranked list, grouped as the view says.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AcrossHeapsList)]
pub struct AcrossHeapsListDto {
    /// The time every heap was ranked at (UTC milliseconds). Send it back as `as_of` to keep the order stable.
    pub as_of: i64,
    /// How many rows the view lists in all groups, whether or not they all fit under the cap of 500 rows.
    pub count: u32,
    /// Items in pinned states (in progress by default), above the rest. Each group is ordered by the item's score in
    /// its own heap, highest first; ties by heap name, then the item's position in its heap. Heaps weigh the same
    /// for now.
    pub pinned: Vec<AcrossHeapsItemDto>,
    /// Items in shown states, including blocked items when they're mixed in.
    pub ranked: Vec<AcrossHeapsItemDto>,
    /// Shown blocked items, when they're grouped. Empty by default, because blocked items are hidden.
    pub blocked: Vec<AcrossHeapsItemDto>,
    /// How many items the filters keep but are left out because their state is hidden, over all heaps.
    pub hidden_count: u32,
    /// How many items are in progress, over all heaps, whatever the view and its filters leave out.
    pub in_progress_count: u32,
}

/// A row of the list across heaps.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = AcrossHeapsItem)]
pub struct AcrossHeapsItemDto {
    /// 1-based position within its group across all heaps.
    pub position: u32,
    /// The heap the item is in, and its team.
    pub heap: ItemHeapDto,
    /// The item as its own heap's list gives it. Its `position` is within its heap's group, and `held_back_by`
    /// names what holds it back in that heap.
    pub entry: RankedItemDto,
}

/// A comma-separated query value, such as `tag=a,b`. An empty value is an empty list.
fn comma_separated<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let raw = String::deserialize(deserializer)?;
    raw.split(',')
        .filter(|part| !part.is_empty())
        .map(|part| T::deserialize(part.to_owned().into_deserializer()))
        .collect()
}

/// The other way from [`comma_separated`]: `[a, b]` as `a,b`. Every value must serialize as a string, as enums of
/// unit variants and UUIDs do.
fn comma_joined<S, T>(values: &[T], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    T: Serialize,
{
    let parts = values
        .iter()
        .map(|value| match serde_json::to_value(value) {
            Ok(serde_json::Value::String(part)) => Ok(part),
            _ => Err(serde::ser::Error::custom("a comma-separated value must be a string")),
        })
        .collect::<Result<Vec<_>, S::Error>>()?;
    serializer.serialize_str(&parts.join(","))
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RankedList)]
pub struct RankedListDto {
    pub heap: HeapSummaryDto,
    /// The time the ranking used (UTC milliseconds). Send it back as `as_of` to keep the order stable.
    pub as_of: i64,
    /// Items in pinned states (in progress by default), above the rest.
    pub pinned: Vec<RankedItemDto>,
    /// Items in shown states, including blocked items when they're mixed in.
    pub ranked: Vec<RankedItemDto>,
    /// Shown blocked items, when they're grouped. Empty by default, because blocked items are hidden.
    pub blocked: Vec<RankedItemDto>,
    /// The heap's reminders to come that the filters keep, soonest first, when `reminders` is `shown`; empty
    /// otherwise. A reminder to come has no position and no explanation: it is out of the ranking until its date
    /// (`arrives_at`), and out of every count.
    // Decoding tolerates an older server that doesn't send it; every response of this server does, so the schema
    // says required.
    #[serde(default)]
    #[schema(required = true)]
    pub reminders: Vec<ItemDto>,
    /// How many items the filters keep but are left out because their state is hidden.
    pub hidden_count: u32,
    /// How many items the view would list without its filters (`tag`, `need`, `effort`, `impact`, `param`): the "14" in
    /// "6 of 14". Hidden states still leave theirs out, and reminders to come aren't counted.
    pub unfiltered_count: u32,
    /// How many of the heap's items are in progress, whatever the view and its filters leave out.
    pub in_progress_count: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ClosedQuery {
    /// `next_cursor` from the previous page.
    pub cursor: Option<String>,
    /// Page size, 1 to 200. Defaults to 50.
    pub limit: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ClosedItems)]
pub struct ClosedItemsDto {
    pub heap: HeapSummaryDto,
    /// Most recently closed first.
    pub items: Vec<ItemDto>,
    /// Pass as `cursor` for the next page; `null` on the last page.
    pub next_cursor: Option<String>,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = CreateItemRequest)]
pub struct CreateItemDto {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Required: every item says how much it's needed.
    pub need: NeedDto,
    /// `"note"` when left out or `null`. A new item is as written, so a task starts at that stage, unless an agent
    /// run adds it: that one starts with its context. A `"reminder"` needs `due` with a `date`: that date is when it
    /// joins the list, and its due is made firm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<KindDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effort: Option<SizeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub impact: Option<SizeDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due: Option<DueInputDto>,
    /// Tags of the same heap, at most 50.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag_ids: Option<Vec<Uuid>>,
    /// An option for each parameter to set, by parameter ID: options in use of the same heap's parameters in use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter_values: Option<BTreeMap<Uuid, Uuid>>,
    /// Makes the item from another, which must be an open note of the same heap (a split): the item gets a link to
    /// it. Not for a reminder. In a run this is required, and the item must be the run's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<OriginInputDto>,
    /// The item the new one starts blocked by: a non-closed item of the same heap. The new item is created blocked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_by: Option<Uuid>,
}

/// The item a new item is made from, and whose age it takes.
#[derive(Serialize, Deserialize, ToSchema)]
#[schema(as = OriginInput)]
pub struct OriginInputDto {
    pub item_id: Uuid,
    /// Required: nothing is inherited silently. `keep` ranks the new item's age from the origin's, `new` from now.
    pub age: AgeDto,
}

/// Whose age a new item made from another counts from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = Age)]
pub enum AgeDto {
    Keep,
    New,
}

impl From<AgeDto> for Age {
    fn from(age: AgeDto) -> Self {
        match age {
            AgeDto::Keep => Self::Keep,
            AgeDto::New => Self::New,
        }
    }
}

impl CreateItemDto {
    /// The new item's fields, with due dates read in `tz`. Fails on the first invalid field, and with
    /// `ReminderNeedsDate` for a reminder without a due.
    pub fn to_new_item(&self, tz: &TimeZone) -> Result<NewItem, ValidationError> {
        let kind = self.kind.map(Kind::try_from).transpose()?.unwrap_or(Kind::Note);
        let due = self.due.as_ref().map(|due| due.to_due(tz)).transpose()?;
        // A reminder's date is its exact deadline, and the start of the date is when it joins the list.
        let (due, arrives_at) = match (kind, &self.due, due) {
            (Kind::Reminder, Some(input), Some(due)) => {
                (Some(Due { firm: true, ..due }), Some(input.to_start_of_day(tz)?))
            }
            (Kind::Reminder, _, _) => return Err(ValidationError::ReminderNeedsDate),
            (_, _, due) => (due, None),
        };
        Ok(NewItem {
            title: Title::parse(&self.title)?,
            description: self
                .description
                .as_deref()
                .map(Description::parse)
                .transpose()?
                .flatten(),
            need: self.need.into(),
            kind,
            effort: self.effort.map(Into::into),
            impact: self.impact.map(Into::into),
            due,
            arrives_at,
            tags: to_tag_set(self.tag_ids.as_deref().unwrap_or_default())?,
            parameters: self
                .parameter_values
                .iter()
                .flatten()
                .map(|(parameter, option)| (ParameterId::from_uuid(*parameter), OptionId::from_uuid(*option)))
                .collect(),
            origin: self.origin.as_ref().map(|origin| NewOrigin {
                item_id: ItemId::from_uuid(origin.item_id),
                age: origin.age.into(),
            }),
            blocked_by: self.blocked_by.map(ItemId::from_uuid),
        })
    }
}

/// A partial edit. Leave a field out to keep it; send `null` to clear it.
#[derive(Default, Serialize, Deserialize, ToSchema)]
#[schema(as = ItemPatchRequest)]
pub struct ItemPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<String>, nullable)]
    pub description: Option<Option<String>>,
    /// Need can be changed but not cleared; `null` is refused.
    #[serde(default, deserialize_with = "never_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = NeedDto, required = false)]
    pub need: Option<NeedDto>,
    /// A note or a reminder can become a task; any other change is refused (`kind_change_not_allowed`). Applied
    /// before `stage`. A reminder made a task starts as written. `null` is refused.
    #[serde(default, deserialize_with = "never_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = KindDto, required = false)]
    pub kind: Option<KindDto>,
    /// `"defined"` needs a task, or `kind` set to `"task"` in the same patch. `null` is refused.
    #[serde(default, deserialize_with = "never_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = StageDto, required = false)]
    pub stage: Option<StageDto>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<SizeDto>, nullable)]
    pub effort: Option<Option<SizeDto>>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<SizeDto>, nullable)]
    pub impact: Option<Option<SizeDto>>,
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<DueInputDto>, nullable)]
    pub due: Option<Option<DueInputDto>>,
    /// Replaces the item's tags. Tags of the same heap, at most 50. Send `[]` to clear them; `null` is refused.
    #[serde(default, deserialize_with = "non_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Vec<Uuid>, required = false)]
    pub tag_ids: Option<Vec<Uuid>>,
    /// Values to change, by parameter ID: an option in use to set, or `null` to clear. Parameters left out keep
    /// their values.
    #[serde(default, deserialize_with = "non_null", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = BTreeMap<Uuid, Option<Uuid>>, required = false)]
    pub parameter_values: Option<BTreeMap<Uuid, Option<Uuid>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<StateDto>,
    /// The item this one waits for: a non-closed item of the same heap. Only a blocked item can have one, so send
    /// it with `"state": "blocked"` unless the item is blocked already. `null` clears it. Leaving the blocked state
    /// clears it too.
    #[serde(default, deserialize_with = "double_option", skip_serializing_if = "Option::is_none")]
    #[schema(value_type = Option<Uuid>, nullable)]
    pub blocked_by_item_id: Option<Option<Uuid>>,
}

impl ItemPatchDto {
    /// The edit, with due dates read in `tz`. Fails on the first invalid field.
    pub fn to_patch(&self, tz: &TimeZone) -> Result<ItemPatch, ValidationError> {
        Ok(ItemPatch {
            title: self.title.as_deref().map(Title::parse).transpose()?,
            description: match &self.description {
                None => None,
                Some(None) => Some(None),
                Some(Some(raw)) => Some(Description::parse(raw)?),
            },
            need: self.need.map(Into::into),
            kind: self.kind.map(Kind::try_from).transpose()?,
            stage: self.stage.map(Stage::try_from).transpose()?,
            effort: self.effort.map(|e| e.map(Into::into)),
            impact: self.impact.map(|i| i.map(Into::into)),
            due: match &self.due {
                None => None,
                Some(None) => Some(None),
                Some(Some(due)) => Some(Some(due.to_due(tz)?)),
            },
            due_starts_at: match &self.due {
                Some(Some(due)) => Some(due.to_start_of_day(tz)?),
                _ => None,
            },
            tags: self.tag_ids.as_deref().map(to_tag_set).transpose()?,
            parameters: self
                .parameter_values
                .iter()
                .flatten()
                .map(|(parameter, option)| (ParameterId::from_uuid(*parameter), option.map(OptionId::from_uuid)))
                .collect(),
            state: self.state.map(StateKind::from),
            blocked_by: self.blocked_by_item_id.map(|id| id.map(ItemId::from_uuid)),
        })
    }
}

fn to_tag_set(ids: &[Uuid]) -> Result<BTreeSet<TagId>, ValidationError> {
    parse_tag_set(ids.iter().copied().map(TagId::from_uuid))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn ranked_query_writes_lists_comma_separated_and_leaves_out_what_is_unset() -> googletest::Result<()> {
        // Arrange
        let query = RankedQuery {
            blocked: Some(StateModeDto::Shown),
            effort: vec![SizeDto::S, SizeDto::Xl],
            need: vec![NeedDto::NiceToHave],
            ..RankedQuery::default()
        };

        // Act
        let encoded = serde_urlencoded::to_string(&query)?;

        // Assert
        verify_that!(encoded, eq("blocked=shown&effort=s%2Cxl&need=nice_to_have"))
    }

    #[gtest]
    fn item_patch_writes_clearing_as_null_and_leaves_out_what_is_unset() -> googletest::Result<()> {
        // Arrange
        let patch = ItemPatchDto {
            description: Some(None),
            state: Some(StateDto::Done),
            ..ItemPatchDto::default()
        };

        // Act
        let written = serde_json::to_string(&patch)?;
        let read: ItemPatchDto = serde_json::from_str(&written)?;

        // Assert
        verify_that!(written, eq(r#"{"description":null,"state":"done"}"#))?;
        verify_that!(read.description, some(none()))?;
        verify_that!(read.effort, none())?;
        verify_that!(read.state, some(eq(StateDto::Done)))
    }

    #[gtest]
    fn create_item_leaves_out_what_is_unset() -> googletest::Result<()> {
        // Arrange
        let item = CreateItemDto {
            title: "Rotate certs".to_owned(),
            description: None,
            need: NeedDto::NiceToHave,
            kind: None,
            effort: Some(SizeDto::S),
            impact: None,
            due: Some(DueInputDto {
                date: "2027-03-12".to_owned(),
                firm: true,
            }),
            tag_ids: None,
            parameter_values: None,
            origin: None,
            blocked_by: None,
        };

        // Act
        let written = serde_json::to_string(&item)?;

        // Assert
        verify_that!(
            written,
            eq(
                r#"{"title":"Rotate certs","need":"nice_to_have","effort":"s","due":{"date":"2027-03-12","firm":true}}"#
            )
        )
    }

    #[gtest]
    fn create_item_writes_a_kind_only_when_it_is_set() -> googletest::Result<()> {
        // Arrange
        let task: CreateItemDto = serde_json::from_str(r#"{"title":"Rotate certs","need":"expected","kind":"task"}"#)?;

        // Act
        let written = serde_json::to_string(&task)?;

        // Assert
        verify_that!(
            written,
            eq(r#"{"title":"Rotate certs","need":"expected","kind":"task"}"#)
        )
    }

    #[gtest]
    fn a_new_item_is_a_note_unless_the_kind_says_task() -> googletest::Result<()> {
        // Arrange
        let read = |json: &str| serde_json::from_str::<CreateItemDto>(json);
        let tz = TimeZone::UTC;

        // Act
        let task = read(r#"{"title":"A","need":"expected","kind":"task"}"#)?.to_new_item(&tz)?;
        let unset = read(r#"{"title":"A","need":"expected"}"#)?.to_new_item(&tz)?;
        let null = read(r#"{"title":"A","need":"expected","kind":null}"#)?.to_new_item(&tz)?;
        let unknown = read(r#"{"title":"A","need":"expected","kind":"reminder"}"#)?.to_new_item(&tz);

        // Assert
        verify_that!(task.kind, eq(Kind::Task))?;
        verify_that!(unset.kind, eq(Kind::Note))?;
        verify_that!(null.kind, eq(Kind::Note))?;
        verify_that!(unknown.is_err(), eq(true))
    }

    #[gtest]
    fn ranked_query_reads_back_what_it_writes() -> googletest::Result<()> {
        // Arrange
        let tag = Uuid::from_u128(7);
        let query = RankedQuery {
            sort: Some(SortDto::Due),
            tag: vec![tag],
            impact: vec![SizeDto::M, SizeDto::L],
            ..RankedQuery::default()
        };

        // Act
        let read: RankedQuery = serde_urlencoded::from_str(&serde_urlencoded::to_string(&query)?)?;

        // Assert
        verify_that!(read.sort, some(eq(SortDto::Due)))?;
        verify_that!(read.tag, elements_are![eq(&tag)])?;
        verify_that!(read.impact, elements_are![eq(&SizeDto::M), eq(&SizeDto::L)])
    }
}

/// Where to move an item.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ItemMoveRequest)]
pub struct ItemMoveDto {
    /// The heap to move the item to: another heap of the item's team.
    pub heap_id: Uuid,
}

/// Weights to rank the heap with, without saving them. Leave a weight out to keep its stored value.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = RankingPreviewRequest)]
pub struct RankingPreviewRequestDto {
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub need: Option<i64>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub age: Option<i64>,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub deadline: Option<i64>,
    /// 0 to 100. Value per effort.
    #[schema(minimum = 0, maximum = 100)]
    pub value: Option<i64>,
    /// Parameters of the heap in use to give another weight, each listed once.
    #[serde(default)]
    pub parameters: Vec<ParameterWeightDraftDto>,
    /// How many items to list, 1 to 20. Defaults to 5.
    #[schema(minimum = 1, maximum = 20)]
    pub limit: Option<i64>,
}

/// A draft weight for one parameter.
#[derive(Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ParameterWeightDraft)]
pub struct ParameterWeightDraftDto {
    pub id: Uuid,
    /// 0 to 100.
    #[schema(minimum = 0, maximum = 100)]
    pub weight: i64,
}

/// The top of the list under draft weights. Nothing is saved.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RankingPreview)]
pub struct RankingPreviewDto {
    /// The time both rankings used (UTC milliseconds).
    pub as_of: i64,
    /// The first items of the ranked group (what `GET …/items` lists under `ranked` with the default view) under
    /// the draft weights, in order.
    pub items: Vec<RankingPreviewItemDto>,
}

/// An item near the top of the list under draft weights.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = RankingPreviewItem)]
pub struct RankingPreviewItemDto {
    pub item: ItemRefDto,
    /// 1-based position under the draft weights.
    pub position: u32,
    /// 1-based position of the same item in the same group under the stored weights.
    pub current_position: u32,
}
