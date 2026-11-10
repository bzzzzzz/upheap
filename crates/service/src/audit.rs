//! Building audit entries and diffs.

use domain::heap::HeapPolicy;
use domain::item::{Due, Item, Kind, Stage};
use domain::list_view::ListView;
use domain::parameter::Parameter;
use domain::saved_view::SavedView;
use domain::tag::Tag;
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
    Value::Object(fields)
}

/// Changed fields between two versions of an item, as `{"field": {"before": …, "after": …}}`.
pub(crate) fn diff_items(before: &Item, after: &Item) -> Value {
    diff_fields(item_fields(before), item_fields(after))
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
        "sort": view.sort.code(),
        "sort_parameter": view.sort.parameter().map(|id| id.to_string()),
        "tag": tags,
        "effort": effort,
        "impact": impact,
        "need": need,
        "param": options,
    })
}
