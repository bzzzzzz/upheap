//! Questions and requests on an item: asked by someone, replied to by a person.
//!
//! A question is its own row with its own version, so replying never conflicts with editing the item
//! (architecture.md, Data model). Questions are never deleted: an answered one stays, with its reply, and one
//! withdrawn when its item was closed stays, withdrawn.

use crate::heap::Heap;
use crate::item::{Item, ItemError, Kind};
use crate::run::RunLink;
use crate::step::{Step, StepNumber, StepState};
use crate::text::{Context, parse_optional, parse_positive, parse_required};
use crate::turn::Goal;
use crate::validation::Field;
use crate::{ItemId, QuestionId, RunId, Timestamp, UserId, ValidationError, Version};

/// The most questions one item has, answered ones included. This is the size assumption that lets every response
/// that carries an item carry all of its questions (architecture.md, Data model).
pub const MAX_QUESTIONS: usize = 100;

/// The labels a request is replied to with, in the order `picked` indexes them: Done is 0, Can't be done is 1.
pub const REQUEST_CHOICES: [&str; 2] = ["Done", "Can't be done"];

/// The labels a review is replied to with: Approved is 0, Changes asked is 1.
pub const REVIEW_CHOICES: [&str; 2] = ["Approved", "Changes asked"];

/// The labels a done proposal is replied to with: Close as done is 0, Not yet is 1.
pub const DONE_PROPOSAL_CHOICES: [&str; 2] = ["Close as done", "Not yet"];

/// The index of the reply that agrees: Done on a request, Approved on a review, Close as done on a done proposal.
pub const AGREE_PICK: u32 = 0;

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

/// What is being asked: something to answer, something to do, work to look at, or a proposal to close the task.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuestionKind {
    /// Free text, or a pick among choices.
    Question,
    /// Something to do, replied to with Done or Can't be done.
    Request,
    /// A link to work for the person to look at, replied to with Approved or Changes asked. Only an agent doing the
    /// task asks it.
    Review,
    /// The agent says nothing is left, replied to with Close as done or Not yet. Only an agent doing the task asks
    /// it.
    DoneProposal,
}

impl QuestionKind {
    pub const ALL: [Self; 4] = [Self::Question, Self::Request, Self::Review, Self::DoneProposal];

    /// Stable code used in storage, audit diffs and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Request => "request",
            Self::Review => "review",
            Self::DoneProposal => "done_proposal",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.code() == code)
    }

    /// The kind in a word, as a message names a question of it: "review q4".
    pub const fn word(self) -> &'static str {
        match self {
            Self::Question => "question",
            Self::Request => "request",
            Self::Review => "review",
            Self::DoneProposal => "proposal",
        }
    }
}

/// Whether a question still waits for a reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuestionState {
    Open,
    Answered,
    /// Closing its item took it back, unanswered. Reopening the item doesn't bring it back.
    Withdrawn,
}

impl QuestionState {
    pub const ALL: [Self; 3] = [Self::Open, Self::Answered, Self::Withdrawn];

    /// Stable code used in storage and the API. Never renamed.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Answered => "answered",
            Self::Withdrawn => "withdrawn",
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

/// The labels a question can be replied to by picking: none, or 2 to 10 given by whoever asked, or for a request, a
/// review or a done proposal the fixed two of its kind.
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

