//! Saved views. Every lookup that comes from a request is scoped by heap through the view's `heap_id`, which the
//! service authorizes. Views are archived, never deleted; there is no delete function, on purpose.
//!
//! The list view is stored as JSON text with stable codes, so renaming a Rust variant never changes stored data.
//! Fields added later must have a default, so rows written before them still read.

use std::collections::BTreeSet;

use domain::item::{Need, Size};
use domain::list_view::{BlockedLayout, ListFilter, ListView, SortKey, StateMode};
use domain::saved_view::SavedView;
use domain::text::Name;
use domain::{HeapId, OptionId, ParameterId, SavedViewId, TagId, Timestamp, UserId, Version};
use sea_orm::{ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QuerySelect};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::StoreError;
use crate::codes::{RowReader, id_to_db, version_to_db};
use crate::entities::saved_views;

const TABLE: &str = "saved_views";

pub async fn insert_saved_view(db: &impl ConnectionTrait, view: &SavedView) -> Result<(), StoreError> {
    saved_views::Entity::insert(to_row(view)?)
        .exec_without_returning(db)
        .await?;
    Ok(())
}

pub async fn find_saved_view(db: &impl ConnectionTrait, id: SavedViewId) -> Result<Option<SavedView>, StoreError> {
    let row = saved_views::Entity::find_by_id(id_to_db(id.as_uuid())).one(db).await?;
    row.map(to_saved_view).transpose()
}

/// Reads the view and locks its row until the transaction ends.
///
/// A locking read sees the latest committed row even on MySQL, where a plain read sees the transaction's snapshot.
/// Callers lock the heap first, as every heap write does, so the order of locks is always heap, then view.
pub async fn lock_saved_view(db: &impl ConnectionTrait, id: SavedViewId) -> Result<Option<SavedView>, StoreError> {
    let row = saved_views::Entity::find_by_id(id_to_db(id.as_uuid()))
        .lock_exclusive()
        .one(db)
        .await?;
    row.map(to_saved_view).transpose()
}

/// The views of `heap_id` that aren't archived, by name ignoring case, then by ID.
///
/// Sorted here rather than in SQL, because MySQL and SQLite order text differently by default.
pub async fn list_saved_views(db: &impl ConnectionTrait, heap_id: HeapId) -> Result<Vec<SavedView>, StoreError> {
    let mut views = saved_views::Entity::find()
        .filter(saved_views::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(saved_views::Column::ArchivedAt.is_null())
        .all(db)
        .await?
        .into_iter()
        .map(to_saved_view)
        .collect::<Result<Vec<_>, _>>()?;
    views.sort_by_cached_key(|view| (view.name.as_str().to_lowercase(), view.id));
    Ok(views)
}

/// The views among `ids` that belong to `heap_id`, archived ones included, in no particular order. IDs from other
/// heaps, or unknown ones, are left out.
pub async fn list_saved_views_by_ids(
    db: &impl ConnectionTrait,
    heap_id: HeapId,
    ids: &[SavedViewId],
) -> Result<Vec<SavedView>, StoreError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    saved_views::Entity::find()
        .filter(saved_views::Column::HeapId.eq(id_to_db(heap_id.as_uuid())))
        .filter(saved_views::Column::Id.is_in(ids.iter().map(|id| id_to_db(id.as_uuid()))))
        .all(db)
        .await?
        .into_iter()
        .map(to_saved_view)
        .collect()
}

/// Writes `view` if the stored row still has version `expected`, and returns it with its new version. Returns
/// `None` when the stored version differs.
pub async fn update_saved_view(
    db: &impl ConnectionTrait,
    view: &SavedView,
    expected: Version,
) -> Result<Option<SavedView>, StoreError> {
    let saved = SavedView {
        version: expected.next(),
        ..view.clone()
    };
    let mut row = to_row(&saved)?;
    row.id = sea_orm::ActiveValue::NotSet;
    let id = id_to_db(view.id.as_uuid());
    let result = saved_views::Entity::update_many()
        .set(row)
        .filter(saved_views::Column::Id.eq(id.as_str()))
        .filter(saved_views::Column::Version.eq(version_to_db(expected, TABLE, &id)?))
        .exec(db)
        .await?;
    Ok((result.rows_affected == 1).then_some(saved))
}

fn to_row(view: &SavedView) -> Result<saved_views::ActiveModel, StoreError> {
    let id = id_to_db(view.id.as_uuid());
    // Proven invariant: a struct of strings and UUIDs always serializes to JSON.
    #[allow(clippy::expect_used)]
    let list_view = serde_json::to_string(&StoredListView::from(&view.list_view)).expect("plain data serializes");
    Ok(saved_views::ActiveModel {
        version: Set(version_to_db(view.version, TABLE, &id)?),
        id: Set(id),
        heap_id: Set(id_to_db(view.heap_id.as_uuid())),
        name: Set(view.name.as_str().to_owned()),
        list_view: Set(list_view),
        created_by: Set(id_to_db(view.created_by.as_uuid())),
        created_at: Set(view.created_at.as_millis()),
        archived_at: Set(view.archived_at.map(Timestamp::as_millis)),
    })
}

