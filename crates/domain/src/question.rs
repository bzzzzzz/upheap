//! Questions and requests on an item: asked by someone, replied to by a person.
//!
//! A question is its own row with its own version, so replying never conflicts with editing the item
//! (architecture.md, Data model). Questions are never deleted: an answered one stays, with its reply.

use crate::heap::Heap;
use crate::item::{Item, ItemError};
use crate::step::StepNumber;
use crate::text::{Context, parse_positive, parse_required};
use crate::validation::Field;
use crate::{ItemId, QuestionId, Timestamp, UserId, ValidationError, Version};

/// The most questions one item has, answered ones included. This is the size assumption that lets every response
/// that carries an item carry all of its questions (architecture.md, Data model).
pub const MAX_QUESTIONS: usize = 100;

/// The labels a request is replied to with, in the order `picked` indexes them: Done is 0, Can't be done is 1.
pub const REQUEST_CHOICES: [&str; 2] = ["Done", "Can't be done"];

/// A question's number within its item: given when it is asked, never changed and never reused. A question is named
/// by its item's key and this number, as `k3x9a.q2`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QuestionNumber(u32);

impl QuestionNumber {
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

impl std::fmt::Display for QuestionNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What is being asked: something to answer, or something to do. Reviews come later.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuestionKind {
    /// Free text, or a pick among choices.
    Question,
    /// Something to do, replied to with Done or Can't be done.
    Request,
}

impl QuestionKind {
    pub const ALL: [Self; 2] = [Self::Question, Self::Request];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Request => "request",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }
}

/// Whether a question still waits for a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuestionState {
    Open,
    Answered,
}

impl QuestionState {
    pub const ALL: [Self; 2] = [Self::Open, Self::Answered];

    /// Stable code used in storage and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.code() == code)
    }
}

/// A question's short version: one line, trimmed, 1 to 300 characters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestionText(String);

impl QuestionText {
    pub const MAX_CHARS: usize = 300;

