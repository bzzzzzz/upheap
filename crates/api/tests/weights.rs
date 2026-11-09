//! The heap's ranking weights: saving them with the parameters' weights in one transaction, their audit entries, and
//! the ranking preview.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// An admin's session, a member's session, and the shared team and heap.
struct Setup {
    app: TestApp,
    admin: String,
    member: String,
    team: String,
    heap: String,
}

async fn setup() -> Result<Setup> {
    let app = TestApp::start().await?;
    let (admin, _) = app.register("ada@example.test").await?;
    let (member, _) = app.register("bob@example.test").await?;
    let (team, heap) = app.shared_heap(&admin).await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let member_id = me.body["user"]["id"].as_str().context("no user")?.to_owned();
    app.add_member(&team, &member_id, Role::Member).await?;
    Ok(Setup {
        app,
        admin,
        member,
        team,
        heap,
    })
}

impl Setup {
    async fn heap_path(&self) -> Result<String> {
        self.app.heap_url(&self.heap).await
    }

    async fn get_heap(&self, session: &str) -> Result<TestResponse> {
        self.app
            .send(Method::GET, &self.heap_path().await?, Some(session), None)
            .await
    }

    async fn heap_version(&self) -> Result<Value> {
        Ok(self.get_heap(&self.admin).await?.body["version"].clone())
    }

    /// `PATCH …/weights` as the admin at the heap's current version.
    async fn save(&self, body: Value) -> Result<TestResponse> {
        let version = self.heap_version().await?;
        self.save_at(&self.admin, &version, body).await
    }

    async fn save_at(&self, session: &str, version: &Value, body: Value) -> Result<TestResponse> {
        let path = format!("{}/weights", self.heap_path().await?);
        self.app.patch(session, &path, version, body).await
    }

    async fn preview(&self, session: &str, body: Value) -> Result<TestResponse> {
        let path = format!("{}/ranking/preview", self.heap_path().await?);
        self.app.send(Method::POST, &path, Some(session), Some(body)).await
    }

    async fn new_parameter(&self, name: &str, options: &[&str]) -> Result<Value> {
        let created = self
            .app
            .send(
                Method::POST,
                &format!("{}/parameters", self.heap_path().await?),
                Some(&self.admin),
                Some(json!({ "name": name, "options": options })),
            )
            .await?;
        anyhow::ensure!(created.status == StatusCode::CREATED, "parameter: {}", created.body);
        Ok(created.body)
    }

    async fn get_parameter(&self, parameter: &Value) -> Result<Value> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/parameters", self.heap_path().await?),
                Some(&self.admin),
                None,
            )
            .await?;
        list.body["parameters"]
            .as_array()
            .and_then(|all| all.iter().find(|p| p["id"] == parameter["id"]))
            .cloned()
            .context("parameter not listed")
    }

    async fn ranked_titles(&self) -> Result<Vec<String>> {
        let list = self
            .app
            .send(
                Method::GET,
                &format!("{}/items", self.heap_path().await?),
                Some(&self.admin),
                None,
            )
            .await?;
        Ok(titles(&list.body["ranked"]))
    }

    /// The audit actions about `entity_id`, oldest first.
    async fn actions(&self, entity_id: &str) -> Result<Vec<String>> {
        Ok(self
            .app
            .audit_entries(entity_id)
            .await?
            .into_iter()
            .map(|(action, _)| action)
            .collect())
    }
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

