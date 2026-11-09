//! How the ranked list is shown, in the API's terms: the query parameters of `GET /teams/{team}/heaps/{heap}/items`
//! and the view a saved view stores. Both use the same names and values, so a client can turn a saved view into a
//! list query field by field.

use domain::item::{Need, Size};
use domain::list_view::{BlockedLayout, ListFilter, ListView, SortKey, StateMode};
use domain::{OptionId, ParameterId, TagId, ValidationError};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::common::{NeedDto, SizeDto};

/// Where items in one state go in the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = StateMode)]
pub enum StateModeDto {
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
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = BlockedLayout)]
pub enum BlockedLayoutDto {
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

/// What the list is sorted by: `priority` (highest first), `due` (soonest first), `effort` (smallest first),
/// `impact` (largest first) or `parameter` (the custom parameter in `sort_parameter`, its first option first).
/// Items without the value come last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
#[schema(as = ListSort)]
pub enum SortDto {
    Priority,
    Due,
    Effort,
    Impact,
    Parameter,
}

impl SortDto {
    /// The sort, with `parameter` exactly when sorting by a custom parameter.
    pub fn to_key(self, parameter: Option<Uuid>) -> Result<SortKey, ValidationError> {
        let parameter = parameter.map(ParameterId::from_uuid);
        match (self, parameter) {
            (Self::Priority, None) => Ok(SortKey::Priority),
            (Self::Due, None) => Ok(SortKey::Due),
            (Self::Effort, None) => Ok(SortKey::Effort),
            (Self::Impact, None) => Ok(SortKey::Impact),
            (Self::Parameter, Some(parameter)) => Ok(SortKey::Parameter(parameter)),
            _ => Err(ValidationError::SortParameterMismatch),
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
            SortKey::Parameter(_) => Self::Parameter,
        }
    }
}

/// A list view with every setting spelled out, as a saved view returns it.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
#[schema(as = ListView)]
pub struct ListViewDto {
    pub open: StateModeDto,
    pub in_progress: StateModeDto,
    pub blocked: StateModeDto,
    pub blocked_layout: BlockedLayoutDto,
    pub sort: SortDto,
    /// The parameter the list is sorted by, when `sort` is `parameter`.
    pub sort_parameter: Option<Uuid>,
    /// Keeps items with any of these tags. Empty doesn't filter.
    pub tag: Vec<Uuid>,
    /// Keeps items with any of these efforts. Empty doesn't filter.
    pub effort: Vec<SizeDto>,
    /// Keeps items with any of these impacts. Empty doesn't filter.
    pub impact: Vec<SizeDto>,
    /// Keeps items with any of these needs. Empty doesn't filter.
    pub need: Vec<NeedDto>,
    /// Options of custom parameters. For each parameter they belong to, keeps items with any of its options listed.
    /// Empty doesn't filter.
    pub param: Vec<Uuid>,
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
            sort_parameter: view.sort.parameter().map(|id| *id.as_uuid()),
            tag: view.filter.tags.iter().map(|id| *id.as_uuid()).collect(),
            effort: sizes(&view.filter.effort),
            impact: sizes(&view.filter.impact),
            need: view.filter.need.iter().copied().map(NeedDto::from).collect(),
            param: view.filter.options.iter().map(|id| *id.as_uuid()).collect(),
        }
    }
}

/// A list view to save. Leave a setting out for its default: in-progress items pinned, open items shown, blocked
/// items hidden and grouped, by priority, without filters. Send `sort_parameter` exactly when `sort` is `parameter`.
#[derive(Clone, Debug, Default, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = ListViewInput)]
pub struct ListViewInputDto {
    pub open: Option<StateModeDto>,
    pub in_progress: Option<StateModeDto>,
    pub blocked: Option<StateModeDto>,
    pub blocked_layout: Option<BlockedLayoutDto>,
    pub sort: Option<SortDto>,
    /// A parameter of the same heap, when `sort` is `parameter`.
    pub sort_parameter: Option<Uuid>,
    /// Tags of the same heap. Keeps items with any of them.
    #[serde(default)]
    pub tag: Vec<Uuid>,
    #[serde(default)]
    pub effort: Vec<SizeDto>,
    #[serde(default)]
    pub impact: Vec<SizeDto>,
    #[serde(default)]
    pub need: Vec<NeedDto>,
    /// Options of the same heap's parameters.
    #[serde(default)]
    pub param: Vec<Uuid>,
}

impl ListViewInputDto {
    /// The view, with defaults for what was left out. Duplicates in the lists collapse. Fails when
    /// `sort_parameter` doesn't go with `sort`.
    pub fn to_view(&self) -> Result<ListView, ValidationError> {
        let default = ListView::default();
        let sort = self.sort.unwrap_or(SortDto::Priority).to_key(self.sort_parameter)?;
        Ok(ListView {
            open: self.open.map_or(default.open, Into::into),
            in_progress: self.in_progress.map_or(default.in_progress, Into::into),
            blocked: self.blocked.map_or(default.blocked, Into::into),
            blocked_layout: self.blocked_layout.map_or(default.blocked_layout, Into::into),
            sort,
            filter: ListFilter {
                // Not capped: every tag and option must belong to the heap, which the service checks in memory.
                tags: self.tag.iter().copied().map(TagId::from_uuid).collect(),
                effort: self.effort.iter().copied().map(Size::from).collect(),
                impact: self.impact.iter().copied().map(Size::from).collect(),
                need: self.need.iter().copied().map(Need::from).collect(),
                options: self.param.iter().copied().map(OptionId::from_uuid).collect(),
            },
        })
    }
}
