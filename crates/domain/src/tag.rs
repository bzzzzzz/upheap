//! Tags group similar items within one heap. They can be renamed and described, but never deleted. Taking a
//! tag off an item is an ordinary item edit.

use std::collections::BTreeSet;

use crate::text::{parse_optional, parse_required};
use crate::validation::{Field, ValidationError};
use crate::{HeapId, TagId, Timestamp, Version};

/// Most tags one item can carry. Bounds the request size; real items carry a handful.
pub const MAX_TAGS_PER_ITEM: usize = 50;

/// A tag name: trimmed, 1 to 50 characters. Unique per heap, ignoring case.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagName(String);

impl TagName {
    pub const MAX_CHARS: usize = 50;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Name, Self::MAX_CHARS).map(Self)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The form uniqueness is checked on: lowercase, so "Infra" and "infra" are the same tag.
    ///
    /// Stored next to the name rather than left to the database collation, because MySQL ignores case by
    /// default and SQLite doesn't, and both must behave the same. The store compares the key exactly, so
    /// "resume" and "résumé" stay two tags.
    pub fn to_key(&self) -> String {
        self.0.to_lowercase()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagDescription(String);

impl TagDescription {
    pub const MAX_CHARS: usize = 1_000;

    /// Blank input means "no description" and gives `None`.
    pub fn parse(raw: &str) -> Result<Option<Self>, ValidationError> {
        Ok(parse_optional(raw, Field::Description, Self::MAX_CHARS)?.map(Self))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The fields of a new tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewTag {
    pub name: TagName,
    pub description: Option<TagDescription>,
}

/// A partial edit. An outer `None` leaves the field unchanged; `Some(None)` clears the description.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TagPatch {
    pub name: Option<TagName>,
    pub description: Option<Option<TagDescription>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub id: TagId,
    pub heap_id: HeapId,
    pub name: TagName,
    pub description: Option<TagDescription>,
    pub created_at: Timestamp,
    pub version: Version,
}

impl Tag {
    pub fn new(id: TagId, heap_id: HeapId, fields: NewTag, now: Timestamp) -> Self {
        Self {
            id,
            heap_id,
            name: fields.name,
            description: fields.description,
            created_at: now,
            version: Version::INITIAL,
        }
    }

    /// The tag with `patch` applied, or `None` when the patch changes nothing. Keeps the old version; the
    /// store bumps it on write.
    pub fn apply_patch(&self, patch: &TagPatch) -> Option<Self> {
        let edited = Self {
            name: patch.name.clone().unwrap_or_else(|| self.name.clone()),
            description: patch.description.clone().unwrap_or_else(|| self.description.clone()),
            ..self.clone()
        };
        (edited != *self).then_some(edited)
    }
}

/// The set of tags on an item. Duplicates collapse; more than [`MAX_TAGS_PER_ITEM`] is refused.
pub fn parse_tag_set(ids: impl IntoIterator<Item = TagId>) -> Result<BTreeSet<TagId>, ValidationError> {
    let set: BTreeSet<TagId> = ids.into_iter().collect();
    if set.len() > MAX_TAGS_PER_ITEM {
        return Err(ValidationError::TooManyTags { max: MAX_TAGS_PER_ITEM });
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn sample_tag() -> Tag {
        let fields = NewTag {
            name: TagName::parse("Infra").expect("valid"),
            description: None,
        };
        Tag::new(
            TagId::from_uuid(Uuid::nil()),
            HeapId::from_uuid(Uuid::nil()),
            fields,
            Timestamp::from_millis(0).expect("in range"),
        )
    }

    #[gtest]
    fn names_are_trimmed_and_keyed_without_case() {
        let name = TagName::parse("  On-Call ").expect("valid");

        expect_that!(name.as_str(), eq("On-Call"));
        expect_that!(name.to_key(), eq("on-call"));
    }

    #[gtest]
    fn blank_and_long_names_are_refused() {
        expect_that!(TagName::parse("   "), err(anything()));
        expect_that!(TagName::parse(&"x".repeat(TagName::MAX_CHARS + 1)), err(anything()));
    }

    #[gtest]
    fn renaming_changes_the_tag() {
        let patch = TagPatch {
            name: Some(TagName::parse("Platform").expect("valid")),
            ..TagPatch::default()
        };

        let renamed = sample_tag().apply_patch(&patch);

        expect_that!(
            renamed.map(|t| t.name),
            some(eq(&TagName::parse("Platform").expect("valid")))
        );
    }

    #[gtest]
    fn patch_without_changes_is_a_no_op() {
        let patch = TagPatch {
            name: Some(TagName::parse("Infra").expect("valid")),
            description: Some(None),
        };

        expect_that!(sample_tag().apply_patch(&patch), none());
    }

    #[gtest]
    fn tag_sets_collapse_duplicates_and_are_bounded() {
        let id = TagId::from_uuid(Uuid::nil());
        let many = (0..=MAX_TAGS_PER_ITEM as u128).map(|n| TagId::from_uuid(Uuid::from_u128(n)));

        expect_that!(parse_tag_set([id, id]).map(|s| s.len()), ok(eq(&1)));
        expect_that!(parse_tag_set(many), err(anything()));
    }
}
