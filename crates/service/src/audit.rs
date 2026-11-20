//! Building audit entries and diffs.

use domain::audit::AuditField;
use domain::comment::Comment;
use domain::heap::{Heap, HeapPolicy};
use domain::item::{Due, Item, Kind, Stage};
use domain::list_view::ListView;
use domain::parameter::Parameter;
use domain::question::Question;
use domain::saved_view::SavedView;
use domain::step::Step;
use domain::tag::Tag;
use domain::turn::{Goal, TakeBackWhy, Turn};
use domain::{OptionId, ParameterId, TagId};
use serde_json::{Map, Value, json};
use store::parameters::HeapUse;

/// The fields of a newly created item, for its `item_created` entry. The kind and the stage are left out while they
/// are the defaults every new item starts with, so a new item's history doesn't grow two lines.
pub(crate) fn describe_new_item(item: &Item) -> Value {
    let mut fields = item_fields(item);
    if item.kind == Kind::Note {
        fields.remove(AuditField::Kind.code());
    }
    if item.stage == Stage::AsWritten {
        fields.remove(AuditField::Stage.code());
    }
    if let Some(origin) = item.origin_id {
        fields.insert(AuditField::Origin.code().into(), json!(origin.to_string()));
    }
    Value::Object(fields)
}

/// Changed fields between two versions of an item, as `{"field": {"before": …, "after": …}}`.
pub(crate) fn diff_items(before: &Item, after: &Item) -> Value {
    diff_fields(item_fields(before), item_fields(after))
}

/// The changes a move to another heap makes to an item, in the shape of [`diff_items`] with the heap, and the key when
/// it changed. Tags and values the target heap lacks show as removed, and the blocker as cleared.
pub(crate) fn diff_moved(before: &Item, after: &Item) -> Value {
    let Value::Object(mut changes) = diff_items(before, after) else {
        return Value::Null;
    };
    changes.insert(
        AuditField::Heap.code().into(),
        json!({ "before": before.heap_id.to_string(), "after": after.heap_id.to_string() }),
    );
    if before.key != after.key {
        changes.insert(
            AuditField::Key.code().into(),
            json!({ "before": before.key.as_str(), "after": after.key.as_str() }),
        );
    }
    Value::Object(changes)
}

/// The entry in the log of the team an item left for another: the heap it was in, with no target, and the title it had,
/// which that log names it by afterwards (`item_title` is never shown as a change).
pub(crate) fn diff_moved_out(before: &Item) -> Value {
    json!({
        AuditField::Heap.code(): { "before": before.heap_id.to_string(), "after": null },
        AuditField::ItemTitle.code(): before.title.as_str(),
    })
}

/// The entry in the log of the team an item came to from another: the heap it is in, with no source and nothing else,
/// as the key, tags, values and blocker it had are the source team's.
pub(crate) fn diff_moved_in(after: &Item) -> Value {
    json!({ AuditField::Heap.code(): { "before": null, "after": after.heap_id.to_string() } })
}

/// The changes freeing an item makes when its blocker is closed as done, in the shape of [`diff_items`], with
/// `blocker_done` naming the blocker as the reason.
pub(crate) fn diff_freed(before: &Item, after: &Item, blocker: &Item) -> Value {
    let Value::Object(mut changes) = diff_items(before, after) else {
        return Value::Null;
    };
    changes.insert(AuditField::BlockerDone.code().into(), json!(blocker.id.to_string()));
    Value::Object(changes)
}

/// The fields of a newly added step, for its `step_added` entry. The step is named by its number and title; the
/// context is left out when there is none.
pub(crate) fn describe_new_step(step: &Step) -> Value {
    let mut fields = Map::new();
    fields.insert(AuditField::Step.code().into(), step_name(step));
    if let Some(context) = &step.context {
        fields.insert(AuditField::StepContext.code().into(), json!(context.as_str()));
    }
    Value::Object(fields)
}