    pub fn parse(raw: &str) -> Result<Self, ValidationError> {
        let text = parse_required(raw, Field::Text, Self::MAX_CHARS)?;
        if has_line_break(&text) {
            return Err(ValidationError::MultipleLines { field: Field::Text });
        }
        Ok(Self(text))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn has_line_break(text: &str) -> bool {
    text.contains(['\n', '\r'])
}

/// The labels a question can be replied to by picking: none, or 2 to 10 given by whoever asked, or for a request
/// the fixed Done and Can't be done.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Choices(Vec<String>);

impl Choices {
    pub const MIN: usize = 2;
    pub const MAX: usize = 10;
    pub const MAX_LABEL_CHARS: usize = 100;

    /// No choices: the reply is free text.
    pub const fn none() -> Self {
        Self(Vec::new())
    }

    /// The choices of a request.
    pub fn request() -> Self {
        Self(REQUEST_CHOICES.iter().map(|label| (*label).to_owned()).collect())
    }

    /// The labels an asker gave: each trimmed, 1 to 100 characters on one line, no two equal ignoring case, and
    /// from 2 to 10 of them.
    pub fn parse(raw: &[String]) -> Result<Self, ValidationError> {
        if !(Self::MIN..=Self::MAX).contains(&raw.len()) {
            return Err(ValidationError::ChoiceCount {
                min: Self::MIN,
                max: Self::MAX,
            });
        }
        let mut labels: Vec<String> = Vec::with_capacity(raw.len());
        for label in raw {
            let label = parse_required(label, Field::Choices, Self::MAX_LABEL_CHARS)?;
            if has_line_break(&label) {
                return Err(ValidationError::MultipleLines { field: Field::Choices });
            }
            if labels.iter().any(|seen| seen.to_lowercase() == label.to_lowercase()) {
                return Err(ValidationError::ChoiceDuplicate);
            }
            labels.push(label);
        }
        Ok(Self(labels))
    }

    /// Labels read back from storage, which were checked when the question was asked.
    pub const fn from_stored(labels: Vec<String>) -> Self {
        Self(labels)
    }

    pub fn labels(&self) -> &[String] {
        &self.0
    }

    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What a person answered: the choices picked, by index, and a comment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// Indices into the question's choices, ascending, without repeats. Empty for a question without choices.
    pub picked: Vec<u32>,
    pub comment: Option<Context>,
    /// `None` when it can't be traced.
    pub replied_by: Option<UserId>,
    pub replied_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Question {
    pub id: QuestionId,
    pub item_id: ItemId,
    pub number: QuestionNumber,
    pub kind: QuestionKind,
    pub text: QuestionText,
    /// The long version.
    pub details: Option<Context>,
    pub choices: Choices,
    /// Whether any number of choices may be picked. Only with choices.
    pub many: bool,
    /// The step of the item it is about, which may have been dropped since.
    pub step: Option<StepNumber>,
    pub asked_by: Option<UserId>,
    pub asked_at: Timestamp,
    /// Set exactly when the question is answered.
    pub reply: Option<Reply>,
    pub version: Version,
}

/// The fields of a question being asked, checked against each other.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewQuestion {
    pub kind: QuestionKind,
    pub text: QuestionText,
    pub details: Option<Context>,
    pub choices: Choices,
    pub many: bool,
    pub step: Option<StepNumber>,
}

impl NewQuestion {
    /// The fields of a question. `choices` is empty for none. A request has the fixed choices and takes neither
    /// `choices` nor `many`; `many` needs choices.
    pub fn new(
        kind: QuestionKind,
        text: QuestionText,
        details: Option<Context>,
        choices: &[String],
        many: bool,
        step: Option<StepNumber>,
    ) -> Result<Self, ValidationError> {
        let choices = match kind {
            QuestionKind::Request => {
                if !choices.is_empty() {
                    return Err(ValidationError::NotAllowed { field: Field::Choices });
                }
                if many {
                    return Err(ValidationError::NotAllowed { field: Field::Many });
                }
                Choices::request()
            }
            QuestionKind::Question if choices.is_empty() => {
                if many {
                    return Err(ValidationError::NotAllowed { field: Field::Many });
                }
                Choices::none()
            }
            QuestionKind::Question => Choices::parse(choices)?,
        };
        Ok(Self {
            kind,
            text,
            details,
            choices,
            many,
            step,
        })
    }
}

/// A rule asking or replying broke.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuestionError {
    #[error("item {item_id} already has {max} questions, the most an item can have")]
    TooMany { item_id: ItemId, max: usize },
    #[error("the item has no step {number}")]
    StepNotFound { number: StepNumber },
    #[error("question {number} is already answered")]
    Answered { number: QuestionNumber },
}

/// Why a reply was refused: the question can't take one, or the reply doesn't fit it.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReplyError {
    #[error(transparent)]
    Question(#[from] QuestionError),
    #[error(transparent)]
    Invalid(#[from] ValidationError),
}

impl Question {
    /// A question asked at `now` as number `number`.
    pub fn new(
        id: QuestionId,
        item_id: ItemId,
        number: QuestionNumber,
        fields: NewQuestion,
        asked_by: UserId,
        now: Timestamp,
    ) -> Self {
        Self {
            id,
            item_id,
            number,
            kind: fields.kind,
            text: fields.text,
            details: fields.details,
            choices: fields.choices,
            many: fields.many,
            step: fields.step,
            asked_by: Some(asked_by),
            asked_at: now,
            reply: None,
            version: Version::INITIAL,
        }
    }

    pub const fn state(&self) -> QuestionState {
        if self.reply.is_some() {
            QuestionState::Answered
        } else {
            QuestionState::Open
        }
    }

    /// This question replied to by `replied_by` at `now`, picking `picked` and saying `comment`. The result keeps
    /// the old version; the store bumps it on write.
    ///
    /// Fails with [`QuestionError::Answered`] when it already has a reply, and with a validation error when the
    /// reply doesn't fit the question: a question without choices takes a comment and no pick; one with choices
    /// takes one pick, or one or more when `many`; a pick is a distinct index of the choices. Picks are kept in
    /// ascending order.
    pub fn reply(
        &self,
        picked: &[u32],
        comment: Option<Context>,
        replied_by: UserId,
        now: Timestamp,
    ) -> Result<Self, ReplyError> {
        if self.reply.is_some() {
            return Err(QuestionError::Answered { number: self.number }.into());
        }
        let picked = self.check_picks(picked)?;
        if self.choices.is_empty() && comment.is_none() {
            return Err(ValidationError::Empty { field: Field::Comment }.into());
        }
        Ok(Self {
            reply: Some(Reply {
                picked,
                comment,
                replied_by: Some(replied_by),
                replied_at: now,
            }),
            ..self.clone()
        })
    }

    fn check_picks(&self, picked: &[u32]) -> Result<Vec<u32>, ValidationError> {
        let choices = self.choices.labels().len();
        if choices == 0 {
            return if picked.is_empty() {
                Ok(Vec::new())
            } else {
                Err(ValidationError::PickOutOfRange)
            };
        }
        if picked.is_empty() {
            return Err(ValidationError::PickNone);
        }
        if picked
            .iter()
            .any(|pick| usize::try_from(*pick).map_or(true, |pick| pick >= choices))
        {
            return Err(ValidationError::PickOutOfRange);
        }
        let mut picked = picked.to_vec();
        picked.sort_unstable();
        if picked.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(ValidationError::PickRepeated);
        }
        if picked.len() > 1 && !self.many {
            return Err(ValidationError::PickTooMany);
        }
        Ok(picked)
    }
}

/// Whether `item` can be asked questions or replied to: not once it is closed. Reopening makes its open questions
/// waiting again, unchanged.
pub const fn check_questions_editable(item: &Item) -> Result<(), ItemError> {
    if item.is_closed() {
        return Err(ItemError::Closed { item_id: item.id });
    }
    Ok(())
}

/// Whether `item` waits for the person: it is open, its heap is active and it has an open question, whoever asked it
/// (Waiting for me). The heap's other rules, such as the role, don't matter: any member sees it waiting.
pub const fn waits_for_you(item: &Item, heap: &Heap, open_questions: usize) -> bool {
    !item.is_closed() && !heap.is_archived() && open_questions > 0
}

/// Checks that `fields` can be asked on `item`, whose questions are `questions` and whose steps are numbered
/// `step_numbers`, and says the new question's number: one more than the highest the item has had, so numbers are
/// never reused.
pub fn number_question(
    item: &Item,
    questions: &[Question],
    step_numbers: &[StepNumber],
    fields: &NewQuestion,
) -> Result<QuestionNumber, QuestionError> {
    if questions.len() >= MAX_QUESTIONS {
        return Err(QuestionError::TooMany {
            item_id: item.id,
            max: MAX_QUESTIONS,
        });
    }
    if let Some(step) = fields.step
        && !step_numbers.contains(&step)
    {
        return Err(QuestionError::StepNotFound { number: step });
    }
    Ok(questions
        .iter()
        .map(|question| question.number)
        .max()
        .map_or(QuestionNumber::FIRST, QuestionNumber::next))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::HeapId;
    use crate::handle::ItemKey;
    use crate::item::{ItemState, Kind, Need, NewItem, Title};

    fn sample_item(closed: bool) -> Item {
        let fields = NewItem {
            title: Title::parse("Rotate the certs").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
        };
        let mut item = Item::new(
            ItemId::from_uuid(Uuid::from_u128(3)),
            HeapId::from_uuid(Uuid::from_u128(2)),
            ItemKey::parse("k3x9a").expect("valid"),
            fields,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        );
        item.kind = Kind::Task;
        if closed {
            item.state = ItemState::Done {
                closed_at: Timestamp::EPOCH,
                closed_by: None,
            };
        }
        item
    }

    fn labels(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_owned()).collect()
    }

