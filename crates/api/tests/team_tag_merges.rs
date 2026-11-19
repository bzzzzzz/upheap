//! Team tag merges: making or renaming a team tag into a name heap tags of the team have merges them into it, in
//! every heap of the team, once the caller has confirmed what it reaches.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use googletest::prelude::*;
use serde_json::{Value, json};

/// A team with three heaps, A, B and C, and its admin.
struct World {
    app: TestApp,
    session: String,
    team_path: String,
    a: String,
    b: String,
    c: String,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, a) = app.shared_heap(&session).await?;
    let team_path = app.team_url(&team).await?;
    let mut others = Vec::new();
    for name in ["B", "C"] {
        let created = app
            .send(
                Method::POST,
                &format!("{team_path}/heaps"),
                Some(&session),
                Some(json!({ "name": name })),
            )
            .await?;
        others.push(created.body["id"].as_str().context("heap has no id")?.to_owned());
    }
    let c = others.pop().context("no heap C")?;
    let b = others.pop().context("no heap B")?;
    Ok(World {
        app,
        session,
        team_path,
        a,
        b,
        c,
    })
}

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap_or_default()
}

impl World {
    async fn make_team_tag(&self, body: Value, confirm: Option<&str>) -> Result<TestResponse> {
        let query = confirm
            .map(|token| format!("?confirm_reach={token}"))
            .unwrap_or_default();
        self.app
            .send(
                Method::POST,
                &format!("{}/tags{query}", self.team_path),
                Some(&self.session),
                Some(body),
            )
            .await
    }

    async fn rename_team_tag(&self, tag: &Value, name: &str, confirm: Option<&str>) -> Result<TestResponse> {
        let query = confirm
            .map(|token| format!("?confirm_reach={token}"))
            .unwrap_or_default();
        let path = format!("{}/tags/{}{query}", self.team_path, id(tag));
        self.app
            .patch(&self.session, &path, &tag["version"], json!({ "name": name }))
            .await
    }

    /// A heap tag that must succeed.
    async fn heap_tag(&self, heap: &str, name: &str) -> Result<String> {
        self.app.new_tag_id(&self.session, heap, name).await
    }

    async fn item(&self, heap: &str, title: &str, tags: &[&str]) -> Result<Value> {
        self.app
            .new_item(&self.session, heap, json!({ "title": title, "tag_ids": tags }))
            .await
    }

    async fn get(&self, path: &str) -> Result<TestResponse> {
        self.app.send(Method::GET, path, Some(&self.session), None).await
    }

    async fn read_item(&self, item: &Value) -> Result<Value> {
        Ok(self.get(&self.app.item_url(id(item)).await?).await?.body)
    }

    async fn heap_tags(&self, heap: &str) -> Result<Value> {
        Ok(self
            .get(&format!("{}/tags", self.app.heap_url(heap).await?))
            .await?
            .body)
    }

    async fn history(&self, heap: &str, entity: &str) -> Result<Value> {
        Ok(self
            .get(&format!("{}/audit?entity_id={entity}", self.app.heap_url(heap).await?))
            .await?
            .body)
    }
}

fn names<'a>(list: &'a Value, field: &str) -> Vec<&'a str> {
    list[field]
        .as_array()
        .map(|tags| tags.iter().filter_map(|t| t["name"].as_str()).collect())
        .unwrap_or_default()
}

fn tag_ids(item: &Value) -> Vec<&str> {
    item["tags"]
        .as_array()
        .map(|tags| tags.iter().filter_map(|t| t["id"].as_str()).collect())
        .unwrap_or_default()
}