/// Changed fields between two versions of a step, in the shape of [`diff_items`], led by the step's name as it is
/// after the change. `places` is the number of the step it followed before and follows after, 0 for first.
pub(crate) fn diff_steps(before: &Step, after: &Step, places: (u32, u32)) -> Value {
    let fields = |step: &Step, place: u32| {
        let mut fields = Map::new();
        fields.insert(AuditField::StepTitle.code().into(), json!(step.title.as_str()));
        fields.insert(
            AuditField::StepContext.code().into(),
            json!(step.context.as_ref().map(|c| c.as_str())),
        );
        fields.insert(AuditField::StepState.code().into(), json!(step.state.code()));
        fields.insert(AuditField::StepPlace.code().into(), json!(place));
        fields
    };
    let Value::Object(mut changes) = diff_fields(fields(before, places.0), fields(after, places.1)) else {
        return Value::Null;
    };
    changes.insert(AuditField::Step.code().into(), step_name(after));
    Value::Object(changes)
}

/// The fields of a newly asked question, for its `question_asked` entry. Details, choices, `many` and the step are
/// left out when there are none.
pub(crate) fn describe_new_question(question: &Question) -> Value {
    let mut fields = Map::new();
    fields.insert(AuditField::Question.code().into(), json!(question.number.get()));
    fields.insert(AuditField::QuestionKind.code().into(), json!(question.kind.code()));
    fields.insert(AuditField::QuestionText.code().into(), json!(question.text.as_str()));
    if let Some(details) = &question.details {
        fields.insert(AuditField::QuestionDetails.code().into(), json!(details.as_str()));
    }
    if !question.choices.is_empty() {
        fields.insert(
            AuditField::QuestionChoices.code().into(),
            json!(question.choices.labels()),
        );
    }
    if question.many {
        fields.insert(AuditField::QuestionMany.code().into(), json!(true));
    }
    if let Some(step) = question.step {
        fields.insert(AuditField::QuestionStep.code().into(), json!(step.get()));
    }
    if let Some(review) = &question.review {
        fields.insert(AuditField::QuestionLink.code().into(), json!(review.link.as_str()));
        fields.insert(AuditField::QuestionPiece.code().into(), json!(review.piece.as_str()));
        fields.insert(AuditField::QuestionRound.code().into(), json!(review.round));
    }
    Value::Object(fields)
}

/// A newly added comment, for its `comment_added` entry: its number and its whole text.
pub(crate) fn describe_new_comment(comment: &Comment) -> Value {
    json!({
        AuditField::Comment.code(): comment.number.get(),
        AuditField::CommentText.code(): comment.body.as_str(),
    })
}