    fn text(raw: &str) -> QuestionText {
        QuestionText::parse(raw).expect("valid")
    }

    fn fields(kind: QuestionKind, choices: &[&str], many: bool) -> std::result::Result<NewQuestion, ValidationError> {
        NewQuestion::new(kind, text("Which CA?"), None, &labels(choices), many, None)
    }

    fn question(kind: QuestionKind, choices: &[&str], many: bool) -> Question {
        Question::new(
            QuestionId::from_uuid(Uuid::from_u128(7)),
            ItemId::from_uuid(Uuid::from_u128(3)),
            QuestionNumber::FIRST,
            fields(kind, choices, many).expect("valid"),
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        )
    }

    fn reply(question: &Question, picked: &[u32], note: &str) -> std::result::Result<Question, ReplyError> {
        question.reply(
            picked,
            Context::parse(note).expect("valid"),
            UserId::from_uuid(Uuid::from_u128(11)),
            Timestamp::EPOCH,
        )
    }

    fn invalid(error: ValidationError) -> ReplyError {
        ReplyError::Invalid(error)
    }

    fn picked_of(question: &Question) -> Option<Vec<u32>> {
        question.reply.as_ref().map(|reply| reply.picked.clone())
    }

    #[gtest]
    fn text_is_trimmed_one_line_and_at_most_300_characters() {
        expect_that!(
            QuestionText::parse("  Which CA? ").map(|t| t.as_str().to_owned()),
            ok(eq("Which CA?"))
        );
        expect_that!(
            QuestionText::parse("  "),
            err(eq(&ValidationError::Empty { field: Field::Text }))
        );
        expect_that!(QuestionText::parse(&"x".repeat(300)), ok(anything()));
        expect_that!(
            QuestionText::parse(&"x".repeat(301)),
            err(eq(&ValidationError::TooLong {
                field: Field::Text,
                max: 300
            }))
        );
        expect_that!(
            QuestionText::parse("one\ntwo"),
            err(eq(&ValidationError::MultipleLines { field: Field::Text }))
        );
        expect_that!(
            QuestionText::parse("one\rtwo"),
            err(eq(&ValidationError::MultipleLines { field: Field::Text }))
        );
    }

