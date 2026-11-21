//! Tag links on team parameters: the team links a parameter to its tags, which suggests it in every heap that uses
//! it, and a heap links its own tags to a team parameter it uses, which suggests it in that heap only.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// A team with two heaps, A and B, a team parameter both use, and the team's admin.
struct World {
    app: TestApp,
    session: String,
    team_path: String,
    a: String,
    b: String,
    parameter: Value,
}

async fn world() -> Result<World> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, a) = app.shared_heap(&session).await?;
    let team_path = app.team_url(&team).await?;
    let created = app
        .send(
            Method::POST,
            &format!("{team_path}/heaps"),
            Some(&session),
            Some(json!({ "name": "Second" })),
        )
        .await?;
    let b = created.body["id"].as_str().context("heap has no id")?.to_owned();
    let parameter = app
        .send(
            Method::POST,
            &format!("{team_path}/parameters"),
            Some(&session),
            Some(json!({ "name": "Customer", "options": ["Enterprise", "Free"] })),
        )
        .await?
        .body;
    let w = World {
        app,
        session,
        team_path,
        a,
        b,
        parameter,
    };
    for heap in [&w.a, &w.b] {
        let used = w
            .heap_post(heap, &format!("parameters/{}/use", id(&w.parameter)))
            .await?;
        anyhow::ensure!(used.status == StatusCode::OK, "use failed: {}", used.body);
    }
    Ok(w)
}

fn id(value: &Value) -> &str {
    value["id"].as_str().unwrap_or_default()
}

impl World {
    async fn heap_post(&self, heap: &str, tail: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/{tail}", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await
    }

    async fn team_tag(&self, name: &str) -> Result<String> {
        let made = self
            .app
            .send(
                Method::POST,
                &format!("{}/tags", self.team_path),
                Some(&self.session),
                Some(json!({ "name": name })),
            )
            .await?;
        anyhow::ensure!(made.status == StatusCode::CREATED, "team tag failed: {}", made.body);
        Ok(id(&made.body).to_owned())
    }

    /// Links team tags to the parameter at `version`.
    async fn link_team(&self, version: &Value, tags: &[&str]) -> Result<TestResponse> {
        self.app
            .send_with(
                Method::PUT,
                &format!("{}/parameters/{}/tags", self.team_path, id(&self.parameter)),
                Some(&self.session),
                Some(json!({ "tag_ids": tags })),
                &[common::CSRF, ("if-match", &version.to_string())],
            )
            .await
    }

    async fn link_heap(&self, heap: &str, tags: &[&str]) -> Result<TestResponse> {
        self.app
            .send(
                Method::PUT,
                &format!(
                    "{}/parameters/{}/heap-tags",
                    self.app.heap_url(heap).await?,
                    id(&self.parameter)
                ),
                Some(&self.session),
                Some(json!({ "tag_ids": tags })),
            )
            .await
    }

    async fn heap_view(&self, heap: &str) -> Result<Value> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/parameters", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await?;
        list.body["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == self.parameter["id"])
            .cloned()
            .context("parameter not used")
    }

    async fn team_view(&self) -> Result<Value> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/parameters", self.team_path),
                Some(&self.session),
                None,
            )
            .await?;
        list.body["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == self.parameter["id"])
            .cloned()
            .context("parameter not listed")
    }
}