    /// The choices of a request, a review or a done proposal, which are fixed; `None` for a question, whose asker
    /// gives them.
    pub fn fixed(kind: QuestionKind) -> Option<Self> {
        let labels = match kind {
            QuestionKind::Question => return None,
            QuestionKind::Request => REQUEST_CHOICES,
            QuestionKind::Review => REVIEW_CHOICES,
            QuestionKind::DoneProposal => DONE_PROPOSAL_CHOICES,
        };
        Some(Self(labels.iter().map(|label| (*label).to_owned()).collect()))
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

/// The piece of work a review is about, which numbers its rounds: trimmed, one line, 1 to 2,000 characters. When the
/// asker names none, it is the review's link, exactly as given.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewPiece(String);

impl ReviewPiece {
    pub const MAX_CHARS: usize = 2_000;

    /// `None` for a blank text, which means no piece was given.
    pub fn parse(raw: &str) -> Result<Option<Self>, ValidationError> {
        let Some(text) = parse_optional(raw, Field::Piece, Self::MAX_CHARS)? else {
            return Ok(None);
        };
        if has_line_break(&text) {
            return Err(ValidationError::MultipleLines { field: Field::Piece });
        }
        Ok(Some(Self(text)))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a review asks about, as asked: a link and the piece of work it is a look at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewTarget {
    pub link: RunLink,
    pub piece: ReviewPiece,
}

/// What a review asks about, with its round. Set exactly on a question of kind [`QuestionKind::Review`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewOf {
    pub link: RunLink,
    pub piece: ReviewPiece,
    /// 1 plus the answered reviews of the same piece the item had when this was asked. Fixed then, so later events
    /// never change it.
    pub round: u32,
}

/// The link and piece an asker sent, before they are checked against the kind.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReviewInput<'a> {
    pub link: Option<&'a str>,
    pub piece: Option<&'a str>,
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

/// When and by whom a question was withdrawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Withdrawal {
    /// `None` when it can't be traced.
    pub withdrawn_by: Option<UserId>,
    pub withdrawn_at: Timestamp,
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
    /// Set exactly for a review.
    pub review: Option<ReviewOf>,
    pub asked_by: Option<UserId>,
    pub asked_at: Timestamp,
    /// Set exactly when the question is answered.
    pub reply: Option<Reply>,
    /// Set exactly when the question is withdrawn. A question has a reply or a withdrawal, never both.
    pub withdrawal: Option<Withdrawal>,
    /// The run it was asked in, when an agent asked it: taking the item back withdraws the open questions of its
    /// runs, and the person's own stay open.
    pub run_id: Option<RunId>,
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
    /// Set exactly for a review.
    pub review: Option<ReviewTarget>,
}

impl NewQuestion {
    /// The fields of a question. `choices` is empty for none. A request, a review and a done proposal have fixed
    /// choices and take neither `choices` nor `many`; `many` needs choices. A review and a done proposal are about
    /// the whole task, so they take no `step`. A review needs a link and may name a piece; no other kind takes
    /// either.
    pub fn new(
        kind: QuestionKind,
        text: QuestionText,
        details: Option<Context>,
        choices: &[String],
        many: bool,
        step: Option<StepNumber>,
        review: ReviewInput<'_>,
    ) -> Result<Self, ValidationError> {
        let choices = match Choices::fixed(kind) {
            Some(fixed) => {
                if !choices.is_empty() {
                    return Err(ValidationError::NotAllowed { field: Field::Choices });
                }
                if many {
                    return Err(ValidationError::NotAllowed { field: Field::Many });
                }
                fixed
            }
            None if choices.is_empty() => {
                if many {
                    return Err(ValidationError::NotAllowed { field: Field::Many });
                }
                Choices::none()
            }
            None => Choices::parse(choices)?,
        };
        let about_the_task = matches!(kind, QuestionKind::Review | QuestionKind::DoneProposal);
        if about_the_task && step.is_some() {
            return Err(ValidationError::NotAllowed { field: Field::Step });
        }
        let review = match (kind, review.link) {
            (QuestionKind::Review, Some(link)) => {
                let link = RunLink::parse_as(link, Field::Link)?;
                let piece = match review.piece.map(ReviewPiece::parse).transpose()?.flatten() {
                    Some(piece) => piece,
                    None => ReviewPiece(link.as_str().to_owned()),
                };
                Some(ReviewTarget { link, piece })
            }
            (QuestionKind::Review, None) => return Err(ValidationError::Empty { field: Field::Link }),
            (_, Some(_)) => return Err(ValidationError::NotAllowed { field: Field::Link }),
            (_, None) if review.piece.is_some() => return Err(ValidationError::NotAllowed { field: Field::Piece }),
            (_, None) => None,
        };
        Ok(Self {
            kind,
            text,
            details,
            choices,
            many,
            step,
            review,
        })
    }
}

/// Something that stops a task being closed on a done proposal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseBlocker {
    /// A question of any kind is still open.
    OpenQuestion { number: QuestionNumber, kind: QuestionKind },
    /// Steps are still open: neither ticked nor dropped.
    OpenSteps { count: usize },
    /// The latest round of a reviewed piece is not answered Approved.
    NotApproved { piece: String, round: u32 },
}

impl std::fmt::Display for CloseBlocker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OpenQuestion { number, kind } => write!(f, "{} q{number} is still open", kind.word()),
            Self::OpenSteps { count: 1 } => f.write_str("1 open step"),
            Self::OpenSteps { count } => write!(f, "{count} open steps"),
            Self::NotApproved { piece, round } => write!(f, "the piece {piece} has round {round} not approved"),
        }
    }
}

