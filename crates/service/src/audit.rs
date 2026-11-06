//! Building audit entries and diffs.

use domain::item::{Due, Item};
use serde_json::{Map, Value, json};

/// The fields of a newly created item, for its `item_created` entry.
pub(crate) fn describe_new_item(item: &Item) -> Value {
    Value::Object(item_fields(item))
}

/// Changed fields between two versions of an item, as `{"field": {"before": …, "after": …}}`.
pub(crate) fn diff_items(before: &Item, after: &Item) -> Value {
    let before = item_fields(before);
    let after = item_fields(after);
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
    fields.insert("effort".into(), json!(item.effort.map(|s| s.code())));
    fields.insert("impact".into(), json!(item.impact.map(|s| s.code())));
    fields.insert("due".into(), json!(due(item.due)));
    fields
}