#[gtest]
#[tokio::test]
async fn the_team_links_a_team_tag_and_every_heap_that_uses_the_parameter_suggests_it() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.team_tag("billing").await?;
    let before = w.team_view().await?;

    // Act
    let linked = w.link_team(&before["version"], &[&tag]).await?;
    let (in_a, in_b, team) = (w.heap_view(&w.a).await?, w.heap_view(&w.b).await?, w.team_view().await?);

    // Assert
    expect_that!(linked.status, eq(StatusCode::OK));
    expect_that!(linked.body["tag_ids"], eq(&json!([tag])));
    expect_that!(team["tag_ids"], eq(&json!([tag])));
    expect_that!(
        team["version"].as_u64(),
        some(eq(before["version"].as_u64().unwrap_or_default() + 1))
    );
    for view in [&in_a, &in_b] {
        expect_that!(view["tag_ids"], eq(&json!([tag])));
        expect_that!(view["team_tag_ids"], eq(&json!([tag])));
        expect_that!(view["heap_tag_ids"], eq(&json!([])));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn team_links_take_only_the_teams_tags_at_the_current_version() -> Result<()> {
    // Arrange
    let w = world().await?;
    let heap_tag = w.app.new_tag_id(&w.session, &w.a, "local").await?;
    let (other_session, _) = w.app.register("eve@example.test").await?;
    let (other_team, other_heap) = w.app.shared_heap(&other_session).await?;
    let other_tag = w.app.new_tag_id(&other_session, &other_heap, "theirs").await?;
    let other_team_path = w.app.team_url(&other_team).await?;
    let foreign_team_tag = w
        .app
        .send(
            Method::POST,
            &format!("{other_team_path}/tags"),
            Some(&other_session),
            Some(json!({ "name": "foreign" })),
        )
        .await?;
    let version = w.team_view().await?["version"].clone();

    // Act
    let heap_tag_refused = w.link_team(&version, &[&heap_tag]).await?;
    let other_heap_tag_refused = w.link_team(&version, &[&other_tag]).await?;
    let foreign_refused = w.link_team(&version, &[id(&foreign_team_tag.body)]).await?;
    let tag = w.team_tag("billing").await?;
    let linked = w.link_team(&version, &[&tag]).await?;
    let stale = w.link_team(&version, &[]).await?;

    // Assert
    for refused in [&heap_tag_refused, &other_heap_tag_refused, &foreign_refused] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["code"], eq(&json!("tag_not_in_heap")));
    }
    expect_that!(linked.status, eq(StatusCode::OK));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_links_its_own_tag_and_only_that_heap_suggests_the_parameter() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.app.new_tag_id(&w.session, &w.a, "billing").await?;
    let before = w.team_view().await?;

    // Act
    let linked = w.link_heap(&w.a, &[&tag]).await?;
    let (in_a, in_b, team) = (w.heap_view(&w.a).await?, w.heap_view(&w.b).await?, w.team_view().await?);

    // Assert
    expect_that!(linked.status, eq(StatusCode::OK));
    expect_that!(in_a["tag_ids"], eq(&json!([tag])));
    expect_that!(in_a["heap_tag_ids"], eq(&json!([tag])));
    expect_that!(in_a["team_tag_ids"], eq(&json!([])));
    expect_that!(in_b["tag_ids"], eq(&json!([])));
    expect_that!(in_b["heap_tag_ids"], eq(&json!([])));
    expect_that!(team["tag_ids"], eq(&json!([])));
    expect_that!(team["version"], eq(&before["version"]));
    expect_that!(linked.body["parameters"][0]["heap_tag_ids"], eq(&json!([tag])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_links_only_what_it_may_and_only_a_team_parameter_it_uses() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag_of_b = w.app.new_tag_id(&w.session, &w.b, "theirs").await?;
    let team_tag = w.team_tag("billing").await?;
    let own = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters", w.app.heap_url(&w.a).await?),
            Some(&w.session),
            Some(json!({ "name": "Area" })),
        )
        .await?
        .body;
    let unused = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters", w.team_path),
            Some(&w.session),
            Some(json!({ "name": "Unused" })),
        )
        .await?
        .body;
    let heap_url = w.app.heap_url(&w.a).await?;
    let put = |parameter: Value, tags: Vec<String>| {
        let path = format!("{heap_url}/parameters/{}/heap-tags", id(&parameter));
        let (app, session) = (&w.app, &w.session);
        async move {
            app.send(Method::PUT, &path, Some(session), Some(json!({ "tag_ids": tags })))
                .await
        }
    };

    // Act
    let foreign_tag = put(w.parameter.clone(), vec![tag_of_b]).await?;
    let heap_parameter = put(own, vec![]).await?;
    let not_used = put(unused, vec![]).await?;
    let first = put(w.parameter.clone(), vec![team_tag.clone()]).await?;
    let entries_after_first = w.app.audit_heaps(id(&w.parameter)).await?;
    let repeated = put(w.parameter.clone(), vec![team_tag.clone()]).await?;
    let entries_after_repeat = w.app.audit_heaps(id(&w.parameter)).await?;

    // Assert
    expect_that!(foreign_tag.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(foreign_tag.body["code"], eq(&json!("tag_not_in_heap")));
    expect_that!(heap_parameter.status, eq(StatusCode::NOT_FOUND));
    expect_that!(not_used.status, eq(StatusCode::NOT_FOUND));
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(repeated.status, eq(StatusCode::OK));
    expect_that!(entries_after_repeat.len(), eq(entries_after_first.len()));
    expect_that!(
        entries_after_first
            .last()
            .map(|(action, heap)| (action.as_str(), heap.is_some())),
        some(eq(("parameter_updated", true)))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_archived_in_the_heaps_view_refuses_heap_links() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tag = w.app.new_tag_id(&w.session, &w.a, "billing").await?;
    w.heap_post(&w.a, &format!("parameters/{}/stop-using", id(&w.parameter)))
        .await?;

    // Act
    let refused = w.link_heap(&w.a, &[&tag]).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("parameter_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_admin_may_link_a_team_parameter_in_a_heap_and_a_member_may_not() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (member_session, _) = w.app.register("bob@example.test").await?;
    let member_id = w
        .app
        .send(Method::GET, "/api/v1/me", Some(&member_session), None)
        .await?
        .body["user"]["id"]
        .as_str()
        .context("user id")?
        .to_owned();
    let team_id = w
        .app
        .send(Method::GET, &w.team_path, Some(&w.session), None)
        .await?
        .body["id"]
        .as_str()
        .context("team id")?
        .to_owned();
    w.app.add_member(&team_id, &member_id, Role::Member).await?;

    // Act
    let admin_view = w.heap_view(&w.a).await?;
    let member_list = w
        .app
        .send(
            Method::GET,
            &format!("{}/parameters", w.app.heap_url(&w.a).await?),
            Some(&member_session),
            None,
        )
        .await?;

    // Assert
    expect_that!(
        admin_view["allowed_actions"],
        eq(&json!(["link_tags", "stop_using", "view_history"]))
    );
    expect_that!(member_list.body["parameters"][0]["allowed_actions"], eq(&json!([])));
    Ok(())
}