fn list_blockers(blockers: &[CloseBlocker]) -> String {
    blockers.iter().map(ToString::to_string).collect::<Vec<_>>().join("; ")
}

/// A rule asking or replying broke.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum QuestionError {
    #[error("item {item_id} already has {max} questions, the most an item can have")]
    TooMany { item_id: ItemId, max: usize },
    #[error("the item has no step {number}")]
    StepNotFound { number: StepNumber },
    #[error("question {number} is already answered")]
    Answered { number: QuestionNumber },
    #[error("question {number} was withdrawn when its item was closed")]
    Withdrawn { number: QuestionNumber },
    /// A review or a done proposal on an item that is not a task.
    #[error("item {item_id} is not a task; only a task is reviewed or proposed closed")]
    NotOnTask { item_id: ItemId },
    /// A review or a done proposal asked outside a run, or in a run whose turn does not go to done.
    #[error("only an agent doing the task asks for review or proposes closing it")]
    NeedsDoTurn { item_id: ItemId },
    /// A second review of a piece that has one open.
    #[error("the piece {piece} already has a review open")]
    ReviewOpen { piece: String },
    #[error("Not ready to close: {}.", list_blockers(.0))]
    NotReadyToClose(Vec<CloseBlocker>),
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
            review: fields.review.map(|target| ReviewOf {
                link: target.link,
                piece: target.piece,
                round: 1,
            }),
            asked_by: Some(asked_by),
            asked_at: now,
            reply: None,
            withdrawal: None,
            run_id: None,
            version: Version::INITIAL,
        }
    }

    /// This question as asked in `run`, when it was asked in one.
    #[must_use]
    pub fn asked_in(self, run: Option<RunId>) -> Self {
        Self { run_id: run, ..self }
    }

    /// This question as the review of round `round`. Only a review has a round: any other question is unchanged.
    #[must_use]
    pub fn in_round(self, round: u32) -> Self {
        Self {
            review: self.review.map(|review| ReviewOf { round, ..review }),
            ..self
        }
    }

    /// Whether this is a review that was replied to with Approved.
    pub fn is_approved_review(&self) -> bool {
        self.review.is_some()
            && self
                .reply
                .as_ref()
                .is_some_and(|reply| reply.picked.as_slice() == [AGREE_PICK])
    }

    pub const fn state(&self) -> QuestionState {
        if self.reply.is_some() {
            QuestionState::Answered
        } else if self.withdrawal.is_some() {
            QuestionState::Withdrawn
        } else {
            QuestionState::Open
        }
    }

    /// This question withdrawn by `by` at `now`. The result keeps the old version; the store bumps it on write.
    /// Only an open question is withdrawn: callers pick those.
    #[must_use]
    pub fn withdraw(&self, by: UserId, now: Timestamp) -> Self {
        Self {
            withdrawal: Some(Withdrawal {
                withdrawn_by: Some(by),
                withdrawn_at: now,
            }),
            ..self.clone()
        }
    }

    /// This question replied to by `replied_by` at `now`, picking `picked` and saying `comment`. The result keeps
    /// the old version; the store bumps it on write.
    ///
    /// Fails with [`QuestionError::Answered`] when it already has a reply, with [`QuestionError::Withdrawn`] when it
    /// was withdrawn, and with a validation error when the
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
        if self.withdrawal.is_some() {
            return Err(QuestionError::Withdrawn { number: self.number }.into());
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

/// Whether `item` waits for the person: it is open, its heap is active and it has an open question, whoever asked it,
/// its open turn is `stuck`, or its blocker is gone, closed or moved away ([`crate::item::Item::blocker_gone`]), so
/// someone has to say what happens to it (Waiting for me). The heap's other rules, such as the role, don't matter: any
/// member sees it waiting.
pub const fn waits_for_you(item: &Item, heap: &Heap, open_questions: usize, stuck: bool, blocker_gone: bool) -> bool {
    !item.is_closed() && !heap.is_archived() && (open_questions > 0 || stuck || blocker_gone)
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

/// The round of a review of `piece` asked now: one more than the answered reviews of that piece. Open and withdrawn
/// ones don't count, so asking again after a take back has the round the withdrawn review had.
pub fn next_round(questions: &[Question], piece: &ReviewPiece) -> u32 {
    let answered = questions
        .iter()
        .filter(|question| {
            question.state() == QuestionState::Answered
                && question.review.as_ref().is_some_and(|review| review.piece == *piece)
        })
        .count();
    u32::try_from(answered).unwrap_or(u32::MAX).saturating_add(1)
}

/// Everything that keeps a task from being closed on a done proposal, in the order a message lists it: open steps,
/// open questions, then each reviewed piece whose latest round is not approved. `Ok` when none does: a task without
/// steps or reviews passes. `questions` and `steps` are all the item has; withdrawn reviews are not looked at.
pub fn check_close_ready(steps: &[Step], questions: &[Question]) -> Result<(), Vec<CloseBlocker>> {
    let mut blockers = Vec::new();
    let open_steps = steps.iter().filter(|step| step.state == StepState::Open).count();
    if open_steps > 0 {
        blockers.push(CloseBlocker::OpenSteps { count: open_steps });
    }
    for question in questions.iter().filter(|q| q.state() == QuestionState::Open) {
        blockers.push(CloseBlocker::OpenQuestion {
            number: question.number,
            kind: question.kind,
        });
    }
    // The latest round of each piece, in the order the pieces were first asked.
    let mut latest: Vec<&Question> = Vec::new();
    for question in questions
        .iter()
        .filter(|q| q.review.is_some() && q.state() != QuestionState::Withdrawn)
    {
        let piece = question.review.as_ref().map(|review| &review.piece);
        match latest
            .iter_mut()
            .find(|seen| seen.review.as_ref().map(|review| &review.piece) == piece)
        {
            Some(seen) if round_of(question) >= round_of(seen) => *seen = question,
            Some(_) => {}
            None => latest.push(question),
        }
    }
    for question in latest.into_iter().filter(|q| !q.is_approved_review()) {
        if let Some(review) = &question.review {
            blockers.push(CloseBlocker::NotApproved {
                piece: review.piece.as_str().to_owned(),
                round: review.round,
            });
        }
    }
    if blockers.is_empty() { Ok(()) } else { Err(blockers) }
}

fn round_of(question: &Question) -> u32 {
    question.review.as_ref().map_or(0, |review| review.round)
}

/// Checks that a review or a done proposal, `fields`, may be asked on `item`, whose questions are `questions` and
/// steps `steps`. `goal` is the goal of the open turn of the run it is asked in, or `None` outside a run. Other kinds
/// pass.
///
/// Fails with [`QuestionError::NotOnTask`] unless the item is a task, with [`QuestionError::NeedsDoTurn`] unless
/// `goal` is [`Goal::Done`], with [`QuestionError::ReviewOpen`] for a review of a piece that has one open, and
/// with [`QuestionError::NotReadyToClose`] for a done proposal while [`check_close_ready`] finds a blocker.
pub fn check_task_ask(
    item: &Item,
    goal: Option<Goal>,
    steps: &[Step],
    questions: &[Question],
    fields: &NewQuestion,
) -> Result<(), QuestionError> {
    if !matches!(fields.kind, QuestionKind::Review | QuestionKind::DoneProposal) {
        return Ok(());
    }
    if item.kind != Kind::Task {
        return Err(QuestionError::NotOnTask { item_id: item.id });
    }
    if goal != Some(Goal::Done) {
        return Err(QuestionError::NeedsDoTurn { item_id: item.id });
    }
    if let Some(target) = &fields.review
        && questions.iter().any(|question| {
            question.state() == QuestionState::Open
                && question
                    .review
                    .as_ref()
                    .is_some_and(|review| review.piece == target.piece)
        })
    {
        return Err(QuestionError::ReviewOpen {
            piece: target.piece.as_str().to_owned(),
        });
    }
    if fields.kind == QuestionKind::DoneProposal {
        check_close_ready(steps, questions).map_err(QuestionError::NotReadyToClose)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use googletest::prelude::*;
    use uuid::Uuid;

    use super::*;
    use crate::HeapId;
    use crate::handle::ItemKey;
    use crate::item::{ItemState, Need, NewItem, Title};

    fn sample_item(closed: bool) -> Item {
        let fields = NewItem {
            kind: Kind::Note,
            title: Title::parse("Rotate the certs").expect("valid"),
            description: None,
            need: Need::Expected,
            effort: None,
            impact: None,
            due: None,
            arrives_at: None,
            tags: BTreeSet::new(),
            parameters: BTreeMap::new(),
            origin: None,
            blocked_by: None,
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
        NewQuestion::new(
            kind,
            text("Which CA?"),
            None,
            &labels(choices),
            many,
            None,
            ReviewInput::default(),
        )
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

        expect_that!(number_question(&item, &[], &[], &new), ok(eq(&QuestionNumber::FIRST)));
        expect_that!(
            number_question(&item, &[fourth], &[], &new),
            ok(eq(&QuestionNumber::new(5).expect("positive")))
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
            err(eq(&QuestionError::TooMany {
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
            err(eq(&QuestionError::StepNotFound { number: step }))
        );
    }

    #[gtest]
    fn a_closed_item_takes_no_questions() {
        expect_that!(check_questions_editable(&sample_item(false)), ok(anything()));
        expect_that!(check_questions_editable(&sample_item(true)), err(anything()));
    }

    #[gtest]
    fn a_withdrawn_question_is_not_open_and_takes_no_reply() {
        let open = question(QuestionKind::Question, &[], false);
        let user = UserId::from_uuid(Uuid::from_u128(10));

        let withdrawn = open.withdraw(user, Timestamp::EPOCH);

        expect_that!(open.state(), eq(QuestionState::Open));
        expect_that!(withdrawn.state(), eq(QuestionState::Withdrawn));
        expect_that!(withdrawn.reply, none());
        expect_that!(
            withdrawn.reply(&[], Context::parse("x").expect("valid"), user, Timestamp::EPOCH),
            err(eq(&ReplyError::Question(QuestionError::Withdrawn {
                number: withdrawn.number
            })))
        );
    }

    #[gtest]
    fn kinds_and_states_round_trip_through_their_codes() {
        for kind in QuestionKind::ALL {
            expect_that!(QuestionKind::from_code(kind.code()), some(eq(kind)));
        }
        for state in QuestionState::ALL {
            expect_that!(QuestionState::from_code(state.code()), some(eq(state)));
        }
        expect_that!(QuestionKind::from_code("unknown"), none());
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

    const PR1: &str = "https://example.com/pull/1";

    fn ask(
        kind: QuestionKind,
        link: Option<&str>,
        piece: Option<&str>,
    ) -> std::result::Result<NewQuestion, ValidationError> {
        NewQuestion::new(kind, text("Look?"), None, &[], false, None, ReviewInput { link, piece })
    }

    fn review_of(number: u32, piece: &str, round: u32) -> Question {
        let fields = ask(QuestionKind::Review, Some(PR1), Some(piece)).expect("valid");
        let mut question = Question::new(
            QuestionId::from_uuid(Uuid::from_u128(100 + u128::from(number))),
            ItemId::from_uuid(Uuid::from_u128(3)),
            QuestionNumber::new(number).expect("positive"),
            fields,
            UserId::from_uuid(Uuid::from_u128(10)),
            Timestamp::EPOCH,
        )
        .in_round(round);
        question.run_id = None;
        question
    }

    fn answered(question: &Question, pick: u32) -> Question {
        reply(question, &[pick], "").expect("valid")
    }

    fn step(number: u32, state: StepState) -> Step {
        Step {
            id: crate::StepId::from_uuid(Uuid::from_u128(u128::from(number))),
            item_id: ItemId::from_uuid(Uuid::from_u128(3)),
            number: StepNumber::new(number).expect("positive"),
            position: number,
            title: crate::item::Title::parse("Run in staging").expect("valid"),
            context: None,
            state,
            created_at: Timestamp::EPOCH,
            version: Version::INITIAL,
        }
    }

    #[gtest]
    fn a_review_and_a_done_proposal_have_fixed_choices() {
        let review = ask(QuestionKind::Review, Some(PR1), None).expect("valid");
        expect_that!(review.choices.labels(), eq(&labels(&["Approved", "Changes asked"])));
        let proposal = ask(QuestionKind::DoneProposal, None, None).expect("valid");
        expect_that!(proposal.choices.labels(), eq(&labels(&["Close as done", "Not yet"])));
        for kind in [QuestionKind::Review, QuestionKind::DoneProposal] {
            let new = |choices: &[String], many, step| {
                NewQuestion::new(
                    kind,
                    text("x"),
                    None,
                    choices,
                    many,
                    step,
                    ReviewInput {
                        link: (kind == QuestionKind::Review).then_some(PR1),
                        piece: None,
                    },
                )
            };
            expect_that!(
                new(&labels(&["A", "B"]), false, None),
                err(eq(&ValidationError::NotAllowed { field: Field::Choices }))
            );
            expect_that!(
                new(&[], true, None),
                err(eq(&ValidationError::NotAllowed { field: Field::Many }))
            );
            expect_that!(
                new(&[], false, StepNumber::new(1)),
                err(eq(&ValidationError::NotAllowed { field: Field::Step }))
            );
        }
    }

    #[gtest]
    fn a_review_needs_a_web_link_and_no_other_kind_takes_one() {
        expect_that!(
            ask(QuestionKind::Review, None, None),
            err(eq(&ValidationError::Empty { field: Field::Link }))
        );
        expect_that!(
            ask(QuestionKind::Review, Some("ftp://x/y"), None),
            err(eq(&ValidationError::FieldInvalid { field: Field::Link }))
        );
        expect_that!(
            ask(QuestionKind::Request, Some(PR1), None),
            err(eq(&ValidationError::NotAllowed { field: Field::Link }))
        );
        expect_that!(
            ask(QuestionKind::DoneProposal, None, Some("p")),
            err(eq(&ValidationError::NotAllowed { field: Field::Piece }))
        );
    }

    #[gtest]
    fn a_pieces_defaults_to_the_link_and_a_blank_one_is_none() {
        let by_default = ask(QuestionKind::Review, Some(PR1), Some("  ")).expect("valid");
        expect_that!(by_default.review.map(|r| r.piece.as_str().to_owned()), some(eq(PR1)));
        let named = ask(QuestionKind::Review, Some(PR1), Some(" the API ")).expect("valid");
        expect_that!(named.review.map(|r| r.piece.as_str().to_owned()), some(eq("the API")));
        expect_that!(
            ask(QuestionKind::Review, Some(PR1), Some("a\nb")),
            err(eq(&ValidationError::MultipleLines { field: Field::Piece }))
        );
        expect_that!(
            ask(QuestionKind::Review, Some(PR1), Some(&"x".repeat(2_001))),
            err(eq(&ValidationError::TooLong {
                field: Field::Piece,
                max: 2_000
            }))
        );
    }

    #[gtest]
    fn the_round_counts_answered_reviews_of_the_same_piece() {
        let piece = ReviewPiece::parse("a").expect("valid").expect("given");
        expect_that!(next_round(&[], &piece), eq(1));
        let first = review_of(1, "a", 1);
        // Open, withdrawn and other pieces don't count.
        let withdrawn = review_of(2, "a", 1).withdraw(UserId::from_uuid(Uuid::from_u128(10)), Timestamp::EPOCH);
        expect_that!(
            next_round(&[first.clone(), withdrawn, review_of(3, "b", 1)], &piece),
            eq(1)
        );
        expect_that!(next_round(&[answered(&first, 0)], &piece), eq(2));
        expect_that!(next_round(&[answered(&first, 1)], &piece), eq(2));
    }

    #[gtest]
    fn a_task_with_no_steps_or_reviews_is_ready_to_close() {
        expect_that!(check_close_ready(&[], &[]), ok(()));
        expect_that!(
            check_close_ready(&[step(1, StepState::Ticked), step(2, StepState::Dropped)], &[]),
            ok(())
        );
    }

    #[gtest]
    fn every_blocker_is_listed_together() {
        let open_review = review_of(4, "a", 2);
        let questions = [answered(&review_of(1, "a", 1), 1), open_review];

        let result = check_close_ready(&[step(1, StepState::Open), step(2, StepState::Open)], &questions);

        expect_that!(
            result,
            err(eq(&vec![
                CloseBlocker::OpenSteps { count: 2 },
                CloseBlocker::OpenQuestion {
                    number: QuestionNumber::new(4).expect("positive"),
                    kind: QuestionKind::Review
                },
                CloseBlocker::NotApproved {
                    piece: "a".to_owned(),
                    round: 2
                },
            ]))
        );
        expect_that!(
            QuestionError::NotReadyToClose(result.expect_err("blocked")).to_string(),
            eq("Not ready to close: 2 open steps; review q4 is still open; the piece a has round 2 not approved.")
        );
    }

    #[gtest]
    fn the_latest_round_of_each_piece_decides() {
        let approved_after_changes = [answered(&review_of(1, "a", 1), 1), answered(&review_of(2, "a", 2), 0)];
        expect_that!(check_close_ready(&[], &approved_after_changes), ok(()));
        let changes_after_approval = [answered(&review_of(1, "a", 1), 0), answered(&review_of(2, "a", 2), 1)];
        expect_that!(
            check_close_ready(&[], &changes_after_approval),
            err(eq(&vec![CloseBlocker::NotApproved {
                piece: "a".to_owned(),
                round: 2
            }]))
        );
        // A withdrawn review is not looked at.
        let withdrawn = review_of(3, "b", 1).withdraw(UserId::from_uuid(Uuid::from_u128(10)), Timestamp::EPOCH);
        expect_that!(check_close_ready(&[], &[withdrawn]), ok(()));
    }

    #[gtest]
    fn a_review_or_proposal_is_asked_on_a_task_in_a_done_turn() {
        let task = sample_item(false);
        let mut note = sample_item(false);
        note.kind = Kind::Note;
        let review = ask(QuestionKind::Review, Some(PR1), None).expect("valid");
        let proposal = ask(QuestionKind::DoneProposal, None, None).expect("valid");

        expect_that!(check_task_ask(&task, Some(Goal::Done), &[], &[], &review), ok(()));
        expect_that!(check_task_ask(&task, Some(Goal::Done), &[], &[], &proposal), ok(()));
        expect_that!(
            check_task_ask(&note, Some(Goal::Done), &[], &[], &review),
            err(eq(&QuestionError::NotOnTask { item_id: note.id }))
        );
        for goal in [None, Some(Goal::Defined), Some(Goal::WithContext)] {
            expect_that!(
                check_task_ask(&task, goal, &[], &[], &proposal),
                err(eq(&QuestionError::NeedsDoTurn { item_id: task.id }))
            );
        }
        // Other kinds are not held to it.
        let plain = fields(QuestionKind::Question, &[], false).expect("valid");
        expect_that!(check_task_ask(&note, None, &[], &[], &plain), ok(()));
    }

    #[gtest]
    fn a_second_review_of_a_piece_waits_for_the_first() {
        let task = sample_item(false);
        let open = review_of(1, "a", 1);
        let same = NewQuestion::new(
            QuestionKind::Review,
            text("x"),
            None,
            &[],
            false,
            None,
            ReviewInput {
                link: Some(PR1),
                piece: Some("a"),
            },
        )
        .expect("valid");
        let other = ask(QuestionKind::Review, Some(PR1), Some("b")).expect("valid");

        expect_that!(
            check_task_ask(&task, Some(Goal::Done), &[], std::slice::from_ref(&open), &same),
            err(eq(&QuestionError::ReviewOpen { piece: "a".to_owned() }))
        );
        expect_that!(
            check_task_ask(&task, Some(Goal::Done), &[], std::slice::from_ref(&open), &other),
            ok(())
        );
        expect_that!(
            check_task_ask(&task, Some(Goal::Done), &[], &[answered(&open, 1)], &same),
            ok(())
        );
    }
}
