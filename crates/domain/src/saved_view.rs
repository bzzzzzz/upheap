//! Saved views: named list views of one heap, such as "Ready to pick up" or "Blocked work". Every member of the
//! team sees them, and can open, change and archive them. They point at the heap's items rather than holding any,
//! so a view never becomes a separate backlog.
//!
//! Views are archived rather than deleted, like everything else. An archived view is left out of the heap's list
//! and can't be changed.

use crate::list_view::ListView;
use crate::text::Name;
use crate::{HeapId, SavedViewId, Timestamp, UserId, ValidationError, Version};

/// The most tags a saved view can filter by. Keeps the stored view well inside a MySQL `TEXT` column (64 KB).
pub const MAX_VIEW_TAGS: usize = 200;

/// The most custom parameter options a saved view can filter by, for the same reason.
pub const MAX_VIEW_OPTIONS: usize = 200;

/// Refuses a list view with more than [`MAX_VIEW_TAGS`] tags or [`MAX_VIEW_OPTIONS`] options in its filter.
pub fn ensure_savable(list_view: &ListView) -> Result<(), ValidationError> {
    if list_view.filter.tags.len() > MAX_VIEW_TAGS {
        return Err(ValidationError::TooManyTags { max: MAX_VIEW_TAGS });
    }
    if list_view.filter.options.len() > MAX_VIEW_OPTIONS {
        return Err(ValidationError::TooManyOptions { max: MAX_VIEW_OPTIONS });
    }
    Ok(())
}

/// The fields of a new saved view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewSavedView {
    pub name: Name,
    pub list_view: ListView,
}

/// A partial edit. `None` leaves the field unchanged; a new `list_view` replaces the whole view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SavedViewPatch {
    pub name: Option<Name>,
    pub list_view: Option<ListView>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedView {
    pub id: SavedViewId,
    pub heap_id: HeapId,
    /// Not unique: two views may share a name, as two items may share a title.
    pub name: Name,
    pub list_view: ListView,
    pub created_by: UserId,
    pub created_at: Timestamp,
    /// When the view was archived; `None` while it is in use.
    pub archived_at: Option<Timestamp>,
    pub version: Version,
}

impl SavedView {
    pub fn new(id: SavedViewId, heap_id: HeapId, fields: NewSavedView, created_by: UserId, now: Timestamp) -> Self {
        Self {
            id,
            heap_id,
            name: fields.name,
            list_view: fields.list_view,
            created_by,
            created_at: now,
            archived_at: None,
            version: Version::INITIAL,
        }
    }

    pub const fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }

    /// The view with `patch` applied, or `None` when the patch changes nothing. Keeps the old version; the store
    /// bumps it on write.
    pub fn apply_patch(&self, patch: &SavedViewPatch) -> Option<Self> {
        let edited = Self {
            name: patch.name.clone().unwrap_or_else(|| self.name.clone()),
            list_view: patch.list_view.clone().unwrap_or_else(|| self.list_view.clone()),
            ..self.clone()
        };
        (edited != *self).then_some(edited)
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::TagId;
    use crate::list_view::{SortKey, StateMode};

    fn sample_view() -> SavedView {
        let fields = NewSavedView {
            name: Name::parse("Ready").expect("valid"),
            list_view: ListView::default(),
        };
        SavedView::new(
            SavedViewId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            UserId::from_uuid(Uuid::nil()),
            Timestamp::EPOCH,
        )
    }

    #[gtest]
    fn a_new_view_is_in_use_at_the_first_version() {
        let view = sample_view();

        expect_that!(view.is_archived(), eq(false));
        expect_that!(view.version, eq(Version::INITIAL));
    }

    #[gtest]
    fn a_new_list_view_replaces_the_whole_view() {
        let shown_blocked = ListView {
            blocked: StateMode::Shown,
            sort: SortKey::Due,
            ..ListView::default()
        };
        let patch = SavedViewPatch {
            list_view: Some(shown_blocked.clone()),
            ..SavedViewPatch::default()
        };

        let edited = sample_view().apply_patch(&patch);

        expect_that!(edited.map(|v| v.list_view), some(eq(&shown_blocked)));
    }

    #[gtest]
    fn a_patch_without_changes_is_a_no_op() {
        let patch = SavedViewPatch {
            name: Some(Name::parse(" Ready ").expect("valid")),
            list_view: Some(ListView::default()),
        };

        expect_that!(sample_view().apply_patch(&patch), none());
    }

    #[gtest]
    fn a_view_can_filter_by_at_most_the_maximum_number_of_tags() {
        let with_tags = |count: usize| {
            let mut view = ListView::default();
            view.filter.tags = (0..count as u128)
                .map(|n| TagId::from_uuid(Uuid::from_u128(n)))
                .collect();
            view
        };

        expect_that!(ensure_savable(&with_tags(MAX_VIEW_TAGS)), ok(anything()));
        expect_that!(
            ensure_savable(&with_tags(MAX_VIEW_TAGS + 1)),
            err(eq(&ValidationError::TooManyTags { max: MAX_VIEW_TAGS }))
        );
    }
}