fn to_saved_view(row: saved_views::Model) -> Result<SavedView, StoreError> {
    let read = RowReader {
        table: TABLE,
        id: &row.id,
    };
    let stored: StoredListView =
        serde_json::from_str(&row.list_view).map_err(|e| read.corrupt(format!("list view: {e}")))?;
    Ok(SavedView {
        id: SavedViewId::from_uuid(read.uuid(&row.id)?),
        heap_id: HeapId::from_uuid(read.uuid(&row.heap_id)?),
        name: Name::parse(&row.name).map_err(|e| read.corrupt(e.to_string()))?,
        list_view: stored
            .to_list_view()
            .map_err(|e| read.corrupt(format!("list view: {e}")))?,
        created_by: UserId::from_uuid(read.uuid(&row.created_by)?),
        created_at: read.timestamp(row.created_at)?,
        archived_at: read.optional_timestamp(row.archived_at)?,
        version: read.version(row.version)?,
    })
}

/// The stored form of a [`ListView`]. Every field is written; the lists may be missing when read.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct StoredListView {
    open: String,
    in_progress: String,
    blocked: String,
    blocked_layout: String,
    sort: String,
    /// Set exactly when `sort` is `parameter`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort_parameter: Option<Uuid>,
    #[serde(default)]
    tags: Vec<Uuid>,
    #[serde(default)]
    effort: Vec<String>,
    #[serde(default)]
    impact: Vec<String>,
    #[serde(default)]
    need: Vec<String>,
    /// Options of custom parameters.
    #[serde(default)]
    options: Vec<Uuid>,
}

impl From<&ListView> for StoredListView {
    fn from(view: &ListView) -> Self {
        let sizes = |sizes: &BTreeSet<Size>| sizes.iter().map(|s| s.code().to_owned()).collect();
        Self {
            open: view.open.code().to_owned(),
            in_progress: view.in_progress.code().to_owned(),
            blocked: view.blocked.code().to_owned(),
            blocked_layout: view.blocked_layout.code().to_owned(),
            sort: view.sort.code().to_owned(),
            sort_parameter: view.sort.parameter().map(|id| *id.as_uuid()),
            tags: view.filter.tags.iter().map(|id| *id.as_uuid()).collect(),
            effort: sizes(&view.filter.effort),
            impact: sizes(&view.filter.impact),
            need: view.filter.need.iter().map(|n| n.code().to_owned()).collect(),
            options: view.filter.options.iter().map(|id| *id.as_uuid()).collect(),
        }
    }
}

impl StoredListView {
    fn to_list_view(&self) -> Result<ListView, String> {
        let sizes = |codes: &[String]| {
            codes
                .iter()
                .map(|code| parse_code(code, Size::from_code))
                .collect::<Result<BTreeSet<_>, _>>()
        };
        Ok(ListView {
            open: parse_code(&self.open, StateMode::from_code)?,
            in_progress: parse_code(&self.in_progress, StateMode::from_code)?,
            blocked: parse_code(&self.blocked, StateMode::from_code)?,
            blocked_layout: parse_code(&self.blocked_layout, BlockedLayout::from_code)?,
            sort: SortKey::from_parts(&self.sort, self.sort_parameter.map(ParameterId::from_uuid))
                .ok_or_else(|| format!("unknown sort {:?} with parameter {:?}", self.sort, self.sort_parameter))?,
            filter: ListFilter {
                tags: self.tags.iter().copied().map(TagId::from_uuid).collect(),
                effort: sizes(&self.effort)?,
                impact: sizes(&self.impact)?,
                need: self
                    .need
                    .iter()
                    .map(|code| parse_code(code, Need::from_code))
                    .collect::<Result<_, _>>()?,
                options: self.options.iter().copied().map(OptionId::from_uuid).collect(),
            },
        })
    }
}

fn parse_code<T>(code: &str, parse: impl Fn(&str) -> Option<T>) -> Result<T, String> {
    parse(code).ok_or_else(|| format!("unknown code {code:?}"))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[gtest]
    fn every_list_view_setting_survives_storage() {
        let view = ListView {
            open: StateMode::Pinned,
            in_progress: StateMode::Hidden,
            blocked: StateMode::Shown,
            blocked_layout: BlockedLayout::Mixed,
            sort: SortKey::Parameter(ParameterId::from_uuid(Uuid::from_u128(9))),
            filter: ListFilter {
                tags: BTreeSet::from([TagId::from_uuid(Uuid::from_u128(7))]),
                effort: BTreeSet::from([Size::S, Size::Xl]),
                impact: BTreeSet::from([Size::M]),
                need: BTreeSet::from([Need::Idea, Need::Critical]),
                options: BTreeSet::from([OptionId::from_uuid(Uuid::from_u128(8))]),
            },
        };

        let json = serde_json::to_string(&StoredListView::from(&view)).expect("serializes");
        let read: StoredListView = serde_json::from_str(&json).expect("parses");

        expect_that!(read.to_list_view(), ok(eq(&view)));
    }

    #[gtest]
    fn missing_filter_lists_read_as_no_filter() {
        let json = r#"{"open":"shown","in_progress":"pinned","blocked":"hidden","blocked_layout":"grouped","sort":"priority"}"#;

        let read: StoredListView = serde_json::from_str(json).expect("parses");

        expect_that!(read.to_list_view(), ok(eq(&ListView::default())));
    }

    #[gtest]
    fn an_unknown_code_is_refused() {
        let json = r#"{"open":"floating","in_progress":"pinned","blocked":"hidden","blocked_layout":"grouped","sort":"priority"}"#;

        let read: StoredListView = serde_json::from_str(json).expect("parses");

        expect_that!(read.to_list_view(), err(anything()));
    }
}
