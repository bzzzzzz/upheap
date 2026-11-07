//! Custom parameters: the admin's changes to them and their options, values on items, ranking, and the list's
//! filter and sort by them.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Creates a parameter with `options`, highest priority first, and returns its body.
async fn new_parameter(app: &TestApp, session: &str, heap: &str, name: &str, options: &[&str]) -> Result<Value> {
    let created = create_parameter(app, session, heap, json!({ "name": name, "options": options })).await?;
    anyhow::ensure!(
        created.status == StatusCode::CREATED,
        "create parameter failed: {}",
        created.body
    );
    Ok(created.body)
}

async fn create_parameter(app: &TestApp, session: &str, heap: &str, body: Value) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/heaps/{heap}/parameters"),
        Some(session),
        Some(body),
    )
    .await
}

/// The ID of the option labelled `label`.
fn option_id(parameter: &Value, label: &str) -> String {
    parameter["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["label"] == label))
        .and_then(|o| o["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn id(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

fn labels(parameter: &Value) -> Vec<String> {
    parameter["options"]
        .as_array()
        .map(|options| {
            options
                .iter()
                .filter_map(|o| o["label"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn list(app: &TestApp, session: &str, heap: &str, query: &str) -> Result<TestResponse> {
    app.send(
        Method::GET,
        &format!("/api/v1/heaps/{heap}/items?{query}"),
        Some(session),
        None,
    )
    .await
}

fn titles(group: &Value) -> Vec<String> {
    group
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| entry["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// The option label an item has for the parameter called `name`, or `None` when unset or not listed.
fn value_of<'a>(item: &'a Value, name: &str) -> Option<&'a str> {
    item["parameters"].as_array()?.iter().find(|p| p["name"] == name)?["option"]["label"].as_str()
}

async fn archive_option(app: &TestApp, session: &str, option: &str, replacement: Option<&str>) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/options/{option}/archive"),
        Some(session),
        Some(json!({ "replacement_option_id": replacement })),
    )
    .await
}

async fn archive_parameter(app: &TestApp, session: &str, parameter: &str) -> Result<TestResponse> {
    app.send(
        Method::POST,
        &format!("/api/v1/parameters/{parameter}/archive"),
        Some(session),
        None,
    )
    .await
}

#[gtest]
#[tokio::test]
async fn items_show_every_parameter_in_use_set_or_unset() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise", "Free"]).await?;
    new_parameter(&app, &session, &heap, "Area", &["Infra", "Web"]).await?;
    let enterprise = option_id(&customer, "Enterprise");

    // Act
    let created = app
        .create_item(
            &session,
            &heap,
            json!({ "title": "Fix login", "parameter_values": { id(&customer): enterprise } }),
        )
        .await?;
    let parameters = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/parameters"),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(created.status, eq(StatusCode::CREATED));
    expect_that!(
        created.body["parameters"],
        eq(&json!([
            { "parameter_id": parameters.body["parameters"][0]["id"], "name": "Area", "parameter_archived": false, "option": null },
            { "parameter_id": id(&customer), "name": "Customer", "parameter_archived": false,
              "option": { "id": enterprise, "label": "Enterprise", "archived": false } },
        ]))
    );
    expect_that!(parameters.body["allowed_actions"], eq(&json!(["create"])));
    expect_that!(
        parameters.body["parameters"][1]["allowed_actions"],
        eq(&json!(["edit", "archive"]))
    );
    expect_that!(customer["weight"], eq(&json!(10)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_set_values_but_only_admins_change_parameters() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (admin, heap) = app.register("ada@example.test").await?;
    let (member, _) = app.register("bob@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&admin), None).await?;
    let team = me.body["teams"][0]["id"].as_str().context("no team")?.to_owned();
    let member_me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = member_me.body["user"]["id"].as_str().context("no user")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    let customer = new_parameter(&app, &admin, &heap, "Customer", &["Enterprise", "Free"]).await?;

    // Act
    let set = app
        .create_item(
            &member,
            &heap,
            json!({ "title": "Fix login", "parameter_values": { id(&customer): option_id(&customer, "Free") } }),
        )
        .await?;
    let listed = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/parameters"),
            Some(&member),
            None,
        )
        .await?;

    // Assert
    expect_that!(set.status, eq(StatusCode::CREATED));
    expect_that!(value_of(&set.body, "Customer"), some(eq("Free")));
    expect_that!(listed.body["allowed_actions"], eq(&json!([])));
    expect_that!(listed.body["parameters"][0]["allowed_actions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_patch_sets_and_clears_values_and_refuses_foreign_ones() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise", "Free"]).await?;
    let area = new_parameter(&app, &session, &heap, "Area", &["Infra", "Web"]).await?;
    let foreign = new_parameter(&app, &other_session, &other_heap, "Customer", &["Enterprise"]).await?;
    let item = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Fix login", "parameter_values": { id(&customer): option_id(&customer, "Free") } }),
        )
        .await?;
    let path = format!("/api/v1/items/{}", id(&item));

    // Act
    let edited = app
        .patch(
            &session,
            &path,
            &item["version"],
            json!({ "parameter_values": { id(&customer): null, id(&area): option_id(&area, "Web") } }),
        )
        .await?;
    let other_parameters_option = app
        .patch(
            &session,
            &path,
            &edited.body["version"],
            json!({ "parameter_values": { id(&customer): option_id(&area, "Infra") } }),
        )
        .await?;
    let foreign_parameter = app
        .patch(
            &session,
            &path,
            &edited.body["version"],
            json!({ "parameter_values": { id(&foreign): option_id(&foreign, "Enterprise") } }),
        )
        .await?;
    let null_map = app
        .patch(
            &session,
            &path,
            &edited.body["version"],
            json!({ "parameter_values": null }),
        )
        .await?;

    // Assert
    expect_that!(edited.status, eq(StatusCode::OK));
    expect_that!(value_of(&edited.body, "Customer"), none());
    expect_that!(value_of(&edited.body, "Area"), some(eq("Web")));
    expect_that!(other_parameters_option.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(
        other_parameters_option.body["code"],
        eq(&json!("option_not_in_parameter"))
    );
    expect_that!(other_parameters_option.body["field"], eq(&json!("parameter_values")));
    expect_that!(foreign_parameter.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(foreign_parameter.body["code"], eq(&json!("parameter_not_in_heap")));
    expect_that!(null_map.status, eq(StatusCode::BAD_REQUEST));
    let audit = app.audit_entries(&id(&item)).await?;
    expect_that!(
        audit.last().map(|(_, diff)| diff["parameters"]["after"].clone()),
        some(eq(&json!({ id(&area): option_id(&area, "Web") })))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_higher_option_ranks_an_item_higher_and_explains_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = create_parameter(
        &app,
        &session,
        &heap,
        json!({ "name": "Customer", "weight": 100, "options": ["Enterprise", "Pro", "Free"] }),
    )
    .await?
    .body;
    let value = |label: &str| json!({ id(&customer): option_id(&customer, label) });
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Older, free", "parameter_values": value("Free") }),
    )
    .await?;
    app.clock.advance(domain::Duration::from_days(10));
    app.new_item(&session, &heap, json!({ "title": "Unset" })).await?;
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Enterprise", "parameter_values": value("Enterprise") }),
    )
    .await?;
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Pro", "parameter_values": value("Pro") }),
    )
    .await?;

    // Act
    let ranked = list(&app, &session, &heap, "").await?;

    // Assert
    expect_that!(
        titles(&ranked.body["ranked"]),
        elements_are![eq("Enterprise"), eq("Pro"), eq("Older, free"), eq("Unset")]
    );
    expect_that!(
        ranked.body["ranked"][0]["contributors"][0],
        eq(&json!({ "factor": "parameter", "label": "Customer: Enterprise", "share_percent": 100 }))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn names_and_labels_are_unique_ignoring_case() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise"]).await?;
    let options_path = format!("/api/v1/parameters/{}/options", id(&customer));

    // Act
    let same_name = create_parameter(&app, &session, &heap, json!({ "name": " customer " })).await?;
    let same_labels = create_parameter(
        &app,
        &session,
        &heap,
        json!({ "name": "Area", "options": ["Web", "web"] }),
    )
    .await?;
    let same_label = app
        .send_with(
            Method::POST,
            &options_path,
            Some(&session),
            Some(json!({ "label": "ENTERPRISE" })),
            &[common::CSRF, ("if-match", "1")],
        )
        .await?;
    let archived = archive_parameter(&app, &session, &id(&customer)).await?;
    let after_archiving = create_parameter(&app, &session, &heap, json!({ "name": "Customer" })).await?;

    // Assert
    expect_that!(same_name.status, eq(StatusCode::CONFLICT));
    expect_that!(same_name.body["code"], eq(&json!("parameter_name_taken")));
    expect_that!(same_labels.status, eq(StatusCode::CONFLICT));
    expect_that!(same_labels.body["code"], eq(&json!("option_label_taken")));
    expect_that!(same_label.body["code"], eq(&json!("option_label_taken")));
    expect_that!(same_label.body["field"], eq(&json!("label")));
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(after_archiving.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn options_are_added_last_and_rearranged_as_a_whole() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let area = new_parameter(&app, &session, &heap, "Area", &["Infra", "Web"]).await?;
    let path = format!("/api/v1/parameters/{}/options", id(&area));

    // Act
    let added = app
        .send_with(
            Method::POST,
            &path,
            Some(&session),
            Some(json!({ "label": "Docs" })),
            &[common::CSRF, ("if-match", "1")],
        )
        .await?;
    let stale = app
        .patch(&session, &path, &json!(1), json!({ "options": added.body["options"] }))
        .await?;
    let missing_one = app
        .patch(
            &session,
            &path,
            &added.body["version"],
            json!({ "options": [{ "id": option_id(&area, "Web"), "label": "Web" }] }),
        )
        .await?;
    let arranged = app
        .patch(
            &session,
            &path,
            &added.body["version"],
            json!({ "options": [
                { "id": option_id(&added.body, "Docs"), "label": "Documentation" },
                { "id": option_id(&area, "Web"), "label": "Web" },
                { "id": option_id(&area, "Infra"), "label": "Infra" },
            ] }),
        )
        .await?;

    // Assert
    expect_that!(labels(&added.body), elements_are![eq("Infra"), eq("Web"), eq("Docs")]);
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(missing_one.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(missing_one.body["code"], eq(&json!("options_mismatch")));
    expect_that!(arranged.status, eq(StatusCode::OK));
    expect_that!(
        labels(&arranged.body),
        elements_are![eq("Documentation"), eq("Web"), eq("Infra")]
    );
    expect_that!(arranged.body["version"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archiving_an_option_switches_open_items_to_its_replacement() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise", "Pro", "Free"]).await?;
    let (pro, free) = (option_id(&customer, "Pro"), option_id(&customer, "Free"));
    let value = json!({ id(&customer): pro });
    let open = app
        .new_item(&session, &heap, json!({ "title": "Open", "parameter_values": value }))
        .await?;
    let closed = app
        .new_item(&session, &heap, json!({ "title": "Closed", "parameter_values": value }))
        .await?;
    let closed_path = format!("/api/v1/items/{}", id(&closed));
    app.patch(&session, &closed_path, &closed["version"], json!({ "state": "done" }))
        .await?;

    // Act
    let without_replacement = archive_option(&app, &session, &pro, None).await?;
    let onto_itself = archive_option(&app, &session, &pro, Some(&pro)).await?;
    let archived = archive_option(&app, &session, &pro, Some(&free)).await?;
    let again = archive_option(&app, &session, &pro, None).await?;
    let open_after = app
        .send(
            Method::GET,
            &format!("/api/v1/items/{}", id(&open)),
            Some(&session),
            None,
        )
        .await?;
    let closed_after = app.send(Method::GET, &closed_path, Some(&session), None).await?;
    let stale_edit = app
        .patch(
            &session,
            &format!("/api/v1/items/{}", id(&open)),
            &open["version"],
            json!({ "title": "Renamed" }),
        )
        .await?;

    // Assert
    expect_that!(without_replacement.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(without_replacement.body["code"], eq(&json!("replacement_required")));
    expect_that!(onto_itself.body["code"], eq(&json!("replacement_required")));
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(labels(&archived.body), elements_are![eq("Enterprise"), eq("Free")]);
    expect_that!(
        archived.body["archived_options"][0]["replaced_by_option_id"],
        eq(&json!(free))
    );
    expect_that!(again.body["version"], eq(&archived.body["version"]));
    expect_that!(value_of(&open_after.body, "Customer"), some(eq("Free")));
    expect_that!(open_after.body["version"], eq(&json!(2)));
    expect_that!(closed_after.body["parameters"][0]["option"]["label"], eq(&json!("Pro")));
    expect_that!(
        closed_after.body["parameters"][0]["option"]["archived"],
        eq(&json!(true))
    );
    expect_that!(stale_edit.body["code"], eq(&json!("version_conflict")));
    let audit = app.audit_entries(&id(&customer)).await?;
    expect_that!(
        audit.last().map(|(action, _)| action.as_str()),
        some(eq("option_archived"))
    );
    expect_that!(
        audit.last().map(|(_, diff)| diff["items_switched"].clone()),
        some(eq(&json!([id(&open)])))
    );
    let item_audit = app.audit_entries(&id(&open)).await?;
    expect_that!(
        item_audit.last(),
        some(eq(&(
            "item_updated".to_owned(),
            json!({ "parameters": {
                "before": { id(&customer): pro },
                "after": { id(&customer): free },
            } })
        )))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_is_archived_only_once_no_open_item_uses_it() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise"]).await?;
    let value = json!({ id(&customer): option_id(&customer, "Enterprise") });
    let open = app
        .new_item(&session, &heap, json!({ "title": "Open", "parameter_values": value }))
        .await?;
    let closed = app
        .new_item(&session, &heap, json!({ "title": "Closed", "parameter_values": value }))
        .await?;
    app.patch(
        &session,
        &format!("/api/v1/items/{}", id(&closed)),
        &closed["version"],
        json!({ "state": "wont_do" }),
    )
    .await?;

    // Act
    let in_use = archive_parameter(&app, &session, &id(&customer)).await?;
    app.patch(
        &session,
        &format!("/api/v1/items/{}", id(&open)),
        &open["version"],
        json!({ "parameter_values": { id(&customer): null } }),
    )
    .await?;
    let archived = archive_parameter(&app, &session, &id(&customer)).await?;
    let rename = app
        .patch(
            &session,
            &format!("/api/v1/parameters/{}", id(&customer)),
            &archived.body["version"],
            json!({ "name": "Client" }),
        )
        .await?;
    let listed = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/parameters"),
            Some(&session),
            None,
        )
        .await?;
    let closed_page = app
        .send(
            Method::GET,
            &format!("/api/v1/heaps/{heap}/items/closed"),
            Some(&session),
            None,
        )
        .await?;
    let open_after = app
        .send(
            Method::GET,
            &format!("/api/v1/items/{}", id(&open)),
            Some(&session),
            None,
        )
        .await?;

    // Assert
    expect_that!(in_use.status, eq(StatusCode::CONFLICT));
    expect_that!(in_use.body["code"], eq(&json!("parameter_in_use")));
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["allowed_actions"], eq(&json!([])));
    expect_that!(rename.body["code"], eq(&json!("parameter_archived")));
    expect_that!(listed.body["parameters"], eq(&json!([])));
    expect_that!(listed.body["archived"][0]["name"], eq(&json!("Customer")));
    expect_that!(
        closed_page.body["items"][0]["parameters"][0]["parameter_archived"],
        eq(&json!(true))
    );
    expect_that!(open_after.body["parameters"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reopened_item_takes_up_replacements_and_drops_archived_parameters() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Pro", "Free"]).await?;
    let area = new_parameter(&app, &session, &heap, "Area", &["Infra"]).await?;
    let (pro, free) = (option_id(&customer, "Pro"), option_id(&customer, "Free"));
    let item = app
        .new_item(
            &session,
            &heap,
            json!({ "title": "Closed", "parameter_values": {
                id(&customer): pro, id(&area): option_id(&area, "Infra")
            } }),
        )
        .await?;
    let path = format!("/api/v1/items/{}", id(&item));
    let closed = app
        .patch(&session, &path, &item["version"], json!({ "state": "done" }))
        .await?;
    archive_option(&app, &session, &pro, Some(&free)).await?;
    archive_parameter(&app, &session, &id(&area)).await?;

    // Act
    let unknown_while_closed = app
        .patch(
            &session,
            &path,
            &closed.body["version"],
            json!({ "parameter_values": { id(&item): null } }),
        )
        .await?;
    let reopened = app
        .patch(&session, &path, &closed.body["version"], json!({ "state": "open" }))
        .await?;

    // Assert
    expect_that!(unknown_while_closed.body["code"], eq(&json!("item_closed")));
    expect_that!(reopened.status, eq(StatusCode::OK));
    expect_that!(value_of(&reopened.body, "Customer"), some(eq("Free")));
    expect_that!(value_of(&reopened.body, "Area"), none());
    let audit = app.audit_entries(&id(&item)).await?;
    expect_that!(
        audit.last().map(|(_, diff)| diff["parameters"].is_null()),
        some(eq(false))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameters_are_linked_to_tags_of_their_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let (other_session, other_heap) = app.register("bob@example.test").await?;
    let ops = app.new_tag_id(&session, &heap, "ops").await?;
    let foreign = app.new_tag_id(&other_session, &other_heap, "ops").await?;
    let customer = create_parameter(&app, &session, &heap, json!({ "name": "Customer", "tag_ids": [ops] }))
        .await?
        .body;
    let path = format!("/api/v1/parameters/{}/tags", id(&customer));

    // Act
    let foreign_link = app
        .send_with(
            Method::PUT,
            &path,
            Some(&session),
            Some(json!({ "tag_ids": [foreign] })),
            &[common::CSRF, ("if-match", "1")],
        )
        .await?;
    let unlinked = app
        .send_with(
            Method::PUT,
            &path,
            Some(&session),
            Some(json!({ "tag_ids": [] })),
            &[common::CSRF, ("if-match", "1")],
        )
        .await?;
    let relinked = app
        .send_with(
            Method::PUT,
            &path,
            Some(&session),
            Some(json!({ "tag_ids": [ops] })),
            &[common::CSRF, ("if-match", "2")],
        )
        .await?;

    // Assert
    expect_that!(customer["tag_ids"], eq(&json!([ops])));
    expect_that!(foreign_link.body["code"], eq(&json!("tag_not_in_heap")));
    expect_that!(unlinked.body["tag_ids"], eq(&json!([])));
    expect_that!(relinked.body["tag_ids"], eq(&json!([ops])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_list_filters_and_sorts_by_a_parameter() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise", "Pro", "Free"]).await?;
    let area = new_parameter(&app, &session, &heap, "Area", &["Infra", "Web"]).await?;
    let (enterprise, pro, free) = (
        option_id(&customer, "Enterprise"),
        option_id(&customer, "Pro"),
        option_id(&customer, "Free"),
    );
    let item = |title: &str, customer_option: Option<&str>, area_option: &str| {
        let mut values = serde_json::Map::new();
        if let Some(option) = customer_option {
            values.insert(id(&customer), json!(option));
        }
        values.insert(id(&area), json!(option_id(&area, area_option)));
        json!({ "title": title, "effort": "s", "parameter_values": values })
    };
    app.new_item(&session, &heap, item("Free infra", Some(&free), "Infra"))
        .await?;
    app.new_item(&session, &heap, item("Unset infra", None, "Infra"))
        .await?;
    app.new_item(&session, &heap, item("Enterprise web", Some(&enterprise), "Web"))
        .await?;
    app.new_item(&session, &heap, item("Pro infra", Some(&pro), "Infra"))
        .await?;
    let infra = option_id(&area, "Infra");

    // Act
    let sorted = list(
        &app,
        &session,
        &heap,
        &format!("sort=parameter&sort_parameter={}", id(&customer)),
    )
    .await?;
    let filtered = list(&app, &session, &heap, &format!("param={pro},{free},{infra}")).await?;
    let missing_parameter = list(&app, &session, &heap, "sort=parameter").await?;
    let stray_parameter = list(
        &app,
        &session,
        &heap,
        &format!("sort=due&sort_parameter={}", id(&customer)),
    )
    .await?;
    let unknown_option = list(&app, &session, &heap, &format!("param={}", id(&customer))).await?;

    // Assert
    expect_that!(
        titles(&sorted.body["ranked"]),
        elements_are![
            eq("Enterprise web"),
            eq("Pro infra"),
            eq("Free infra"),
            eq("Unset infra")
        ]
    );
    expect_that!(
        titles(&filtered.body["ranked"]),
        unordered_elements_are![eq("Free infra"), eq("Pro infra")]
    );
    expect_that!(missing_parameter.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(missing_parameter.body["code"], eq(&json!("sort_parameter_mismatch")));
    expect_that!(stray_parameter.body["code"], eq(&json!("sort_parameter_mismatch")));
    expect_that!(unknown_option.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(unknown_option.body["code"], eq(&json!("option_not_in_heap")));
    expect_that!(unknown_option.body["field"], eq(&json!("param")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_saved_view_keeps_filtering_after_its_option_is_archived() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let customer = new_parameter(&app, &session, &heap, "Customer", &["Enterprise", "Pro", "Free"]).await?;
    let (pro, free) = (option_id(&customer, "Pro"), option_id(&customer, "Free"));
    app.new_item(
        &session,
        &heap,
        json!({ "title": "Pro", "parameter_values": { id(&customer): pro } }),
    )
    .await?;
    app.new_item(&session, &heap, json!({ "title": "Unset" })).await?;
    let saved = app
        .send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/views"),
            Some(&session),
            Some(json!({ "name": "Paying", "view": {
                "param": [pro], "sort": "parameter", "sort_parameter": id(&customer),
            } })),
        )
        .await?;
    let foreign_option = app
        .send(
            Method::POST,
            &format!("/api/v1/heaps/{heap}/views"),
            Some(&session),
            Some(json!({ "name": "Wrong", "view": { "param": [id(&customer)] } })),
        )
        .await?;

    // Act
    archive_option(&app, &session, &pro, Some(&free)).await?;
    let view = &saved.body["view"];
    let query = format!(
        "param={}&sort={}&sort_parameter={}",
        view["param"][0].as_str().unwrap_or_default(),
        view["sort"].as_str().unwrap_or_default(),
        view["sort_parameter"].as_str().unwrap_or_default(),
    );
    let shown = list(&app, &session, &heap, &query).await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::CREATED));
    expect_that!(view["param"], eq(&json!([pro])));
    expect_that!(foreign_option.body["field"], eq(&json!("view.param")));
    expect_that!(shown.status, eq(StatusCode::OK));
    expect_that!(titles(&shown.body["ranked"]), elements_are![eq("Pro")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn parameter_input_is_checked() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let many: Vec<String> = (0..=50).map(|n| format!("Option {n}")).collect();

    // Act
    let heavy = create_parameter(&app, &session, &heap, json!({ "name": "Area", "weight": 101 })).await?;
    let negative = create_parameter(&app, &session, &heap, json!({ "name": "Area", "weight": -1 })).await?;
    let blank_label = create_parameter(&app, &session, &heap, json!({ "name": "Area", "options": [" "] })).await?;
    let too_many = create_parameter(&app, &session, &heap, json!({ "name": "Area", "options": many })).await?;
    let unknown_field = create_parameter(&app, &session, &heap, json!({ "name": "Area", "option": ["Web"] })).await?;

    // Assert
    expect_that!(heavy.body["code"], eq(&json!("weight_out_of_range")));
    expect_that!(heavy.body["field"], eq(&json!("weight")));
    expect_that!(negative.body["code"], eq(&json!("weight_out_of_range")));
    expect_that!(blank_label.body["code"], eq(&json!("field_empty")));
    expect_that!(blank_label.body["field"], eq(&json!("options")));
    expect_that!(too_many.body["code"], eq(&json!("too_many_options")));
    expect_that!(unknown_field.body["code"], eq(&json!("invalid_body")));
    Ok(())
}