    #[gtest]
    fn choices_are_two_to_ten_unique_single_lines() {
        let count = ValidationError::ChoiceCount { min: 2, max: 10 };
        expect_that!(Choices::parse(&labels(&["Only"])), err(eq(&count)));
        let eleven: Vec<String> = (0..11).map(|n| format!("c{n}")).collect();
        expect_that!(Choices::parse(&eleven), err(eq(&count)));
        let ten: Vec<String> = (0..10).map(|n| format!("c{n}")).collect();
        expect_that!(Choices::parse(&ten), ok(anything()));
        expect_that!(
            Choices::parse(&labels(&["Yes", "yes"])),
            err(eq(&ValidationError::ChoiceDuplicate))
        );
        expect_that!(
            Choices::parse(&labels(&["Yes", &"x".repeat(101)])),
            err(eq(&ValidationError::TooLong {
                field: Field::Choices,
                max: 100
            }))
        );
        expect_that!(
            Choices::parse(&labels(&["Yes", "a\nb"])),
            err(eq(&ValidationError::MultipleLines { field: Field::Choices }))
        );
        expect_that!(
            Choices::parse(&labels(&["Yes", " "])),
            err(eq(&ValidationError::Empty { field: Field::Choices }))
        );
        expect_that!(
            Choices::parse(&labels(&[" Yes ", "No"])).map(|c| c.labels().to_vec()),
            ok(eq(&labels(&["Yes", "No"])))
        );
    }

    #[gtest]
    fn a_request_has_the_fixed_choices_and_takes_none_of_its_own() {
        let request = fields(QuestionKind::Request, &[], false).expect("valid");
        expect_that!(request.choices.labels(), eq(&labels(&["Done", "Can't be done"])));
        expect_that!(
            fields(QuestionKind::Request, &["A", "B"], false),
            err(eq(&ValidationError::NotAllowed { field: Field::Choices }))
        );
        expect_that!(
            fields(QuestionKind::Request, &[], true),
            err(eq(&ValidationError::NotAllowed { field: Field::Many }))
        );
    }

    #[gtest]
    fn many_needs_choices() {
        expect_that!(
            fields(QuestionKind::Question, &[], true),
            err(eq(&ValidationError::NotAllowed { field: Field::Many }))
        );
        expect_that!(fields(QuestionKind::Question, &["A", "B"], true), ok(anything()));
    }

    #[gtest]
    fn a_question_without_choices_needs_a_comment_and_no_pick() {
        let open = question(QuestionKind::Question, &[], false);

        let answered = reply(&open, &[], " Internal ").expect("valid");
        expect_that!(answered.state(), eq(QuestionState::Answered));
        expect_that!(picked_of(&answered), some(is_empty()));
        expect_that!(
            reply(&open, &[], "  "),
            err(eq(&invalid(ValidationError::Empty { field: Field::Comment })))
        );
        expect_that!(
            reply(&open, &[0], "x"),
            err(eq(&invalid(ValidationError::PickOutOfRange)))
        );
    }

    #[gtest]
    fn a_question_with_choices_takes_one_pick_and_an_optional_comment() {
        let open = question(QuestionKind::Question, &["A", "B"], false);

        let answered = reply(&open, &[1], "").expect("valid");
        expect_that!(picked_of(&answered), some(eq(&vec![1])));
        expect_that!(answered.reply.and_then(|r| r.comment), none());
        expect_that!(reply(&open, &[], ""), err(eq(&invalid(ValidationError::PickNone))));
        expect_that!(
            reply(&open, &[0, 1], ""),
            err(eq(&invalid(ValidationError::PickTooMany)))
        );
        expect_that!(
            reply(&open, &[2], ""),
            err(eq(&invalid(ValidationError::PickOutOfRange)))
        );
        expect_that!(
            reply(&open, &[0, 0], ""),
            err(eq(&invalid(ValidationError::PickRepeated)))
        );
    }

