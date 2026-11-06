//! Building audit entries and diffs.

use domain::item::{Due, Item};
use domain::tag::Tag;
use serde_json::{Map, Value, json};

/// The fields of a newly created item, for its `item_created` entry.
pub(crate) fn describe_new_item(item: &Item) -> Value {
    Value::Object(item_fields(item))
}

/// Changed fields between two versions of an item, as `{"field": {"before": …, "after": …}}`.
pub(crate) fn diff_items(before: &Item, after: &Item) -> Value {
    diff_fields(item_fields(before), item_fields(after))
}

/// The fields of a newly created tag, for its `tag_created` entry.
pub(crate) fn describe_new_tag(tag: &Tag) -> Value {
    Value::Object(tag_fields(tag))
}

/// Changed fields between two versions of a tag.
pub(crate) fn diff_tags(before: &Tag, after: &Tag) -> Value {
    diff_fields(tag_fields(before), tag_fields(after))
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
    fields.insert("state".into(), json!(item.state.kind().code()));
    fields.insert("blocked_by".into(), json!(item.blocked_by().map(|id| id.to_string())));
    fields.insert("effort".into(), json!(item.effort.map(|s| s.code())));
    fields.insert("impact".into(), json!(item.impact.map(|s| s.code())));
    fields.insert("due".into(), json!(due(item.due)));
    // Tag IDs rather than names: names change, and the audit entry must still say which tag it was.
    let tags: Vec<String> = item.tags.iter().map(ToString::to_string).collect();
    fields.insert("tags".into(), json!(tags));
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
