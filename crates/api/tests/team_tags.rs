//! Team tags: a team's own tags, which every heap of the team has, and how they sit with the heaps' tags.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A team with two heaps, and its admin.
struct World {
    app: TestApp,
    session: String,
    team_path: String,
    first: String,
    second: String,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, first) = app.shared_heap(&session).await?;
    let team_path = app.team_url(&team).await?;
    let created = app
        .send(
            Method::POST,
            &format!("{team_path}/heaps"),
            Some(&session),
            Some(json!({ "name": "Second" })),
        )
        .await?;
    let second = created.body["id"].as_str().context("heap has no id")?.to_owned();
    Ok(World {
        app,
        session,
        team_path,
        first,
        second,
    })
}

impl World {
    async fn make_team_tag(&self, name: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/tags", self.team_path),
                Some(&self.session),
                Some(json!({ "name": name })),
            )
            .await
    }

    /// A team tag that must succeed.
    async fn new_team_tag(&self, name: &str) -> Result<Value> {
        let made = self.make_team_tag(name).await?;
        anyhow::ensure!(
            made.status == StatusCode::CREATED,
            "make team tag failed: {}",
            made.body
        );
        Ok(made.body)
    }

    async fn rename_team_tag(&self, tag: &Value, name: &str) -> Result<TestResponse> {
        let path = format!("{}/tags/{}", self.team_path, tag["id"].as_str().unwrap_or_default());
        self.app
            .patch(&self.session, &path, &tag["version"], json!({ "name": name }))
            .await
    }

    async fn heap_tags(&self, heap: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("{}/tags", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await
    }
}

fn names<'a>(list: &'a Value, field: &str) -> Vec<&'a str> {
    list[field]
        .as_array()
        .map(|tags| tags.iter().filter_map(|t| t["name"].as_str()).collect())
        .unwrap_or_default()
}