    #[gtest]
    fn a_many_question_takes_one_or_more_picks_kept_ascending() {
        let open = question(QuestionKind::Question, &["A", "B", "C"], true);

        let answered = reply(&open, &[2, 0], "").expect("valid");
        expect_that!(picked_of(&answered), some(eq(&vec![0, 2])));
        expect_that!(reply(&open, &[0, 1, 2], ""), ok(anything()));
        expect_that!(reply(&open, &[], ""), err(eq(&invalid(ValidationError::PickNone))));
        expect_that!(
            reply(&open, &[1, 1], ""),
            err(eq(&invalid(ValidationError::PickRepeated)))
        );
    }

    #[gtest]
    fn a_request_is_replied_to_with_done_or_cant_be_done() {
        let open = question(QuestionKind::Request, &[], false);

        expect_that!(reply(&open, &[0], ""), ok(anything()));
        expect_that!(reply(&open, &[1], "No access"), ok(anything()));
        expect_that!(reply(&open, &[], ""), err(eq(&invalid(ValidationError::PickNone))));
        expect_that!(
            reply(&open, &[0, 1], ""),
            err(eq(&invalid(ValidationError::PickTooMany)))
        );
        expect_that!(
            reply(&open, &[2], ""),
            err(eq(&invalid(ValidationError::PickOutOfRange)))
        );
    }

    #[gtest]
    fn an_answered_question_is_not_replied_to_again() {
        let answered = reply(&question(QuestionKind::Question, &[], false), &[], "x").expect("valid");

        expect_that!(
            reply(&answered, &[], "y"),
            err(eq(&ReplyError::Question(QuestionError::Answered {
                number: QuestionNumber::FIRST
            })))
        );
    }

    #[gtest]
    fn the_next_number_is_one_past_the_highest() {
        let item = sample_item(false);
        let new = fields(QuestionKind::Question, &[], false).expect("valid");
        let mut fourth = question(QuestionKind::Question, &[], false);
        fourth.number = QuestionNumber::new(4).expect("positive");

        expect_that!(number_question(&item, &[], &[], &new), ok(eq(QuestionNumber::FIRST)));
        expect_that!(
            number_question(&item, &[fourth], &[], &new),
            ok(eq(QuestionNumber::new(5).expect("positive")))
        );
    }

    #[gtest]
    fn an_item_has_at_most_100_questions() {
        let item = sample_item(false);
        let new = fields(QuestionKind::Question, &[], false).expect("valid");
        let questions: Vec<Question> = (0..MAX_QUESTIONS)
            .map(|_| question(QuestionKind::Question, &[], false))
            .collect();

        expect_that!(
            number_question(&item, &questions, &[], &new),
            err(eq(QuestionError::TooMany {
                item_id: item.id,
                max: 100
            }))
        );
        expect_that!(number_question(&item, &questions[1..], &[], &new), ok(anything()));
    }

    #[gtest]
    fn a_question_names_a_step_the_item_has() {
        let item = sample_item(false);
        let step = StepNumber::new(2).expect("positive");
        let mut new = fields(QuestionKind::Question, &[], false).expect("valid");
        new.step = Some(step);

        expect_that!(number_question(&item, &[], &[step], &new), ok(anything()));
        expect_that!(
            number_question(&item, &[], &[], &new),
            err(eq(QuestionError::StepNotFound { number: step }))
        );
    }

    #[gtest]
    fn a_closed_item_takes_no_questions() {
        expect_that!(check_questions_editable(&sample_item(false)), ok(anything()));
        expect_that!(check_questions_editable(&sample_item(true)), err(anything()));
    }

    #[gtest]
    fn kinds_and_states_round_trip_through_their_codes() {
        for kind in QuestionKind::ALL {
            expect_that!(QuestionKind::from_code(kind.code()), some(eq(kind)));
        }
        for state in QuestionState::ALL {
            expect_that!(QuestionState::from_code(state.code()), some(eq(state)));
        }
        expect_that!(QuestionKind::from_code("review"), none());
    }

    #[gtest]
    fn numbers_are_positive_digits() {
        expect_that!(
            QuestionNumber::parse("2"),
            some(eq(QuestionNumber::new(2).expect("positive")))
        );
        expect_that!(QuestionNumber::parse("0"), none());
        expect_that!(QuestionNumber::parse("02"), none());
        expect_that!(QuestionNumber::parse("-1"), none());
    }
}
