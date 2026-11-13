//! Building audit entries and diffs.

use domain::comment::Comment;
use domain::heap::HeapPolicy;
use domain::item::{Due, Item, Kind, Stage};
use domain::list_view::ListView;
use domain::parameter::Parameter;
use domain::question::Question;
use domain::saved_view::SavedView;
use domain::step::Step;
use domain::tag::Tag;
use domain::turn::{Goal, TakeBackWhy, Turn};
use domain::{OptionId, ParameterId};
use serde_json::{Map, Value, json};

/// The fields of a newly created item, for its `item_created` entry. The kind and the stage are left out while they
/// are the defaults every new item starts with, so a new item's history doesn't grow two lines.
pub(crate) fn describe_new_item(item: &Item) -> Value {
    let mut fields = item_fields(item);
    if item.kind == Kind::Note {
        fields.remove("kind");
    }
    if item.stage == Stage::AsWritten {
        fields.remove("stage");
    }
    if let Some(origin) = item.origin_id {
        fields.insert("origin".into(), json!(origin.to_string()));
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
        "heap".into(),
        json!({ "before": before.heap_id.to_string(), "after": after.heap_id.to_string() }),
    );
    if before.key != after.key {
        changes.insert(
            "key".into(),
            json!({ "before": before.key.as_str(), "after": after.key.as_str() }),
        );
    }
    Value::Object(changes)
}

/// The changes freeing an item makes when its blocker is closed as done, in the shape of [`diff_items`], with
/// `blocker_done` naming the blocker as the reason.
pub(crate) fn diff_freed(before: &Item, after: &Item, blocker: &Item) -> Value {
    let Value::Object(mut changes) = diff_items(before, after) else {
        return Value::Null;
    };
    changes.insert("blocker_done".into(), json!(blocker.id.to_string()));
    Value::Object(changes)
}

/// The fields of a newly added step, for its `step_added` entry. The step is named by its number and title; the
/// context is left out when there is none.
pub(crate) fn describe_new_step(step: &Step) -> Value {
    let mut fields = Map::new();
    fields.insert("step".into(), step_name(step));
    if let Some(context) = &step.context {
        fields.insert("step_context".into(), json!(context.as_str()));
    }
    Value::Object(fields)
}

/// Changed fields between two versions of a step, in the shape of [`diff_items`], led by the step's name as it is
/// after the change.
pub(crate) fn diff_steps(before: &Step, after: &Step) -> Value {
    let fields = |step: &Step| {
        let mut fields = Map::new();
        fields.insert("step_title".into(), json!(step.title.as_str()));
        fields.insert("step_context".into(), json!(step.context.as_ref().map(|c| c.as_str())));
        fields.insert("step_state".into(), json!(step.state.code()));
        fields
    };
    let Value::Object(mut changes) = diff_fields(fields(before), fields(after)) else {
        return Value::Null;
    };
    changes.insert("step".into(), step_name(after));
    Value::Object(changes)
}

/// The fields of a newly asked question, for its `question_asked` entry. Details, choices, `many` and the step are
/// left out when there are none.
pub(crate) fn describe_new_question(question: &Question) -> Value {
    let mut fields = Map::new();
    fields.insert("question".into(), json!(question.number.get()));
    fields.insert("question_kind".into(), json!(question.kind.code()));
    fields.insert("question_text".into(), json!(question.text.as_str()));
    if let Some(details) = &question.details {
        fields.insert("question_details".into(), json!(details.as_str()));
    }
    if !question.choices.is_empty() {
        fields.insert("question_choices".into(), json!(question.choices.labels()));
    }
    if question.many {
        fields.insert("question_many".into(), json!(true));
    }
    if let Some(step) = question.step {
        fields.insert("question_step".into(), json!(step.get()));
    }
    if let Some(review) = &question.review {
        fields.insert("question_link".into(), json!(review.link.as_str()));
        fields.insert("question_piece".into(), json!(review.piece.as_str()));
        fields.insert("question_round".into(), json!(review.round));
    }
    Value::Object(fields)
}

/// A newly added comment, for its `comment_added` entry: its number and its whole text.
pub(crate) fn describe_new_comment(comment: &Comment) -> Value {
    json!({
        "comment": comment.number.get(),
        "comment_text": comment.body.as_str(),
    })
}

/// The reply to a question, for its `question_answered` entry: the labels picked, joined, and the comment, each
/// `null` when there is none.
pub(crate) fn describe_reply(question: &Question) -> Value {
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
    json!({
        "question": question.number.get(),
        "question_text": question.text.as_str(),
        "reply": (!picked.is_empty()).then(|| picked.join(", ")),
        "reply_comment": reply.and_then(|reply| reply.comment.as_ref()).map(|comment| comment.as_str()),
    })
}

/// The question withdrawn when its item was closed, or taken back from the agent that asked it, for its
/// `question_withdrawn` entry.
pub(crate) fn describe_withdrawal(question: &Question, why: TakeBackWhy) -> Value {
    json!({
        "question": question.number.get(),
        "question_text": question.text.as_str(),
        "why": why.code(),
    })
}

/// A turn just opened, for its `turn_opened` entry. The line is `null` when there is none.
pub(crate) fn describe_new_turn(turn: &Turn) -> Value {
    json!({
        "reach": turn.reach.code(),
        "goal": turn.goal.code(),
        "line": turn.line.as_ref().map(domain::turn::TurnLine::as_str),
        "source": turn.source.code(),
    })
}