/// The labels `question`'s reply picked, joined, and its comment, each `None` when there is none.
fn reply_parts(question: &Question) -> (Option<String>, Option<&str>) {
    let reply = question.reply.as_ref();
    let picked: Vec<&str> = reply
        .map(|reply| {
            reply
                .picked
                .iter()
                .filter_map(|index| question.choices.labels().get(usize::try_from(*index).ok()?))
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    (
        (!picked.is_empty()).then(|| picked.join(", ")),
        reply
            .and_then(|reply| reply.comment.as_ref())
            .map(|comment| comment.as_str()),
    )
}

/// The reply to a question, for its `question_answered` entry: the labels picked, joined, and the comment, each
/// `null` when there is none.
pub(crate) fn describe_reply(question: &Question) -> Value {
    let (reply, reply_comment) = reply_parts(question);
    json!({
        AuditField::Question.code(): question.number.get(),
        AuditField::QuestionText.code(): question.text.as_str(),
        AuditField::Reply.code(): reply,
        AuditField::ReplyComment.code(): reply_comment,
    })
}

/// A reply changed from `before` to `after`, the same question, for its `reply_changed` entry: the question, and
/// the picks and the comment as before and after, only those that changed.
pub(crate) fn describe_reply_change(before: &Question, after: &Question) -> Value {
    let (picked_before, comment_before) = reply_parts(before);
    let (picked_after, comment_after) = reply_parts(after);
    let mut fields = Map::new();
    fields.insert(AuditField::Question.code().into(), json!(after.number.get()));
    fields.insert(AuditField::QuestionText.code().into(), json!(after.text.as_str()));
    if picked_before != picked_after {
        fields.insert(
            AuditField::Reply.code().into(),
            json!({"before": picked_before, "after": picked_after}),
        );
    }
    if comment_before != comment_after {
        fields.insert(
            AuditField::ReplyComment.code().into(),
            json!({"before": comment_before, "after": comment_after}),
        );
    }
    Value::Object(fields)
}

/// The question withdrawn when its item was closed, or taken back from the agent that asked it, for its
/// `question_withdrawn` entry.
pub(crate) fn describe_withdrawal(question: &Question, why: TakeBackWhy) -> Value {
    json!({
        AuditField::Question.code(): question.number.get(),
        AuditField::QuestionText.code(): question.text.as_str(),
        AuditField::Why.code(): why.code(),
    })
}

/// A turn just opened, for its `turn_opened` entry. The line is `null` when there is none.
pub(crate) fn describe_new_turn(turn: &Turn) -> Value {
    json!({
        AuditField::Reach.code(): turn.reach.code(),
        AuditField::Goal.code(): turn.goal.code(),
        AuditField::Line.code(): turn.line.as_ref().map(domain::turn::TurnLine::as_str),
        AuditField::Source.code(): turn.source.code(),
    })
}

/// A turn just taken back, for its `turn_taken_back` entry.
pub(crate) fn describe_take_back(turn: &Turn, why: TakeBackWhy) -> Value {
    json!({ AuditField::Goal.code(): turn.goal.code(), AuditField::Why.code(): why.code() })
}

/// A turn whose goal a run reached, for its `turn_reached` entry: the goal, and the stage the item moved from and to.
pub(crate) fn describe_reached(goal: Goal, before: Stage, after: Stage) -> Value {
    json!({
        AuditField::Goal.code(): goal.code(),
        AuditField::Stage.code(): { "before": before.code(), "after": after.code() },
    })
}

/// How a turn came to end because its item was at the goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoalMetHow {
    /// The person set the item's stage to the goal or past it.
    StageSet,
    /// A claim found the item already there.
    AlreadyMet,
    /// A run's became made the item defined, as a task split from its note.
    Became,
}

impl GoalMetHow {
    /// Stable code used in audit diffs. Never renamed.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::StageSet => "stage_set",
            Self::AlreadyMet => "already_met",
            Self::Became => "became",
        }
    }
}

/// A turn that ended because its item was at the goal, for its `turn_reached` entry: the run path's diff, and how.
/// The stage is left out when it didn't change, as for a claim that found the item already there.
pub(crate) fn describe_goal_met(goal: Goal, before: Stage, after: Stage, how: GoalMetHow) -> Value {
    if before == after {
        return json!({ AuditField::Goal.code(): goal.code(), AuditField::How.code(): how.code() });
    }
    json!({
        AuditField::Goal.code(): goal.code(),
        AuditField::Stage.code(): { "before": before.code(), "after": after.code() },
        AuditField::How.code(): how.code(),
    })
}

/// A turn a run handed back, for its `turn_handed_back` entry.
pub(crate) fn describe_hand_back(turn: &Turn, reason: &str) -> Value {
    json!({ AuditField::Goal.code(): turn.goal.code(), AuditField::Reason.code(): reason })
}

/// A turn a run left to watch the pull request `url`, for its `turn_watching` entry.
pub(crate) fn describe_watch(turn: &Turn, url: &str) -> Value {
    json!({ AuditField::Goal.code(): turn.goal.code(), AuditField::Url.code(): url })
}

/// A turn the person sent back for changes, for its `turn_redone` entry: the redone turn's goal, the stage the item
/// moved from and to, and the person's line.
pub(crate) fn describe_redone(turn: &Turn, before: Stage, after: Stage, line: &str) -> Value {
    json!({
        AuditField::Goal.code(): turn.goal.code(),
        AuditField::Stage.code(): { "before": before.code(), "after": after.code() },
        AuditField::Line.code(): line,
    })
}

/// A stuck turn let go again, for its `turn_retried` entry.
pub(crate) fn describe_retry(turn: &Turn) -> Value {
    json!({ AuditField::Goal.code(): turn.goal.code() })
}

fn step_name(step: &Step) -> Value {
    json!({ "number": step.number.get(), "title": step.title.as_str() })
}

