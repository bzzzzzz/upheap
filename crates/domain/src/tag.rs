//! Tags group similar items. A tag is a heap's or a team's: a heap's tags are its own and its team's together.
//! They can be renamed and described, but never deleted, and their owner never changes. Taking a tag off an item
//! is an ordinary item edit.

use std::collections::BTreeSet;

use crate::text::{parse_optional, parse_required};
use crate::validation::{Field, ValidationError};
use crate::{HeapId, TagId, TeamId, Timestamp, Version};

/// Most tags one item can carry. Bounds the request size; real items carry a handful.
pub const MAX_TAGS_PER_ITEM: usize = 50;

/// A tag name: trimmed, 1 to 50 characters. Unique among a heap's tags (its own and its team's), ignoring case.
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

/// Who owns a tag: one heap, or a whole team and so all its heaps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagOwner {
    Heap(HeapId),
    Team(TeamId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    pub id: TagId,
    pub owner: TagOwner,
    pub name: TagName,
    pub description: Option<TagDescription>,
    pub created_at: Timestamp,
    pub version: Version,
}

impl Tag {
    pub fn new(id: TagId, owner: TagOwner, fields: NewTag, now: Timestamp) -> Self {
        Self {
            id,
            owner,
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

/// A heap's tag order after `merged` heap tags are merged into the team tag `team_tag`: the team tag takes the
/// position of the highest-ranked of itself and the merged tags the order holds, and the others are removed.
/// `None` when the order holds none of the merged tags, so it needs no write.
pub fn merged_tag_order(order: &[TagId], team_tag: TagId, merged: &BTreeSet<TagId>) -> Option<Vec<TagId>> {
    if !order.iter().any(|tag| merged.contains(tag)) {
        return None;
    }
    let mut placed = false;
    let mut new_order = Vec::with_capacity(order.len());
    for tag in order {
        if *tag == team_tag || merged.contains(tag) {
            if !placed {
                new_order.push(team_tag);
                placed = true;
            }
        } else {
            new_order.push(*tag);
        }
    }
    Some(new_order)
}

/// The text a merge's reach is hashed from: for each heap tag in the reach, by tag ID, `"<tag id>:<items>\n"`, with
/// `items` the number of items that carry it. It changes when a heap tag joins or leaves the reach or a count changes.
pub fn reach_text(entries: impl IntoIterator<Item = (TagId, u32)>) -> String {
    let mut entries: Vec<(TagId, u32)> = entries.into_iter().collect();
    entries.sort_by_key(|(tag, _)| *tag);
    entries
        .into_iter()
        .map(|(tag, items)| format!("{tag}:{items}\n"))
        .collect()
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
            TagOwner::Heap(HeapId::from_uuid(Uuid::nil())),
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

    fn tag_n(n: u128) -> TagId {
        TagId::from_uuid(Uuid::from_u128(n))
    }

    #[gtest]
    fn the_team_tag_takes_the_highest_position_of_the_merged_tags() {
        let merged = BTreeSet::from([tag_n(2), tag_n(4)]);

        let order = merged_tag_order(&[tag_n(1), tag_n(4), tag_n(3), tag_n(2)], tag_n(9), &merged);

        expect_that!(order, some(eq(&vec![tag_n(1), tag_n(9), tag_n(3)])));
    }

    #[gtest]
    fn a_team_tag_already_ranked_keeps_the_higher_position() {
        let merged = BTreeSet::from([tag_n(2)]);

        let before = merged_tag_order(&[tag_n(9), tag_n(1), tag_n(2)], tag_n(9), &merged);
        let after = merged_tag_order(&[tag_n(1), tag_n(2), tag_n(9)], tag_n(9), &merged);

        expect_that!(before, some(eq(&vec![tag_n(9), tag_n(1)])));
        expect_that!(after, some(eq(&vec![tag_n(1), tag_n(9)])));
    }

    #[gtest]
    fn an_order_without_merged_tags_is_left_alone() {
        let merged = BTreeSet::from([tag_n(2)]);

        expect_that!(merged_tag_order(&[tag_n(1), tag_n(9)], tag_n(9), &merged), none());
        expect_that!(merged_tag_order(&[], tag_n(9), &merged), none());
    }

    #[gtest]
    fn the_reach_text_is_the_same_in_any_input_order_and_changes_with_a_count() {
        let one = reach_text([(tag_n(1), 2), (tag_n(2), 0)]);
        let other_order = reach_text([(tag_n(2), 0), (tag_n(1), 2)]);
        let counted = reach_text([(tag_n(1), 3), (tag_n(2), 0)]);

        expect_that!(one, eq(&other_order));
        expect_that!(one, not(eq(&counted)));
    }
}