fn titles(response: &TestResponse) -> Vec<&str> {
    response.body["ranked"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["item"]["title"].as_str()).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn renaming_a_team_tag_shows_at_once_on_the_items_of_every_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.new_team_tag("docs").await?;
    let tag_id = tag["id"].as_str().context("tag has no id")?;
    let one = w
        .app
        .new_item(&w.session, &w.first, json!({ "title": "One", "tag_ids": [tag_id] }))
        .await?;
    let two = w
        .app
        .new_item(&w.session, &w.second, json!({ "title": "Two", "tag_ids": [tag_id] }))
        .await?;

    // Act
    let renamed = w.rename_team_tag(&tag, "documentation").await?;
    let read = |item: &Value| {
        let id = item["id"].as_str().unwrap_or_default().to_owned();
        let app = &w.app;
        let session = &w.session;
        async move {
            let path = app.item_url(&id).await?;
            app.send(Method::GET, &path, Some(session), None).await
        }
    };
    let (item_one, item_two) = (read(&one).await?, read(&two).await?);
    let (tags_one, tags_two) = (w.heap_tags(&w.first).await?, w.heap_tags(&w.second).await?);

    // Assert
    expect_that!(tag["kind"], eq(&json!("team")));
    expect_that!(tag["heap_id"], eq(&Value::Null));
    expect_that!(renamed.status, eq(StatusCode::OK));
    for item in [&item_one, &item_two] {
        expect_that!(item.body["tags"][0]["name"], eq(&json!("documentation")));
        expect_that!(item.body["tags"][0]["id"], eq(&json!(tag_id)));
    }
    for list in [&tags_one, &tags_two] {
        expect_that!(names(&list.body, "tags"), is_empty());
        expect_that!(names(&list.body, "team_tags"), elements_are![eq(&"documentation")]);
        expect_that!(list.body["team_tags"][0]["kind"], eq(&json!("team")));
        expect_that!(list.body["team_tags"][0]["heap_id"], eq(&Value::Null));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_team_lists_its_tags_and_a_heap_lists_its_own_with_the_teams() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.new_team_tag("Zeta").await?;
    w.new_team_tag("alpha").await?;
    let own = w.app.new_tag_id(&w.session, &w.first, "mid").await?;
    w.app.new_tag_id(&w.session, &w.second, "elsewhere").await?;

    // Act
    let team_list = w
        .app
        .send(Method::GET, &format!("{}/tags", w.team_path), Some(&w.session), None)
        .await?;
    let heap_list = w.heap_tags(&w.first).await?;

    // Assert
    expect_that!(team_list.status, eq(StatusCode::OK));
    expect_that!(names(&team_list.body, "tags"), elements_are![eq(&"alpha"), eq(&"Zeta")]);
    expect_that!(team_list.body["allowed_actions"], eq(&json!(["create"])));
    expect_that!(team_list.body["tags"][0]["allowed_actions"], eq(&json!(["edit"])));
    expect_that!(names(&heap_list.body, "tags"), elements_are![eq(&"mid")]);
    expect_that!(
        names(&heap_list.body, "team_tags"),
        elements_are![eq(&"alpha"), eq(&"Zeta")]
    );
    expect_that!(heap_list.body["tags"][0]["kind"], eq(&json!("heap")));
    expect_that!(heap_list.body["tags"][0]["id"], eq(&json!(own)));
    expect_that!(heap_list.body["tags"][0]["heap_id"], eq(&json!(w.first)));
    expect_that!(heap_list.body["team_tags"][0]["kind"], eq(&json!("team")));
    expect_that!(heap_list.body["team_tags"][0]["allowed_actions"], eq(&json!(["edit"])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_tag_cannot_take_the_name_of_a_team_tag() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.new_team_tag("bug").await?;
    let own = w.app.create_tag(&w.session, &w.first, "crash").await?;
    let own_path = format!(
        "{}/tags/{}",
        w.app.heap_url(&w.first).await?,
        own.body["id"].as_str().unwrap_or_default()
    );

    // Act
    let created = w.app.create_tag(&w.session, &w.second, "BUG").await?;
    let renamed = w
        .app
        .patch(&w.session, &own_path, &own.body["version"], json!({ "name": "Bug" }))
        .await?;

    // Assert
    for refused in [&created, &renamed] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("tag_name_taken_by_team_tag")));
        expect_that!(
            refused.body["title"],
            eq(&json!("\"bug\" is a team tag; use it instead"))
        );
        expect_that!(refused.body["field"], eq(&json!("name")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_tag_cannot_take_a_name_a_heap_tag_of_the_team_has() -> Result<()> {
    // Arrange
    let w = world().await?;
    w.app.new_tag_id(&w.session, &w.second, "Docs").await?;
    let spare = w.new_team_tag("spare").await?;
    let (other_session, other_heap) = w.app.register("bob@example.test").await?;
    w.app.new_tag_id(&other_session, &other_heap, "elsewhere").await?;

    // Act
    let made = w.make_team_tag("docs").await?;
    let renamed = w.rename_team_tag(&spare, "DOCS").await?;
    let other_teams_name = w.make_team_tag("elsewhere").await?;

    // Assert
    for refused in [&made, &renamed] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("tag_name_taken")));
    }
    expect_that!(other_teams_name.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn team_tag_names_are_unique_and_edits_check_the_version() -> Result<()> {
    // Arrange
    let w = world().await?;
    let docs = w.new_team_tag("Docs").await?;
    let ops = w.new_team_tag("ops").await?;

    // Act
    let again = w.make_team_tag(" docs ").await?;
    let onto = w.rename_team_tag(&ops, "DOCS").await?;
    let recase = w.rename_team_tag(&docs, "docs").await?;
    let stale = w.rename_team_tag(&docs, "Documentation").await?;
    let unchanged = w.rename_team_tag(&recase.body, "docs").await?;

    // Assert
    expect_that!(again.body["code"], eq(&json!("tag_name_taken")));
    expect_that!(onto.body["code"], eq(&json!("tag_name_taken")));
    expect_that!(recase.status, eq(StatusCode::OK));
    expect_that!(recase.body["version"], eq(&json!(2)));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(unchanged.status, eq(StatusCode::OK));
    expect_that!(unchanged.body["version"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_tag_of_another_team_is_not_a_tag_of_the_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (other_session, _) = w.app.register("bob@example.test").await?;
    let (other_team, other_heap) = w.app.shared_heap(&other_session).await?;
    let other_path = w.app.team_url(&other_team).await?;
    let foreign = w
        .app
        .send(
            Method::POST,
            &format!("{other_path}/tags"),
            Some(&other_session),
            Some(json!({ "name": "theirs" })),
        )
        .await?;
    let theirs = foreign.body["id"].as_str().context("no id")?;
    let ours = w.new_team_tag("ours").await?;
    let item = w.app.new_item(&w.session, &w.first, json!({ "title": "Task" })).await?;

    // Act
    let on_item = w
        .app
        .create_item(&w.session, &w.first, json!({ "title": "x", "tag_ids": [theirs] }))
        .await?;
    let by_other_team = w
        .app
        .patch(
            &w.session,
            &format!("{other_path}/tags/{theirs}"),
            &foreign.body["version"],
            json!({ "name": "mine" }),
        )
        .await?;
    let heap_path = format!(
        "{}/tags/{}",
        w.app.heap_url(&w.first).await?,
        ours["id"].as_str().unwrap_or_default()
    );
    let by_heap_path = w
        .app
        .patch(&w.session, &heap_path, &ours["version"], json!({ "name": "renamed" }))
        .await?;
    let heap_tag = w.app.create_tag(&w.session, &other_heap, "x").await?;
    let heap_tag_on_team_path = w
        .app
        .patch(
            &other_session,
            &format!("{other_path}/tags/{}", heap_tag.body["id"].as_str().unwrap_or_default()),
            &heap_tag.body["version"],
            json!({ "name": "y" }),
        )
        .await?;

    // Assert
    expect_that!(item.get("id"), some(anything()));
    expect_that!(on_item.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(on_item.body["code"], eq(&json!("tag_not_in_heap")));
    expect_that!(by_other_team.status, eq(StatusCode::NOT_FOUND));
    expect_that!(by_heap_path.status, eq(StatusCode::NOT_FOUND));
    expect_that!(heap_tag_on_team_path.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn filters_views_parameters_and_the_tag_order_take_a_team_tag() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.new_team_tag("infra").await?;
    let tag_id = tag["id"].as_str().context("no id")?;
    w.app
        .new_item(&w.session, &w.first, json!({ "title": "Tagged", "tag_ids": [tag_id] }))
        .await?;
    w.app
        .new_item(&w.session, &w.first, json!({ "title": "Plain" }))
        .await?;
    w.app
        .new_item(
            &w.session,
            &w.second,
            json!({ "title": "Elsewhere", "tag_ids": [tag_id] }),
        )
        .await?;
    let heap_path = w.app.heap_url(&w.first).await?;
    let parameter = w
        .app
        .send(
            Method::POST,
            &format!("{heap_path}/parameters"),
            Some(&w.session),
            Some(json!({ "name": "Customer", "options": ["A"] })),
        )
        .await?;

    // Act
    let filtered = w
        .app
        .send(
            Method::GET,
            &format!("{heap_path}/items?tag={tag_id}"),
            Some(&w.session),
            None,
        )
        .await?;
    let view = w
        .app
        .send(
            Method::POST,
            &format!("{heap_path}/views"),
            Some(&w.session),
            Some(json!({ "name": "Infra", "view": { "tag": [tag_id] } })),
        )
        .await?;
    let linked = w
        .app
        .send_with(
            Method::PUT,
            &format!(
                "{heap_path}/parameters/{}/tags",
                parameter.body["id"].as_str().unwrap_or_default()
            ),
            Some(&w.session),
            Some(json!({ "tag_ids": [tag_id] })),
            &[common::CSRF, ("if-match", &parameter.body["version"].to_string())],
        )
        .await?;
    let preview = w
        .app
        .send(
            Method::POST,
            &format!("{heap_path}/ranking/preview"),
            Some(&w.session),
            Some(json!({ "tags": 40, "tag_order": [tag_id] })),
        )
        .await?;
    let heap = w.app.send(Method::GET, &heap_path, Some(&w.session), None).await?;
    let saved = w
        .app
        .patch(
            &w.session,
            &format!("{heap_path}/weights"),
            &heap.body["version"],
            json!({ "tags": 40, "tag_order": [tag_id] }),
        )
        .await?;

    // Assert
    expect_that!(filtered.status, eq(StatusCode::OK));
    expect_that!(titles(&filtered), elements_are![eq(&"Tagged")]);
    expect_that!(view.status, eq(StatusCode::CREATED));
    expect_that!(view.body["view"]["tag"], eq(&json!([tag_id])));
    expect_that!(linked.status, eq(StatusCode::OK));
    expect_that!(linked.body["tag_ids"], eq(&json!([tag_id])));
    expect_that!(preview.status, eq(StatusCode::OK));
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(saved.body["heap"]["tag_order"], eq(&json!([tag_id])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn making_and_renaming_a_team_tag_write_team_level_audit_entries() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.new_team_tag("docs").await?;
    let tag_id = tag["id"].as_str().context("no id")?;
    w.rename_team_tag(&tag, "guides").await?;

    // Act
    let entries = w.app.audit_heaps(tag_id).await?;
    let team_log = w
        .app
        .send(Method::GET, &format!("{}/audit", w.team_path), Some(&w.session), None)
        .await?;

    // Assert
    expect_that!(
        entries,
        elements_are![
            eq(&("tag_created".to_owned(), None)),
            eq(&("tag_updated".to_owned(), None))
        ]
    );
    expect_that!(team_log.status, eq(StatusCode::OK));
    let labelled: Vec<(&Value, &Value)> = team_log.body["entries"]
        .as_array()
        .context("no entries")?
        .iter()
        .filter(|e| e["action"] == json!("tag_updated") || e["action"] == json!("tag_created"))
        .map(|e| (&e["action"], &e["entity"]["label"]))
        .collect();
    expect_that!(
        labelled,
        unordered_elements_are![
            eq(&(&json!("tag_created"), &json!("guides"))),
            eq(&(&json!("tag_updated"), &json!("guides")))
        ]
    );
    Ok(())
}