fn actions(log: &Value) -> Vec<&str> {
    log["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default()
}

fn preview_titles(preview: &Value) -> Vec<String> {
    preview["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|entry| entry["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

#[gtest]
#[tokio::test]
async fn a_heap_shows_its_weights_and_only_admins_may_edit_them() -> Result<()> {
    // Arrange
    let s = setup().await?;

    // Act
    let admin = s.get_heap(&s.admin).await?;
    let member = s.get_heap(&s.member).await?;

    // Assert
    let tens = json!({ "need": 10, "age": 10, "deadline": 10, "value": 10 });
    expect_that!(admin.body["weights"], eq(&tens));
    expect_that!(
        admin.body["allowed_actions"],
        eq(&json!(["rename", "archive", "edit_weights", "view_history"]))
    );
    expect_that!(member.body["weights"], eq(&tens));
    expect_that!(member.body["allowed_actions"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn one_save_sets_the_heap_and_parameter_weights_and_moves_both_versions() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    s.new_parameter("Area", &["Infra", "Web"]).await?;
    let before = s.get_heap(&s.admin).await?.body;

    // Act
    let saved = s
        .save(json!({ "need": 30, "parameters": [{ "id": customer["id"], "weight": 40, "version": customer["version"] }] }))
        .await?;
    let fetched = s.get_heap(&s.admin).await?;
    let parameter = s.get_parameter(&customer).await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(
        saved.body["heap"]["weights"],
        eq(&json!({ "need": 30, "age": 10, "deadline": 10, "value": 10 }))
    );
    expect_that!(
        saved.body["heap"]["version"],
        eq(&json!(before["version"].as_u64().unwrap_or_default() + 1))
    );
    expect_that!(saved.body["heap"]["allowed_actions"], eq(&before["allowed_actions"]));
    let listed = saved.body["parameters"].as_array().context("parameters")?;
    let names: Vec<&str> = listed.iter().filter_map(|p| p["name"].as_str()).collect();
    expect_that!(names, elements_are![eq(&"Area"), eq(&"Customer")]);
    expect_that!(listed[1]["weight"], eq(&json!(40)));
    expect_that!(
        listed[1]["version"],
        eq(&json!(customer["version"].as_u64().unwrap_or_default() + 1))
    );
    expect_that!(listed[0]["weight"], eq(&json!(10)));
    expect_that!(fetched.body, eq(&saved.body["heap"]));
    expect_that!(&parameter, eq(&listed[1]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn saving_weights_reranks_the_list() -> Result<()> {
    // Arrange
    let s = setup().await?;
    s.app
        .new_item(&s.admin, &s.heap, json!({ "title": "Urgent", "need": "critical" }))
        .await?;
    s.app
        .new_item(
            &s.admin,
            &s.heap,
            json!({ "title": "Quick win", "need": "expected", "effort": "s", "impact": "l" }),
        )
        .await?;
    let before = s.ranked_titles().await?;

    // Act
    s.save(json!({ "value": 100 })).await?;
    let after = s.ranked_titles().await?;

    // Assert
    expect_that!(before, elements_are![eq("Urgent"), eq("Quick win")]);
    expect_that!(after, elements_are![eq("Quick win"), eq("Urgent")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn saving_only_parameter_weights_moves_the_heap_version_without_a_heap_entry() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let before = s.heap_version().await?;

    // Act
    let saved = s
        .save(json!({ "parameters": [{ "id": customer["id"], "weight": 0, "version": customer["version"] }] }))
        .await?;

    // Assert
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(
        saved.body["heap"]["version"],
        eq(&json!(before.as_u64().unwrap_or_default() + 1))
    );
    expect_that!(saved.body["parameters"][0]["weight"], eq(&json!(0)));
    let heap = s.heap.clone();
    expect_that!(s.actions(&heap).await?, elements_are![eq("heap_created")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_save_that_changes_nothing_writes_nothing() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let before = s.get_heap(&s.admin).await?.body;

    // Act
    let empty = s.save(json!({})).await?;
    let same = s
        .save(json!({
            "need": 10, "age": 10, "deadline": 10, "value": 10,
            "parameters": [{ "id": customer["id"], "weight": 10, "version": customer["version"] }],
        }))
        .await?;

    // Assert
    for saved in [&empty, &same] {
        expect_that!(saved.status, eq(StatusCode::OK));
        expect_that!(saved.body["heap"], eq(&before));
        expect_that!(saved.body["parameters"][0]["version"], eq(&customer["version"]));
    }
    let (heap, parameter) = (s.heap.clone(), customer["id"].as_str().context("id")?.to_owned());
    expect_that!(s.actions(&heap).await?, elements_are![eq("heap_created")]);
    expect_that!(s.actions(&parameter).await?, elements_are![eq("parameter_created")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn weights_outside_0_to_100_are_refused_and_nothing_changes() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let before = s.get_heap(&s.admin).await?.body;

    // Act
    let low = s.save(json!({ "need": -1 })).await?;
    let high = s.save(json!({ "age": 101 })).await?;
    let parameter = s
        .save(json!({ "need": 50, "parameters": [{ "id": customer["id"], "weight": 101, "version": customer["version"] }] }))
        .await?;

    // Assert
    for refused in [&low, &high, &parameter] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["code"], eq(&json!("weight_out_of_range")));
    }
    expect_that!(s.get_heap(&s.admin).await?.body, eq(&before));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_unknown_field_is_refused() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;

    // Act
    let typo = s.save(json!({ "effort": 20 })).await?;
    let in_parameter = s
        .save(json!({ "parameters": [{ "id": customer["id"], "weight": 5, "version": customer["version"], "name": "X" }] }))
        .await?;
    let preview = s.preview(&s.admin, json!({ "effort": 20 })).await?;

    // Assert
    for refused in [&typo, &in_parameter, &preview] {
        expect_that!(refused.body["code"], eq(&json!("invalid_body")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_bad_parameter_saves_nothing_not_even_the_heap_weights() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let retired = s.new_parameter("Retired", &["A", "B"]).await?;
    let archived = s
        .app
        .send(
            Method::POST,
            &format!(
                "{}/parameters/{}/archive",
                s.heap_path().await?,
                retired["id"].as_str().context("id")?
            ),
            Some(&s.admin),
            None,
        )
        .await?;
    anyhow::ensure!(archived.status == StatusCode::OK, "archive: {}", archived.body);
    let (_, other_heap) = s.app.shared_heap(&s.admin).await?;
    let foreign = {
        let created = s
            .app
            .send(
                Method::POST,
                &format!("{}/parameters", s.app.heap_url(&other_heap).await?),
                Some(&s.admin),
                Some(json!({ "name": "Foreign" })),
            )
            .await?;
        created.body
    };
    let before = s.get_heap(&s.admin).await?.body;
    let version = &customer["version"];
    let entry = |p: &Value, version: &Value| json!({ "id": p["id"], "weight": 70, "version": version });

    // Act
    let not_in_heap = s
        .save(json!({ "need": 60, "parameters": [entry(&customer, version), entry(&foreign, &foreign["version"])] }))
        .await?;
    let archived_one = s
        .save(json!({ "need": 60, "parameters": [entry(&customer, version), entry(&retired, &retired["version"])] }))
        .await?;
    let twice = s
        .save(json!({ "need": 60, "parameters": [entry(&customer, version), entry(&customer, version)] }))
        .await?;
    let stale = s
        .save(json!({ "need": 60, "parameters": [entry(&customer, &json!(99))] }))
        .await?;

    // Assert
    expect_that!(not_in_heap.body["code"], eq(&json!("parameter_not_in_heap")));
    expect_that!(archived_one.status, eq(StatusCode::CONFLICT));
    expect_that!(archived_one.body["code"], eq(&json!("parameter_archived")));
    expect_that!(twice.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(twice.body["code"], eq(&json!("duplicate_parameter")));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(s.get_heap(&s.admin).await?.body, eq(&before));
    expect_that!(s.get_parameter(&customer).await?, eq(&customer));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_heap_version_conflicts_even_when_only_parameters_change() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let stale = s.heap_version().await?;
    s.save(json!({ "need": 20 })).await?;
    let missing = s
        .app
        .send(
            Method::PATCH,
            &format!("{}/weights", s.heap_path().await?),
            Some(&s.admin),
            Some(json!({})),
        )
        .await?;

    // Act
    let conflict = s
        .save_at(
            &s.admin,
            &stale,
            json!({ "parameters": [{ "id": customer["id"], "weight": 0, "version": customer["version"] }] }),
        )
        .await?;

    // Assert
    expect_that!(conflict.status, eq(StatusCode::CONFLICT));
    expect_that!(conflict.body["code"], eq(&json!("version_conflict")));
    expect_that!(s.get_parameter(&customer).await?["weight"], eq(&json!(10)));
    expect_that!(missing.status, eq(StatusCode::PRECONDITION_REQUIRED));
    expect_that!(missing.body["code"], eq(&json!("version_required")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_save_against_a_policy_changed_only_in_its_parameters_conflicts() -> Result<()> {
    // Arrange: two admins open the same policy; the first saves one parameter's weight.
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let risk = s.new_parameter("Risk", &["High", "Low"]).await?;
    let seen = s.heap_version().await?;
    let first = s
        .save_at(
            &s.admin,
            &seen,
            json!({ "parameters": [{ "id": customer["id"], "weight": 40, "version": customer["version"] }] }),
        )
        .await?;

    // Act: the second saves another parameter against the policy as it was.
    let second = s
        .save_at(
            &s.admin,
            &seen,
            json!({ "parameters": [{ "id": risk["id"], "weight": 0, "version": risk["version"] }] }),
        )
        .await?;

    // Assert
    expect_that!(first.status, eq(StatusCode::OK));
    expect_that!(second.status, eq(StatusCode::CONFLICT));
    expect_that!(second.body["code"], eq(&json!("version_conflict")));
    expect_that!(s.get_parameter(&risk).await?["weight"], eq(&json!(10)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_may_not_save_weights_and_an_archived_heap_refuses_them() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let version = s.heap_version().await?;

    // Act
    let member = s.save_at(&s.member, &version, json!({ "need": 20 })).await?;
    s.app.set_heap_archived(&s.heap, true).await?;
    let archived = s.save_at(&s.admin, &version, json!({ "need": 20 })).await?;

    // Assert
    expect_that!(member.status, eq(StatusCode::FORBIDDEN));
    expect_that!(archived.status, eq(StatusCode::CONFLICT));
    expect_that!(archived.body["code"], eq(&json!("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_history_and_activity_name_the_weights_that_changed() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;

    // Act
    s.save(
        json!({ "need": 30, "parameters": [{ "id": customer["id"], "weight": 40, "version": customer["version"] }] }),
    )
    .await?;
    let heap_history = s
        .app
        .send(
            Method::GET,
            &format!("{}/audit?entity_id={}", s.heap_path().await?, s.heap),
            Some(&s.admin),
            None,
        )
        .await?;
    let parameter_history = s
        .app
        .send(
            Method::GET,
            &format!(
                "{}/audit?entity_id={}",
                s.heap_path().await?,
                customer["id"].as_str().context("id")?
            ),
            Some(&s.admin),
            None,
        )
        .await?;
    let heap_activity = s
        .app
        .send(
            Method::GET,
            &format!("{}/audit", s.heap_path().await?),
            Some(&s.admin),
            None,
        )
        .await?;
    let team_activity = s
        .app
        .send(
            Method::GET,
            &format!("{}/audit", s.app.team_url(&s.team).await?),
            Some(&s.admin),
            None,
        )
        .await?;

    // Assert
    let latest = &heap_history.body["entries"][0];
    expect_that!(latest["action"], eq(&json!("heap_updated")));
    expect_that!(
        latest["changes"],
        eq(&json!([{ "field": "need_weight", "label": "Need weight", "before": "10", "after": "30" }]))
    );
    let latest = &parameter_history.body["entries"][0];
    expect_that!(latest["action"], eq(&json!("parameter_updated")));
    expect_that!(
        latest["changes"],
        eq(&json!([{ "field": "weight", "label": "Weight", "before": "10", "after": "40" }]))
    );
    // The heap's activity holds the parameter's entry too; the team's holds the heap's own.
    let in_heap: Vec<&str> = actions(&heap_activity.body);
    expect_that!(in_heap, superset_of([&"heap_updated", &"parameter_updated"]));
    expect_that!(actions(&team_activity.body), contains(eq(&"heap_updated")));
    Ok(())
}

/// Items for the preview tests, as `(title, body)`: in default-weight order "Urgent" comes first, then "Soon".
async fn preview_items(s: &Setup) -> Result<()> {
    for (title, extra) in [
        ("Urgent", json!({ "need": "critical" })),
        ("Soon", json!({ "due": { "date": "2027-01-03", "firm": true } })),
        ("Later", json!({ "due": { "date": "2027-01-20", "firm": true } })),
        ("Plain", json!({})),
    ] {
        let mut body = json!({ "title": title });
        if let (Some(body), Some(extra)) = (body.as_object_mut(), extra.as_object()) {
            body.extend(extra.clone());
        }
        s.app.new_item(&s.admin, &s.heap, body).await?;
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_preview_without_a_draft_is_the_current_top_of_the_list() -> Result<()> {
    // Arrange
    let s = setup().await?;
    preview_items(&s).await?;
    let version = s.heap_version().await?;
    let list = s.ranked_titles().await?;

    // Act
    let preview = s.preview(&s.member, json!({})).await?;

    // Assert
    expect_that!(preview.status, eq(StatusCode::OK));
    expect_that!(preview_titles(&preview.body), eq(&list));
    for (index, entry) in preview.body["items"].as_array().context("items")?.iter().enumerate() {
        expect_that!(entry["position"], eq(&json!(index + 1)));
        expect_that!(entry["current_position"], eq(&entry["position"]));
    }
    expect_that!(s.heap_version().await?, eq(&version));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_draft_ranks_by_the_new_weights_and_saves_nothing() -> Result<()> {
    // Arrange
    let s = setup().await?;
    preview_items(&s).await?;
    let version = s.heap_version().await?;
    let list = s.ranked_titles().await?;
    let soon_now = list.iter().position(|t| t == "Soon").context("Soon")? + 1;

    // Act
    let preview = s
        .preview(
            &s.admin,
            json!({ "need": 0, "age": 0, "value": 0, "deadline": 10, "limit": 2 }),
        )
        .await?;

    // Assert
    expect_that!(preview.status, eq(StatusCode::OK));
    expect_that!(preview_titles(&preview.body), elements_are![eq("Soon"), eq("Later")]);
    expect_that!(preview.body["items"][0]["position"], eq(&json!(1)));
    expect_that!(preview.body["items"][0]["current_position"], eq(&json!(soon_now)));
    expect_that!(preview.body["items"][0]["item"]["key"].is_string(), eq(true));
    expect_that!(s.heap_version().await?, eq(&version));
    expect_that!(s.ranked_titles().await?, eq(&list));
    let heap = s.heap.clone();
    expect_that!(s.actions(&heap).await?, elements_are![eq("heap_created")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_preview_takes_parameter_weights_and_checks_the_limit() -> Result<()> {
    // Arrange
    let s = setup().await?;
    let customer = s.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    s.app.new_item(&s.admin, &s.heap, json!({ "title": "Plain" })).await?;
    let draft = json!([{ "id": customer["id"], "weight": 5 }]);

    // Act
    let ok = s.preview(&s.admin, json!({ "parameters": draft })).await?;
    let twice = s
        .preview(&s.admin, json!({ "parameters": [draft[0], draft[0]] }))
        .await?;
    let unknown = s
        .preview(
            &s.admin,
            json!({ "parameters": [{ "id": "00000000-0000-0000-0000-000000000001", "weight": 5 }] }),
        )
        .await?;
    let too_heavy = s.preview(&s.admin, json!({ "need": 101 })).await?;
    let zero = s.preview(&s.admin, json!({ "limit": 0 })).await?;
    let many = s.preview(&s.admin, json!({ "limit": 21 })).await?;
    s.app.set_heap_archived(&s.heap, true).await?;
    let archived = s.preview(&s.member, json!({})).await?;

    // Assert
    expect_that!(ok.status, eq(StatusCode::OK));
    expect_that!(twice.body["code"], eq(&json!("duplicate_parameter")));
    expect_that!(unknown.body["code"], eq(&json!("parameter_not_in_heap")));
    expect_that!(too_heavy.body["code"], eq(&json!("weight_out_of_range")));
    for refused in [&zero, &many] {
        expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
        expect_that!(refused.body["code"], eq(&json!("limit_out_of_range")));
    }
    expect_that!(archived.status, eq(StatusCode::OK));
    Ok(())
}