fn actions(history: &Value) -> Vec<&str> {
    history["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn renaming_a_team_tag_back_merges_the_heap_tag_that_took_its_old_name() -> Result<()> {
    // Arrange
    let w = world().await?;
    let team_tag = w.make_team_tag(json!({ "name": "security" }), None).await?.body;
    let renamed = w.rename_team_tag(&team_tag, "security-review", None).await?;
    let heap_tag = w.heap_tag(&w.c, "security").await?;
    let one = w.item(&w.c, "One", &[&heap_tag]).await?;
    let two = w.item(&w.c, "Two", &[&heap_tag]).await?;

    // Act
    let refused = w.rename_team_tag(&renamed.body, "security", None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let confirmed = w.rename_team_tag(&renamed.body, "security", Some(&token)).await?;
    let (one, two) = (w.read_item(&one).await?, w.read_item(&two).await?);
    let tags = w.heap_tags(&w.c).await?;
    let again = w.app.create_tag(&w.session, &w.c, "security").await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("tag_merge_unconfirmed")));
    expect_that!(
        refused.body["title"],
        eq(&json!("Making \"security\" a team tag merges 1 heap tag into it"))
    );
    let reach = &refused.body["merge_reach"]["heaps"];
    expect_that!(reach.as_array().map(Vec::len), some(eq(1)));
    expect_that!(reach[0]["heap"]["id"], eq(&json!(w.c)));
    expect_that!(reach[0]["archived"], eq(&json!(false)));
    expect_that!(reach[0]["tag_id"], eq(&json!(heap_tag)));
    expect_that!(reach[0]["tag_name"], eq(&json!("security")));
    expect_that!(reach[0]["items"], eq(&json!(2)));
    expect_that!(confirmed.status, eq(StatusCode::OK));
    expect_that!(confirmed.body["name"], eq(&json!("security")));
    for item in [&one, &two] {
        expect_that!(tag_ids(item), elements_are![eq(&id(&team_tag))]);
    }
    expect_that!(names(&tags, "tags"), is_empty());
    expect_that!(names(&tags, "team_tags"), elements_are![eq(&"security")]);
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(again.body["code"], eq(&json!("tag_name_taken_by_team_tag")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_heap_is_reached_and_keeps_the_team_tag_when_unarchived() -> Result<()> {
    // Arrange
    let w = world().await?;
    let in_a = w.heap_tag(&w.a, "billing").await?;
    let in_z = w.heap_tag(&w.b, "billing").await?;
    w.item(&w.a, "In A", &[&in_a]).await?;
    let z_item = w.item(&w.b, "In Z", &[&in_z]).await?;
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{}/archive", w.app.heap_url(&w.b).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let promote = json!({ "name": "billing", "description": "Money matters" });

    // Act
    let refused = w.make_team_tag(promote.clone(), None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let made = w.make_team_tag(promote, Some(&token)).await?;
    let unarchived = w
        .app
        .send(
            Method::POST,
            &format!("{}/unarchive", w.app.heap_url(&w.b).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let z_item = w.read_item(&z_item).await?;
    let tags = w.heap_tags(&w.b).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(unarchived.status, eq(StatusCode::OK));
    let reach = refused.body["merge_reach"]["heaps"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    expect_that!(reach.len(), eq(2));
    let archived_entry = reach.iter().find(|entry| entry["heap"]["id"] == json!(w.b));
    expect_that!(
        archived_entry.map(|entry| entry["archived"].clone()),
        some(eq(&json!(true)))
    );
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(made.body["description"], eq(&json!("Money matters")));
    expect_that!(tag_ids(&z_item), elements_are![eq(&id(&made.body))]);
    expect_that!(names(&tags, "tags"), is_empty());
    expect_that!(names(&tags, "team_tags"), elements_are![eq(&"billing")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_closed_item_is_merged_and_its_history_says_so() -> Result<()> {
    // Arrange
    let w = world().await?;
    let billing = w.heap_tag(&w.a, "billing").await?;
    let infra_a = w.heap_tag(&w.a, "infra").await?;
    let infra_b = w.heap_tag(&w.b, "infra").await?;
    let closed = w.item(&w.a, "Closed", &[&billing]).await?;
    let closed_url = w.app.item_url(id(&closed)).await?;
    let done = w
        .app
        .patch(&w.session, &closed_url, &closed["version"], json!({ "state": "done" }))
        .await?;
    anyhow::ensure!(done.status == StatusCode::OK, "close failed: {}", done.body);
    let team_infra = w.make_team_tag(json!({ "name": "shared" }), None).await?.body;
    let mover = w.item(&w.a, "Mover", &[&infra_a, id(&team_infra)]).await?;

    // Act
    let refused = w.make_team_tag(json!({ "name": "billing" }), None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let made = w.make_team_tag(json!({ "name": "billing" }), Some(&token)).await?;
    let closed = w.read_item(&closed).await?;
    let history = w.history(&w.a, id(&closed)).await?;
    let team_activity = w.get(&format!("{}/audit", w.team_path)).await?;
    let mover_url = w.app.item_url(id(&mover)).await?;
    let moved = w
        .app
        .send_with(
            Method::POST,
            &format!("{mover_url}/move"),
            Some(&w.session),
            Some(json!({ "heap_id": w.b })),
            &[common::CSRF, ("if-match", &mover["version"].to_string())],
        )
        .await?;

    // Assert
    expect_that!(refused.body["merge_reach"]["heaps"][0]["items"], eq(&json!(1)));
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(tag_ids(&closed), elements_are![eq(&id(&made.body))]);
    expect_that!(
        actions(&history),
        elements_are![eq(&"tag_merged"), eq(&"item_updated"), eq(&"item_created")]
    );
    let merged = &history["entries"][0];
    expect_that!(merged["phrase"], eq(&json!("merged a tag on")));
    expect_that!(
        merged["changes"],
        eq(&json!([
            { "field": "tags", "label": "Tags", "before": "billing", "after": "billing", "text_diff": null },
        ]))
    );
    expect_that!(merged["diff"]["tags"]["before"], eq(&json!([billing])));
    expect_that!(merged["diff"]["tags"]["after"], eq(&json!([id(&made.body)])));
    // An entry from before the merge still names the heap tag by its kept name.
    let earlier = history["entries"][2]["changes"].as_array().cloned().unwrap_or_default();
    expect_that!(
        earlier,
        contains(eq(
            &json!({ "field": "tags", "label": "Tags", "before": null, "after": "billing", "text_diff": null })
        ))
    );
    expect_that!(actions(&team_activity.body), contains(eq(&"tag_merged")));
    expect_that!(moved.status, eq(StatusCode::OK));
    let moved = w.read_item(&mover).await?;
    let mut expected = vec![id(&team_infra), infra_b.as_str()];
    expected.sort_unstable();
    let mut actual = tag_ids(&moved);
    actual.sort_unstable();
    expect_that!(actual, eq(&expected));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn views_parameters_and_tag_orders_follow_the_merge() -> Result<()> {
    // Arrange
    let w = world().await?;
    let billing = w.heap_tag(&w.a, "billing").await?;
    let x = w.heap_tag(&w.a, "x").await?;
    w.heap_tag(&w.b, "billing").await?;
    w.item(&w.a, "Invoices", &[&billing]).await?;
    w.item(&w.a, "Other", &[&x]).await?;
    let heap_url = w.app.heap_url(&w.a).await?;
    let view = w
        .app
        .send(
            Method::POST,
            &format!("{heap_url}/views"),
            Some(&w.session),
            Some(json!({ "name": "Money", "view": { "tag": [billing] } })),
        )
        .await?;
    let parameter = w
        .app
        .send(
            Method::POST,
            &format!("{heap_url}/parameters"),
            Some(&w.session),
            Some(json!({ "name": "Customer", "tag_ids": [billing] })),
        )
        .await?;
    let heap = w.get(&heap_url).await?;
    let weights = w
        .app
        .patch(
            &w.session,
            &format!("{heap_url}/weights"),
            &heap.body["version"],
            json!({ "tags": 40, "tag_order": [x, billing] }),
        )
        .await?;
    anyhow::ensure!(weights.status == StatusCode::OK, "weights failed: {}", weights.body);

    // Act
    let refused = w.make_team_tag(json!({ "name": "billing" }), None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let made = w.make_team_tag(json!({ "name": "billing" }), Some(&token)).await?;
    let team_tag = id(&made.body).to_owned();
    let views = w.get(&format!("{heap_url}/views")).await?;
    let filter = views.body["views"][0]["view"]["tag"].clone();
    let through_view = w.get(&format!("{heap_url}/items?tag={team_tag}")).await?;
    let parameters = w.get(&format!("{heap_url}/parameters")).await?;
    let heap = w.get(&heap_url).await?;

    // Assert
    expect_that!(view.status, eq(StatusCode::CREATED));
    expect_that!(parameter.status, eq(StatusCode::CREATED));
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(filter, eq(&json!([team_tag])));
    let titles: Vec<&str> = through_view.body["ranked"]
        .as_array()
        .map(|rows| rows.iter().filter_map(|r| r["item"]["title"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(titles, elements_are![eq(&"Invoices")]);
    expect_that!(parameters.body["parameters"][0]["tag_ids"], eq(&json!([team_tag])));
    expect_that!(heap.body["tag_order"], eq(&json!([x, team_tag])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_reach_that_changed_is_refused_again_with_a_new_token() -> Result<()> {
    // Arrange
    let w = world().await?;
    for heap in [&w.a, &w.b] {
        let tag = w.heap_tag(heap, "billing").await?;
        w.item(heap, "Invoice", &[&tag]).await?;
    }
    let first = w.make_team_tag(json!({ "name": "billing" }), None).await?;
    let t1 = first.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    // Act
    let in_c = w.heap_tag(&w.c, "Billing").await?;
    w.item(&w.c, "Invoice", &[&in_c]).await?;
    let stale = w.make_team_tag(json!({ "name": "billing" }), Some(&t1)).await?;
    let t2 = stale.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let made = w.make_team_tag(json!({ "name": "billing" }), Some(&t2)).await?;
    let mut carried = 0;
    for heap in [&w.a, &w.b, &w.c] {
        let list = w.get(&format!("{}/items", w.app.heap_url(heap).await?)).await?;
        carried += list.body["ranked"]
            .as_array()
            .map(|rows| {
                rows.iter()
                    .filter(|r| r["item"]["tags"][0]["id"] == made.body["id"])
                    .count()
            })
            .unwrap_or_default();
    }

    // Assert
    expect_that!(first.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("tag_merge_unconfirmed")));
    expect_that!(stale.body["merge_reach"]["heaps"].as_array().map(Vec::len), some(eq(3)));
    expect_that!(t2, not(eq(&t1)));
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(carried, eq(3));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_first_of_two_writes_for_one_name_wins() -> Result<()> {
    // Arrange: the team tag comes first.
    let w = world().await?;
    let docs = w.make_team_tag(json!({ "name": "docs" }), None).await?;

    // Act
    let heap_tag = w.app.create_tag(&w.session, &w.b, "docs").await?;

    // Assert
    expect_that!(docs.status, eq(StatusCode::CREATED));
    expect_that!(heap_tag.status, eq(StatusCode::CONFLICT));
    expect_that!(heap_tag.body["code"], eq(&json!("tag_name_taken_by_team_tag")));

    // Arrange: a fresh team, the heap tag comes first.
    let w = world().await?;
    let tag = w.heap_tag(&w.b, "docs").await?;
    let item = w.item(&w.b, "Readme", &[&tag]).await?;

    // Act
    let refused = w.make_team_tag(json!({ "name": "docs" }), None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();
    let made = w.make_team_tag(json!({ "name": "docs" }), Some(&token)).await?;
    let item = w.read_item(&item).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["merge_reach"]["heaps"][0]["heap"]["id"], eq(&json!(w.b)));
    expect_that!(made.status, eq(StatusCode::CREATED));
    expect_that!(tag_ids(&item), elements_are![eq(&id(&made.body))]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_edit_and_a_merge_in_the_same_heap_both_finish() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.heap_tag(&w.a, "billing").await?;
    let item = w.item(&w.a, "Invoice", &[&tag]).await?;
    let item_url = w.app.item_url(id(&item)).await?;
    let refused = w.make_team_tag(json!({ "name": "billing" }), None).await?;
    let token = refused.body["merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned();

    // Act
    let (merged, edited) = tokio::join!(
        w.make_team_tag(json!({ "name": "billing" }), Some(&token)),
        w.app
            .patch(&w.session, &item_url, &item["version"], json!({ "title": "Invoices" })),
    );
    let (merged, edited) = (merged?, edited?);

    // Assert
    for status in [merged.status, edited.status] {
        expect_that!(status.is_success() || status == StatusCode::CONFLICT, eq(true));
    }
    Ok(())
}
