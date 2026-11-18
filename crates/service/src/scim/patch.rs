//! SCIM `PATCH` (RFC 7644 §3.5.2, cut down) applied to a resource held as JSON.
//!
//! Understood: `active`, `userName` and `externalId` of a user, `displayName`, `externalId` and `members` of a group,
//! and `members[value eq "<id>"]`. A `replace` or `add` with no path and an object value applies each key as a path.
//! A path or key besides those changes nothing, and so does removing a required attribute. The resource's `members`
//! are `[{"value": <id>}]`.

use serde_json::{Map, Value, json};

use super::filter;

/// Which resource a patch is for, which says what its paths mean.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Resource {
    User,
    Group,
}

/// Why a patch can't be applied.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PatchError {
    Syntax(&'static str),
    Value(&'static str),
    Filter,
    NoTarget,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Remove,
    Replace,
}

/// Applies each operation of the `PatchOp` message `patch`, in order, to `resource`.
pub(crate) fn apply(resource: &mut Map<String, Value>, kind: Resource, patch: &Value) -> Result<(), PatchError> {
    let operations = patch
        .get("Operations")
        .and_then(Value::as_array)
        .ok_or(PatchError::Syntax("Operations must be a list"))?;
    for operation in operations {
        let op = match operation.get("op").and_then(Value::as_str) {
            Some(op) if op.eq_ignore_ascii_case("add") => Op::Add,
            Some(op) if op.eq_ignore_ascii_case("remove") => Op::Remove,
            Some(op) if op.eq_ignore_ascii_case("replace") => Op::Replace,
            _ => return Err(PatchError::Syntax("op must be add, remove or replace")),
        };
        let path = match operation.get("path") {
            None | Some(Value::Null) => None,
            Some(Value::String(path)) => Some(path.as_str()),
            Some(_) => return Err(PatchError::Syntax("path must be a string")),
        };
        let value = operation.get("value").filter(|value| !value.is_null());
        match path {
            Some(path) => apply_path(resource, kind, op, path, value)?,
            None if op == Op::Remove => return Err(PatchError::NoTarget),
            None => {
                let Some(Value::Object(keys)) = value else {
                    return Err(PatchError::Value("value must be an object when there is no path"));
                };
                for (key, value) in keys {
                    apply_path(resource, kind, op, key, Some(value))?;
                }
            }
        }
    }
    Ok(())
}

fn apply_path(
    resource: &mut Map<String, Value>,
    kind: Resource,
    op: Op,
    path: &str,
    value: Option<&Value>,
) -> Result<(), PatchError> {
    if let Some((name, filtered)) = path.split_once('[') {
        // Only a filter on the members of a group is understood, and only to remove one.
        if kind == Resource::Group && name.eq_ignore_ascii_case("members") && op == Op::Remove {
            let inner = filtered
                .strip_suffix(']')
                .ok_or(PatchError::Syntax("the path's filter isn't closed"))?;
            let filter = filter::parse(inner, &["value"]).map_err(|_| PatchError::Filter)?;
            let mut members = members_of(resource);
            members.retain(|member| member != &filter.value);
            set_members(resource, &members);
        }
        return Ok(());
    }
    let attributes: &[&str] = match kind {
        Resource::User => &["active", "userName", "externalId"],
        Resource::Group => &["displayName", "externalId", "members"],
    };
    let Some(attribute) = attributes.iter().copied().find(|name| name.eq_ignore_ascii_case(path)) else {
        return Ok(());
    };
    match (op, attribute) {
        (Op::Remove, "userName" | "displayName") => {}
        (Op::Remove, "members") => match value {
            None => set_members(resource, &[]),
            Some(value) => {
                let gone = member_ids(value)?;
                let mut members = members_of(resource);
                members.retain(|member| !gone.contains(member));
                set_members(resource, &members);
            }
        },
        (Op::Remove, _) => {
            resource.remove(attribute);
        }
        (Op::Add | Op::Replace, _) => {
            let value = value.ok_or(PatchError::Value("value is required"))?;
            match (op, attribute) {
                (Op::Add, "members") => {
                    let mut members = members_of(resource);
                    for id in member_ids(value)? {
                        if !members.contains(&id) {
                            members.push(id);
                        }
                    }
                    set_members(resource, &members);
                }
                (_, "members") => {
                    let mut members = Vec::new();
                    for id in member_ids(value)? {
                        if !members.contains(&id) {
                            members.push(id);
                        }
                    }
                    set_members(resource, &members);
                }
                _ => {
                    resource.insert(attribute.to_owned(), value.clone());
                }
            }
        }
    }
    Ok(())
}

/// The `value`s of a list of `{"value": <id>}`.
fn member_ids(value: &Value) -> Result<Vec<String>, PatchError> {
    const SHAPE: PatchError = PatchError::Value("members must be a list of objects with a value");
    value
        .as_array()
        .ok_or(SHAPE)?
        .iter()
        .map(|member| {
            member
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or(SHAPE)
        })
        .collect()
}

fn members_of(resource: &Map<String, Value>) -> Vec<String> {
    resource
        .get("members")
        .and_then(Value::as_array)
        .map(|members| {
            members
                .iter()
                .filter_map(|member| member.get("value").and_then(Value::as_str).map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn set_members(resource: &mut Map<String, Value>, members: &[String]) {
    let members: Vec<Value> = members.iter().map(|id| json!({ "value": id })).collect();
    resource.insert("members".to_owned(), Value::Array(members));
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    fn object(value: Value) -> Map<String, Value> {
        let Value::Object(map) = value else {
            unreachable!("a test resource is an object")
        };
        map
    }

    fn user() -> Map<String, Value> {
        object(json!({ "userName": "ana@example.test", "externalId": "sub-ana", "active": true, "title": "Dev" }))
    }

    fn group() -> Map<String, Value> {
        object(json!({ "displayName": "eng", "members": [{ "value": "a" }, { "value": "b" }] }))
    }

    fn patched(
        mut resource: Map<String, Value>,
        kind: Resource,
        operations: Value,
    ) -> std::result::Result<Value, PatchError> {
        apply(&mut resource, kind, &json!({ "Operations": operations }))?;
        Ok(Value::Object(resource))
    }

    #[gtest]
    fn okta_deactivates_with_a_replace_that_has_no_path() {
        let result = patched(
            user(),
            Resource::User,
            json!([{ "op": "replace", "value": { "active": false } }]),
        );

        expect_that!(result.map(|u| u["active"].clone()), ok(eq(&json!(false))));
    }

    #[gtest]
    fn replaces_an_attribute_by_its_path_whatever_the_case_of_op_and_path() {
        let result = patched(
            user(),
            Resource::User,
            json!([{ "op": "Replace", "path": "ACTIVE", "value": false }, { "op": "ADD", "path": "userName", "value": "b" }]),
        );

        expect_that!(
            result,
            ok(eq(
                &json!({ "userName": "b", "externalId": "sub-ana", "active": false, "title": "Dev" })
            ))
        );
    }

    #[gtest]
    fn removes_an_optional_attribute_and_ignores_removing_a_required_one() {
        let result = patched(
            user(),
            Resource::User,
            json!([{ "op": "remove", "path": "externalId" }, { "op": "remove", "path": "userName" }]),
        );

        expect_that!(
            result,
            ok(eq(
                &json!({ "userName": "ana@example.test", "active": true, "title": "Dev" })
            ))
        );
        let group = patched(
            group(),
            Resource::Group,
            json!([{ "op": "remove", "path": "displayName" }]),
        );
        expect_that!(group.map(|g| g["displayName"].clone()), ok(eq(&json!("eng"))));
    }

    #[gtest]
    fn applies_each_key_of_a_no_path_object_and_ignores_the_unknown_and_the_dotted() {
        let result = patched(
            user(),
            Resource::User,
            json!([{ "op": "replace", "value": {
                "active": false, "externalId": "sub-2", "name.givenName": "Ana", "title": "Lead",
            } }]),
        );

        expect_that!(
            result,
            ok(eq(
                &json!({ "userName": "ana@example.test", "externalId": "sub-2", "active": false, "title": "Dev" })
            ))
        );
    }

    #[gtest]
    fn leaves_unknown_paths_and_filters_on_other_attributes_alone() {
        let result = patched(
            user(),
            Resource::User,
            json!([
                { "op": "replace", "path": "name.givenName", "value": "Ana" },
                { "op": "replace", "path": "emails[type eq \"work\"].value", "value": "a@b.c" },
                { "op": "remove", "path": "title" },
                { "op": "add", "path": "members", "value": [{ "value": "x" }] },
            ]),
        );

        expect_that!(result, ok(eq(&Value::Object(user()))));
    }

    #[gtest]
    fn adds_members_and_skips_those_already_there() {
        let result = patched(
            group(),
            Resource::Group,
            json!([{ "op": "Add", "path": "members", "value": [{ "value": "b" }, { "value": "c" }] }]),
        );

        expect_that!(
            result.map(|g| g["members"].clone()),
            ok(eq(&json!([{ "value": "a" }, { "value": "b" }, { "value": "c" }])))
        );
    }

    #[gtest]
    fn removes_one_member_by_a_path_filter() {
        let result = patched(
            group(),
            Resource::Group,
            json!([{ "op": "remove", "path": "members[value eq \"a\"]" }]),
        );

        expect_that!(result.map(|g| g["members"].clone()), ok(eq(&json!([{ "value": "b" }]))));
    }

    #[gtest]
    fn removes_listed_members_or_all_of_them() {
        let listed = patched(
            group(),
            Resource::Group,
            json!([{ "op": "Remove", "path": "members", "value": [{ "value": "b" }] }]),
        );
        let all = patched(group(), Resource::Group, json!([{ "op": "remove", "path": "members" }]));

        expect_that!(listed.map(|g| g["members"].clone()), ok(eq(&json!([{ "value": "a" }]))));
        expect_that!(all.map(|g| g["members"].clone()), ok(eq(&json!([]))));
    }

    #[gtest]
    fn replaces_the_members() {
        let result = patched(
            group(),
            Resource::Group,
            json!([{ "op": "replace", "path": "members", "value": [{ "value": "z" }] }]),
        );

        expect_that!(result.map(|g| g["members"].clone()), ok(eq(&json!([{ "value": "z" }]))));
    }

    #[gtest]
    fn refuses_what_it_cant_read() {
        let cases = [
            (
                json!([{ "op": "move", "path": "active" }]),
                PatchError::Syntax("op must be add, remove or replace"),
            ),
            (json!([{ "op": "remove" }]), PatchError::NoTarget),
            (
                json!([{ "op": "replace", "value": "x" }]),
                PatchError::Value("value must be an object when there is no path"),
            ),
            (
                json!([{ "op": "replace", "path": "externalId" }]),
                PatchError::Value("value is required"),
            ),
            (
                json!([{ "op": "add", "path": "members", "value": [{ "id": "a" }] }]),
                PatchError::Value("members must be a list of objects with a value"),
            ),
            (
                json!([{ "op": "remove", "path": "members[value ne \"a\"]" }]),
                PatchError::Filter,
            ),
            (
                json!([{ "op": "remove", "path": "members[value eq \"a\"" }]),
                PatchError::Syntax("the path's filter isn't closed"),
            ),
        ];
        for (operations, error) in cases {
            expect_that!(
                patched(group(), Resource::Group, operations.clone()),
                err(eq(&error)),
                "{operations}"
            );
        }
        expect_that!(
            apply(&mut group(), Resource::Group, &json!({ "schemas": [] })),
            err(eq(&PatchError::Syntax("Operations must be a list")))
        );
    }
}