/// The change to an item that archiving `from` switched to `to`, in the shape of [`diff_items`] but naming only
/// that parameter's value: the item's other values didn't change.
pub(crate) fn diff_switched_value(parameter: ParameterId, from: OptionId, to: OptionId) -> Value {
    let value = |option: OptionId| json!({ parameter.to_string(): option.to_string() });
    json!({ AuditField::Parameters.code(): { "before": value(from), "after": value(to) } })
}

/// The fields of a newly created tag, for its `tag_created` entry.
pub(crate) fn describe_new_tag(tag: &Tag) -> Value {
    Value::Object(tag_fields(tag))
}

/// Changed fields between two versions of a tag.
pub(crate) fn diff_tags(before: &Tag, after: &Tag) -> Value {
    diff_fields(tag_fields(before), tag_fields(after))
}

/// The fields of a new saved view, for its `saved_view_created` entry.
pub(crate) fn describe_new_saved_view(view: &SavedView) -> Value {
    Value::Object(saved_view_fields(view))
}

/// Changed fields between two versions of a saved view. A changed list view is recorded whole.
pub(crate) fn diff_saved_views(before: &SavedView, after: &SavedView) -> Value {
    diff_fields(saved_view_fields(before), saved_view_fields(after))
}

/// The fields of a new parameter, for its `parameter_created` entry.
pub(crate) fn describe_new_parameter(parameter: &Parameter) -> Value {
    Value::Object(parameter_fields(parameter))
}

/// Changed fields between two versions of a parameter. Changed options and tag links are recorded whole.
pub(crate) fn diff_parameters(before: &Parameter, after: &Parameter) -> Value {
    diff_fields(parameter_fields(before), parameter_fields(after))
}

/// Changed fields between a heap's use of a team parameter before (`None` when it never used it) and after: whether it
/// uses it, and its weight.
pub(crate) fn diff_heap_use(before: Option<&HeapUse>, after: &HeapUse) -> Value {
    let fields = |heap_use: Option<&HeapUse>| {
        let mut fields = Map::new();
        fields.insert(
            AuditField::Used.code().into(),
            json!(heap_use.is_some_and(|u| u.stopped_at.is_none())),
        );
        fields.insert(
            AuditField::Weight.code().into(),
            json!(heap_use.map(|u| u.weight.get())),
        );
        fields
    };
    diff_fields(fields(before), fields(Some(after)))
}

/// Changed weights and tag order between a heap and its new `policy` and `tag_order`, named like the heap's columns.
pub(crate) fn diff_policies(before: &Heap, policy: &HeapPolicy, tag_order: &[TagId]) -> Value {
    diff_fields(
        policy_fields(&before.policy, &before.tag_order),
        policy_fields(policy, tag_order),
    )
}

fn diff_fields(before: Map<String, Value>, after: Map<String, Value>) -> Value {
    let changes = after
        .into_iter()
        .filter_map(|(field, new)| {
            let old = before.get(&field).cloned().unwrap_or(Value::Null);
            (old != new).then(|| (field, json!({ "before": old, "after": new })))
        })
        .collect::<Map<_, _>>();
    Value::Object(changes)
}

fn item_fields(item: &Item) -> Map<String, Value> {
    let due = |due: Option<Due>| due.map(|d| json!({ "at": d.at.as_millis(), "firm": d.firm }));
    let mut fields = Map::new();
    fields.insert(AuditField::Title.code().into(), json!(item.title.as_str()));
    fields.insert(
        AuditField::Description.code().into(),
        json!(item.description.as_ref().map(|d| d.as_str())),
    );
    fields.insert(AuditField::Kind.code().into(), json!(item.kind.code()));
    fields.insert(AuditField::Stage.code().into(), json!(item.stage.code()));
    fields.insert(AuditField::State.code().into(), json!(item.state.kind().code()));
    fields.insert(
        AuditField::BlockedBy.code().into(),
        json!(item.blocked_by().map(|id| id.to_string())),
    );
    fields.insert(
        AuditField::BlockedReason.code().into(),
        json!(item.blocked_reason().map(|reason| reason.as_str())),
    );
    fields.insert(AuditField::Need.code().into(), json!(item.need.code()));
    fields.insert(AuditField::Effort.code().into(), json!(item.effort.map(|s| s.code())));
    fields.insert(AuditField::Impact.code().into(), json!(item.impact.map(|s| s.code())));
    fields.insert(AuditField::Due.code().into(), json!(due(item.due)));
    // Tag IDs rather than names: names change, and the audit entry must still say which tag it was.
    let tags: Vec<String> = item.tags.iter().map(ToString::to_string).collect();
    fields.insert(AuditField::Tags.code().into(), json!(tags));
    let values: Map<String, Value> = item
        .parameters
        .iter()
        .map(|(parameter, option)| (parameter.to_string(), json!(option.to_string())))
        .collect();
    fields.insert(AuditField::Parameters.code().into(), Value::Object(values));
    fields
}

