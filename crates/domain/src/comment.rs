//! Comments on an item: an append-only thread under its context.
//!
//! A comment is its own row with no version: it is added and never changed or deleted, and adding one never touches
//! the item's version, so an agent or a person can add to an item without rewriting its context or conflicting with
//! an edit (architecture.md, Data model).

use crate::text::{Context, parse_positive, parse_required};
use crate::validation::Field;
use crate::{CommentId, ItemId, Timestamp, UserId, ValidationError};

/// The most comments one item has.
///
/// Size assumption: an item response carries every comment, so the most one can hold is 200 comments of 20,000
/// characters each. Raising this means paging comments.
pub const MAX_COMMENTS: usize = 200;

/// A comment's number within its item: given when it is added, never changed and never reused. A comment is named by
/// its item's key and this number, as `k3x9a.c3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CommentNumber(u32);

impl CommentNumber {
    pub const FIRST: Self = Self(1);

    /// `None` for zero.
    pub const fn new(value: u32) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    /// A positive integer written in digits, as in a path. `None` for anything else, a sign and a leading zero
    /// included.
    pub fn parse(raw: &str) -> Option<Self> {
        parse_positive(raw).and_then(Self::new)
    }

    pub const fn get(self) -> u32 {
        self.0
    }

    /// The number after this one. Saturates instead of wrapping, which no real item reaches.
    const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl std::fmt::Display for CommentNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What a comment says: plain text, trimmed, 1 to 20,000 characters, line breaks kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommentBody(String);

impl CommentBody {
    /// The same limit as [`Context`], whose text this sits beside.
    pub const MAX_CHARS: usize = Context::MAX_CHARS;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        parse_required(raw, Field::Body, Self::MAX_CHARS).map(Self)
    }

    /// A body read back from storage, which was checked when the comment was added.
    pub const fn from_stored(text: String) -> Self {
        Self(text)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Comment {
    pub id: CommentId,
    pub item_id: ItemId,
    pub number: CommentNumber,
    pub body: CommentBody,
    pub created_by: UserId,
    pub created_at: Timestamp,
}

impl Comment {
    /// A comment added at `now` as number `number`.
    pub const fn new(
        id: CommentId,
        item_id: ItemId,
        number: CommentNumber,
        body: CommentBody,
        created_by: UserId,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            item_id,
            number,
            body,
            created_by,
            created_at: now,
        }
    }
}

/// A rule adding a comment broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CommentError {
    #[error("the item already has {max} comments, the most an item can have")]
    TooMany { max: usize },
}

/// Says the number of the next comment of an item whose comments are `existing`: one more than the highest it has
/// had, so numbers are never reused. Fails when the item is at [`MAX_COMMENTS`].
pub fn number_comment(existing: &[Comment]) -> Result<CommentNumber, CommentError> {
    if existing.len() >= MAX_COMMENTS {
        return Err(CommentError::TooMany { max: MAX_COMMENTS });
    }
    Ok(existing
        .iter()
        .map(|comment| comment.number)
        .max()
        .map_or(CommentNumber::FIRST, CommentNumber::next))
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;

    fn comment(number: u32) -> Comment {
        Comment::new(
            CommentId::from_uuid(Uuid::from_u128(u128::from(number))),
            ItemId::from_uuid(Uuid::from_u128(3)),
            CommentNumber::new(number).expect("positive"),
            CommentBody::parse("said").expect("valid"),
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        )
    }

    #[gtest]
    fn a_body_is_trimmed_and_keeps_its_line_breaks() {
        expect_that!(
            CommentBody::parse("  one\ntwo \n").map(|body| body.as_str().to_owned()),
            ok(eq("one\ntwo"))
        );
    }

    #[gtest]
    fn a_blank_body_is_empty() {
        expect_that!(
            CommentBody::parse(" \n\t "),
            err(eq(&ValidationError::Empty { field: Field::Body }))
        );
    }

    #[gtest]
    fn a_body_is_at_most_20000_characters() {
        expect_that!(CommentBody::parse(&"é".repeat(20_000)), ok(anything()));
        expect_that!(
            CommentBody::parse(&"x".repeat(20_001)),
            err(eq(&ValidationError::TooLong {
                field: Field::Body,
                max: 20_000
            }))
        );
    }

    #[gtest]
    fn the_first_comment_is_one_and_the_next_follows_the_highest() {
        expect_that!(number_comment(&[]), ok(eq(CommentNumber::FIRST)));
        expect_that!(
            number_comment(&[comment(1), comment(2)]).map(CommentNumber::get),
            ok(eq(3))
        );
        expect_that!(number_comment(&[comment(5)]).map(CommentNumber::get), ok(eq(6)));
    }

    #[gtest]
    fn the_201st_comment_is_refused() {
        let at_the_limit: Vec<Comment> = (1..=200).map(comment).collect();
        expect_that!(
            number_comment(&at_the_limit),
            err(eq(CommentError::TooMany { max: MAX_COMMENTS }))
        );
        expect_that!(
            number_comment(&at_the_limit[..199]).map(CommentNumber::get),
            ok(eq(200))
        );
    }

    #[gtest]
    fn a_number_parses_from_digits_only() {
        expect_that!(CommentNumber::parse("3").map(CommentNumber::get), some(eq(3)));
        expect_that!(CommentNumber::parse("0"), none());
        expect_that!(CommentNumber::parse("03"), none());
        expect_that!(CommentNumber::parse("+3"), none());
    }
}
