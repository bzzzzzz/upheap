//! Request types of the question endpoints. They answer with the whole item (`ItemDto`).

use domain::ValidationError;
use domain::question::{NewQuestion, QuestionKind, QuestionText, ReviewInput};
use domain::step::StepNumber;
use domain::text::Context;
use domain::validation::Field;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::common::QuestionKindDto;

/// A question, request, review or done proposal to put on an item. A review and a done proposal are accepted only
/// from an agent doing a task (a run whose turn goes to done); a person asking by hand is `question_needs_do_turn`.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = AskQuestionRequest)]
pub struct AskQuestionDto {
    pub kind: QuestionKindDto,
    /// The short version: 1 to 300 characters, on one line.
    pub text: String,
    /// The long version: Markdown, at most 20,000 characters; blank means none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    /// For a `question`: none, or from 2 to 10 labels to pick from, each 1 to 100 characters on one line, no two the
    /// same ignoring case. Not for a `request`, which always has `Done` and `Can't be done`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choices: Option<Vec<String>>,
    /// Whether any number of choices may be picked. Only with `choices`. Defaults to `false`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub many: Option<bool>,
    /// The number of one of the item's steps this is about.
    #[schema(minimum = 1)]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<u32>,
    /// For a `review`, required: the address of the work to look at, an absolute `http` or `https` link of up to 2,000
    /// characters. Not for any other kind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
    /// For a `review`: the piece of work it is about, one line of 1 to 2,000 characters, which numbers its rounds;
    /// blank or `null` means the link itself. Not for any other kind. The server works out the round: 1 plus the
    /// answered reviews of the same piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub piece: Option<String>,
}

impl AskQuestionDto {
    /// The new question's fields. Fails on the first invalid field. Step numbers start at 1, so a `step` of zero is
    /// `ValueUnknown`, and so is a `kind` this server doesn't know.
    pub fn to_new_question(&self) -> Result<NewQuestion, ValidationError> {
        NewQuestion::new(
            QuestionKind::try_from(self.kind)?,
            QuestionText::parse(&self.text)?,
            self.details
                .as_deref()
                .map(|details| Context::parse_as(details, Field::Details))
                .transpose()?
                .flatten(),
            self.choices.as_deref().unwrap_or_default(),
            self.many.unwrap_or(false),
            self.step
                .map(|number| StepNumber::new(number).ok_or(ValidationError::ValueUnknown { field: Field::Step }))
                .transpose()?,
            ReviewInput {
                link: self.link.as_deref(),
                piece: self.piece.as_deref(),
            },
        )
    }
}

/// A reply to a question.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = QuestionReplyRequest)]
pub struct ReplyDto {
    /// Indices into the question's `choices`: none for a question without choices, one for a request or a question
    /// that takes one pick, one or more when it takes `many`. Ascending or not, without repeats.
    #[serde(default)]
    pub picked: Vec<u32>,
    /// Markdown, at most 20,000 characters. Required for a question without choices; otherwise optional.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// An edit of a question. For now the only edit is the reply.
#[derive(Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
#[schema(as = QuestionPatch)]
pub struct QuestionPatchDto {
    /// The reply. Required: a body without it is `field_empty` on `reply`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyDto>,
}

impl QuestionPatchDto {
    /// The reply's picks and comment. Fails when there is no reply or the comment is too long.
    pub fn to_reply(&self) -> Result<(Vec<u32>, Option<Context>), ValidationError> {
        let reply = self
            .reply
            .as_ref()
            .ok_or(ValidationError::Empty { field: Field::Reply })?;
        let comment = reply
            .comment
            .as_deref()
            .map(|comment| Context::parse_as(comment, Field::Comment))
            .transpose()?
            .flatten();
        Ok((reply.picked.clone(), comment))
    }
}