fn parameter_fields(parameter: &Parameter) -> Map<String, Value> {
    let options: Vec<Value> = parameter
        .options
        .iter()
        .map(|o| json!({ "id": o.id.to_string(), "label": o.label.as_str() }))
        .collect();
    let archived: Vec<Value> = parameter
        .archived_options
        .iter()
        .map(|o| json!({ "id": o.id.to_string(), "label": o.label.as_str(), "replaced_by": o.replaced_by.to_string() }))
        .collect();
    let tags: Vec<String> = parameter.tags.iter().map(ToString::to_string).collect();
    let mut fields = Map::new();
    fields.insert(AuditField::Name.code().into(), json!(parameter.name.as_str()));
    // A team parameter has no weight of its own: each heap's is in its use (see `diff_heap_use`).
    if !parameter.is_team_owned() {
        fields.insert(AuditField::Weight.code().into(), json!(parameter.weight.get()));
    }
    fields.insert(AuditField::Options.code().into(), json!(options));
    fields.insert(AuditField::ArchivedOptions.code().into(), json!(archived));
    fields.insert(AuditField::Tags.code().into(), json!(tags));
    fields.insert(
        AuditField::ArchivedAt.code().into(),
        json!(parameter.archived_at.map(domain::Timestamp::as_millis)),
    );
    fields
}

fn policy_fields(policy: &HeapPolicy, tag_order: &[TagId]) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert(AuditField::NeedWeight.code().into(), json!(policy.need_weight.get()));
    fields.insert(AuditField::AgeWeight.code().into(), json!(policy.age_weight.get()));
    fields.insert(
        AuditField::DeadlineWeight.code().into(),
        json!(policy.deadline_weight.get()),
    );
    fields.insert(AuditField::ValueWeight.code().into(), json!(policy.value_weight.get()));
    fields.insert(AuditField::TagsWeight.code().into(), json!(policy.tags_weight.get()));
    fields.insert(
        AuditField::TagOrder.code().into(),
        json!(tag_order.iter().map(ToString::to_string).collect::<Vec<_>>()),
    );
    fields
}

fn tag_fields(tag: &Tag) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert(AuditField::Name.code().into(), json!(tag.name.as_str()));
    fields.insert(
        AuditField::Description.code().into(),
        json!(tag.description.as_ref().map(|d| d.as_str())),
    );
    fields
}

fn saved_view_fields(view: &SavedView) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert(AuditField::Name.code().into(), json!(view.name.as_str()));
    fields.insert(AuditField::ListView.code().into(), list_view_fields(&view.list_view));
    fields
}

/// The view in the API's terms, so an entry reads like the request that made it.
fn list_view_fields(view: &ListView) -> Value {
    let tags: Vec<String> = view.filter.tags.iter().map(ToString::to_string).collect();
    let effort: Vec<&str> = view.filter.effort.iter().map(|s| s.code()).collect();
    let impact: Vec<&str> = view.filter.impact.iter().map(|s| s.code()).collect();
    let need: Vec<&str> = view.filter.need.iter().map(|n| n.code()).collect();
    let options: Vec<String> = view.filter.options.iter().map(ToString::to_string).collect();
    json!({
        "open": view.open.code(),
        "in_progress": view.in_progress.code(),
        "blocked": view.blocked.code(),
        "blocked_layout": view.blocked_layout.code(),
        "reminders": view.reminders.code(),
        "sort": view.sort.code(),
        "sort_parameter": view.sort.parameter().map(|id| id.to_string()),
        "tag": tags,
        "effort": effort,
        "impact": impact,
        "need": need,
        "param": options,
    })
}