/// A turn just taken back, for its `turn_taken_back` entry.
pub(crate) fn describe_take_back(turn: &Turn, why: TakeBackWhy) -> Value {
    json!({ "goal": turn.goal.code(), "why": why.code() })
}

/// A turn whose goal a run reached, for its `turn_reached` entry: the goal, and the stage the item moved from and to.
pub(crate) fn describe_reached(goal: Goal, before: Stage, after: Stage) -> Value {
    json!({
        "goal": goal.code(),
        "stage": { "before": before.code(), "after": after.code() },
    })
}

/// How a turn came to end because its item was at the goal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GoalMetHow {
    /// The person set the item's stage to the goal or past it.
    StageSet,
    /// A claim found the item already there.
    AlreadyMet,
}

impl GoalMetHow {
    /// Stable code used in audit diffs. Never renamed.
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::StageSet => "stage_set",
            Self::AlreadyMet => "already_met",
        }
    }
}

/// A turn that ended because its item was at the goal, for its `turn_reached` entry: the run path's diff, and how.
/// The stage is left out when it didn't change, as for a claim that found the item already there.
pub(crate) fn describe_goal_met(goal: Goal, before: Stage, after: Stage, how: GoalMetHow) -> Value {
    if before == after {
        return json!({ "goal": goal.code(), "how": how.code() });
    }
    json!({
        "goal": goal.code(),
        "stage": { "before": before.code(), "after": after.code() },
        "how": how.code(),
    })
}

/// A turn a run handed back, for its `turn_handed_back` entry.
pub(crate) fn describe_hand_back(turn: &Turn, reason: &str) -> Value {
    json!({ "goal": turn.goal.code(), "reason": reason })
}

/// A turn the person sent back for changes, for its `turn_redone` entry: the redone turn's goal, the stage the item
/// moved from and to, and the person's line.
pub(crate) fn describe_redone(turn: &Turn, before: Stage, after: Stage, line: &str) -> Value {
    json!({
        "goal": turn.goal.code(),
        "stage": { "before": before.code(), "after": after.code() },
        "line": line,
    })
}

/// A stuck turn let go again, for its `turn_retried` entry.
pub(crate) fn describe_retry(turn: &Turn) -> Value {
    json!({ "goal": turn.goal.code() })
}

fn step_name(step: &Step) -> Value {
    json!({ "number": step.number.get(), "title": step.title.as_str() })
}

/// The change to an item that archiving `from` switched to `to`, in the shape of [`diff_items`] but naming only
/// that parameter's value: the item's other values didn't change.
pub(crate) fn diff_switched_value(parameter: ParameterId, from: OptionId, to: OptionId) -> Value {
    let value = |option: OptionId| json!({ parameter.to_string(): option.to_string() });
    json!({ "parameters": { "before": value(from), "after": value(to) } })
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

/// Changed weights between two policies of a heap, named like the heap's columns.
pub(crate) fn diff_policies(before: &HeapPolicy, after: &HeapPolicy) -> Value {
    diff_fields(policy_fields(before), policy_fields(after))
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
    fields.insert("title".into(), json!(item.title.as_str()));
    fields.insert(
        "description".into(),
        json!(item.description.as_ref().map(|d| d.as_str())),
    );
    fields.insert("kind".into(), json!(item.kind.code()));
    fields.insert("stage".into(), json!(item.stage.code()));
    fields.insert("state".into(), json!(item.state.kind().code()));
    fields.insert("blocked_by".into(), json!(item.blocked_by().map(|id| id.to_string())));
    fields.insert(
        "blocked_reason".into(),
        json!(item.blocked_reason().map(|reason| reason.as_str())),
    );
    fields.insert("need".into(), json!(item.need.code()));
    fields.insert("effort".into(), json!(item.effort.map(|s| s.code())));
    fields.insert("impact".into(), json!(item.impact.map(|s| s.code())));
    fields.insert("due".into(), json!(due(item.due)));
    // Tag IDs rather than names: names change, and the audit entry must still say which tag it was.
    let tags: Vec<String> = item.tags.iter().map(ToString::to_string).collect();
    fields.insert("tags".into(), json!(tags));
    let values: Map<String, Value> = item
        .parameters
        .iter()
        .map(|(parameter, option)| (parameter.to_string(), json!(option.to_string())))
        .collect();
    fields.insert("parameters".into(), Value::Object(values));
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
    fields.insert("name".into(), json!(parameter.name.as_str()));
    fields.insert("weight".into(), json!(parameter.weight.get()));
    fields.insert("options".into(), json!(options));
    fields.insert("archived_options".into(), json!(archived));
    fields.insert("tags".into(), json!(tags));
    fields.insert(
        "archived_at".into(),
        json!(parameter.archived_at.map(domain::Timestamp::as_millis)),
    );
    fields
}

fn policy_fields(policy: &HeapPolicy) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("need_weight".into(), json!(policy.need_weight.get()));
    fields.insert("age_weight".into(), json!(policy.age_weight.get()));
    fields.insert("deadline_weight".into(), json!(policy.deadline_weight.get()));
    fields.insert("value_weight".into(), json!(policy.value_weight.get()));
    fields
}

fn tag_fields(tag: &Tag) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(tag.name.as_str()));
    fields.insert(
        "description".into(),
        json!(tag.description.as_ref().map(|d| d.as_str())),
    );
    fields
}

fn saved_view_fields(view: &SavedView) -> Map<String, Value> {
    let mut fields = Map::new();
    fields.insert("name".into(), json!(view.name.as_str()));
    fields.insert("list_view".into(), list_view_fields(&view.list_view));
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
