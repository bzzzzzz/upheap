//! How the ranked list is shown, in the API's terms: the query parameters of `GET /heaps/{id}/items` and the view a
//! saved view stores. Both use the same names and values, so a client can turn a saved view into a list query
//! field by field.

use domain::TagId;
use domain::item::Size;
use domain::list_view::{BlockedLayout, ListFilter, ListView, SortKey, StateMode};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::dto::SizeDto;

/// Where items in one state go in the list.
#[derive(Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = StateMode)]
pub(crate) enum StateModeDto {
    Pinned,
    Shown,
    Hidden,
}

impl From<StateModeDto> for StateMode {
    fn from(mode: StateModeDto) -> Self {
        match mode {
            StateModeDto::Pinned => Self::Pinned,
            StateModeDto::Shown => Self::Shown,
            StateModeDto::Hidden => Self::Hidden,
        }
    }
}

impl From<StateMode> for StateModeDto {
    fn from(mode: StateMode) -> Self {
        match mode {
            StateMode::Pinned => Self::Pinned,
            StateMode::Shown => Self::Shown,
            StateMode::Hidden => Self::Hidden,
        }
    }
}

/// Where shown blocked items go.
#[derive(Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BlockedLayout)]
pub(crate) enum BlockedLayoutDto {
    Grouped,
    Mixed,
}

impl From<BlockedLayoutDto> for BlockedLayout {
    fn from(layout: BlockedLayoutDto) -> Self {
        match layout {
            BlockedLayoutDto::Grouped => Self::Grouped,
            BlockedLayoutDto::Mixed => Self::Mixed,
        }
    }
}

impl From<BlockedLayout> for BlockedLayoutDto {
    fn from(layout: BlockedLayout) -> Self {
        match layout {
            BlockedLayout::Grouped => Self::Grouped,
            BlockedLayout::Mixed => Self::Mixed,
        }
    }
}

/// What the list is sorted by: `priority` (highest first), `due` (soonest first), `effort` (smallest first) or
/// `impact` (largest first). Items without the value come last.
#[derive(Clone, Copy, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ListSort)]
pub(crate) enum SortDto {
    Priority,
    Due,
    Effort,
    Impact,
}

impl From<SortDto> for SortKey {
    fn from(sort: SortDto) -> Self {
        match sort {
            SortDto::Priority => Self::Priority,
            SortDto::Due => Self::Due,
            SortDto::Effort => Self::Effort,
            SortDto::Impact => Self::Impact,
        }
    }
}

impl From<SortKey> for SortDto {
    fn from(sort: SortKey) -> Self {
        match sort {
            SortKey::Priority => Self::Priority,
            SortKey::Due => Self::Due,
            SortKey::Effort => Self::Effort,
            SortKey::Impact => Self::Impact,
        }
    }
}

/// A list view with every setting spelled out, as a saved view returns it.
#[derive(Serialize, ToSchema)]
#[schema(as = ListView)]
pub(crate) struct ListViewDto {
    pub open: StateModeDto,
    pub in_progress: StateModeDto,
    pub blocked: StateModeDto,
    pub blocked_layout: BlockedLayoutDto,
    pub sort: SortDto,
    /// Keeps items with any of these tags. Empty doesn't filter.
    pub tag: Vec<Uuid>,
    /// Keeps items with any of these efforts. Empty doesn't filter.
    pub effort: Vec<SizeDto>,
    /// Keeps items with any of these impacts. Empty doesn't filter.
    pub impact: Vec<SizeDto>,
}

impl From<&ListView> for ListViewDto {
    fn from(view: &ListView) -> Self {
        let sizes = |sizes: &std::collections::BTreeSet<Size>| sizes.iter().copied().map(SizeDto::from).collect();
        Self {
            open: view.open.into(),
            in_progress: view.in_progress.into(),
            blocked: view.blocked.into(),
            blocked_layout: view.blocked_layout.into(),
            sort: view.sort.into(),
            tag: view.filter.tags.iter().map(|id| *id.as_uuid()).collect(),
            effort: sizes(&view.filter.effort),
            impact: sizes(&view.filter.impact),
        }
    }
}

/// A list view to save. Leave a setting out for its default: in-progress items pinned, open items shown, blocked
/// items hidden and grouped, by priority, without filters.
#[derive(Clone, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ListViewInput)]
pub(crate) struct ListViewInputDto {
    pub open: Option<StateModeDto>,
    pub in_progress: Option<StateModeDto>,
    pub blocked: Option<StateModeDto>,
    pub blocked_layout: Option<BlockedLayoutDto>,
    pub sort: Option<SortDto>,
    /// Tags of the same heap. Keeps items with any of them.
    #[serde(default)]
    pub tag: Vec<Uuid>,
    #[serde(default)]
    pub effort: Vec<SizeDto>,
    #[serde(default)]
    pub impact: Vec<SizeDto>,
}

impl ListViewInputDto {
    /// The view, with defaults for what was left out. Duplicates in the lists collapse.
    pub fn to_view(&self) -> ListView {
        let default = ListView::default();
        ListView {
            open: self.open.map_or(default.open, Into::into),
            in_progress: self.in_progress.map_or(default.in_progress, Into::into),
            blocked: self.blocked.map_or(default.blocked, Into::into),
            blocked_layout: self.blocked_layout.map_or(default.blocked_layout, Into::into),
            sort: self.sort.map_or(default.sort, Into::into),
            filter: ListFilter {
                // Not capped: every tag must belong to the heap, which the service checks in memory.
                tags: self.tag.iter().copied().map(TagId::from_uuid).collect(),
                effort: self.effort.iter().copied().map(Size::from).collect(),
                impact: self.impact.iter().copied().map(Size::from).collect(),
            },
        }
    }
}
