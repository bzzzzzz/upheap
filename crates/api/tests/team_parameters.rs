//! Team parameters: a parameter a team defines once, which each of its heaps chooses to use and weights on its own.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
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

fn id(body: &Value) -> String {
    body["id"].as_str().unwrap_or_default().to_owned()
}

fn option_id(parameter: &Value, label: &str) -> String {
    parameter["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["label"] == label))
        .and_then(|o| o["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn titles(response: &TestResponse) -> Vec<&str> {
    response.body["ranked"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["item"]["title"].as_str()).collect())
        .unwrap_or_default()
}

fn names<'a>(list: &'a Value, field: &str) -> Vec<&'a str> {
    list[field]
        .as_array()
        .map(|all| all.iter().filter_map(|p| p["name"].as_str()).collect())
        .unwrap_or_default()
}

impl World {
    async fn make_parameter(&self, name: &str, options: &[&str]) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/parameters", self.team_path),
                Some(&self.session),
                Some(json!({ "name": name, "options": options })),
            )
            .await
    }

    /// A team parameter that must succeed.
    async fn new_parameter(&self, name: &str, options: &[&str]) -> Result<Value> {
        let made = self.make_parameter(name, options).await?;
        anyhow::ensure!(
            made.status == StatusCode::CREATED,
            "make team parameter failed: {}",
            made.body
        );
        Ok(made.body)
    }

    async fn team_parameter(&self, parameter: &Value) -> Result<Value> {
        let list = self.team_parameters().await?;
        list.body["parameters"]
            .as_array()
            .into_iter()
            .chain(list.body["archived"].as_array())
            .flatten()
            .find(|p| p["id"] == parameter["id"])
            .cloned()
            .context("team parameter not listed")
    }

    async fn team_parameters(&self) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("{}/parameters", self.team_path),
                Some(&self.session),
                None,
            )
            .await
    }

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

    async fn use_in(&self, heap: &str, parameter: &Value) -> Result<TestResponse> {
        self.heap_post(heap, &format!("parameters/{}/use", id(parameter))).await
    }

    async fn stop_in(&self, heap: &str, parameter: &Value) -> Result<TestResponse> {
        self.heap_post(heap, &format!("parameters/{}/stop-using", id(parameter)))
            .await
    }

    async fn heap_parameters(&self, heap: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("{}/parameters", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await
    }

    /// The heap's weights save for one parameter, at the heap's and the parameter's current versions.
    async fn set_weight(&self, heap: &str, parameter: &Value, weight: u32) -> Result<TestResponse> {
        let path = self.app.heap_url(heap).await?;
        let version = self.app.send(Method::GET, &path, Some(&self.session), None).await?.body["version"].clone();
        let listed = self.heap_parameters(heap).await?;
        let current = listed.body["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|p| p["id"] == parameter["id"])
            .cloned()
            .context("parameter not used")?;
        self.app
            .patch(
                &self.session,
                &format!("{path}/weights"),
                &version,
                json!({ "parameters": [{ "id": id(parameter), "weight": weight, "version": current["version"] }] }),
            )
            .await
    }

    async fn items(&self, heap: &str, query: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("{}/items?{query}", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await
    }

    async fn item_with(&self, heap: &str, title: &str, parameter: &Value, label: &str) -> Result<Value> {
        self.app
            .new_item(
                &self.session,
                heap,
                json!({ "need": "idea", "title": title, "parameter_values": { id(parameter): option_id(parameter, label) } }),
            )
            .await
    }

    async fn close(&self, item: &Value) -> Result<TestResponse> {
        self.app
            .patch(
                &self.session,
                &self.app.item_url(id(item)).await?,
                &item["version"],
                json!({ "state": "wont_do" }),
            )
            .await
    }
}

#[gtest]
#[tokio::test]
async fn a_team_parameter_is_made_once_and_each_heap_weights_it_on_its_own() -> Result<()> {
    // Arrange: heap one weights it 100, heap two 0, and both hold an older "Free" item and a newer "Enterprise" one.
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    w.use_in(&w.first, &customer).await?;
    w.use_in(&w.second, &customer).await?;
    w.set_weight(&w.first, &customer, 100).await?;
    w.set_weight(&w.second, &customer, 0).await?;
    for heap in [&w.first, &w.second] {
        w.item_with(heap, "Older, free", &customer, "Free").await?;
    }
    w.app.clock.advance(domain::Duration::from_days(10));
    for heap in [&w.first, &w.second] {
        w.item_with(heap, "Enterprise", &customer, "Enterprise").await?;
    }

    // Act
    let one = w.items(&w.first, "").await?;
    let two = w.items(&w.second, "").await?;

    // Assert
    expect_that!(titles(&one), elements_are![eq(&"Enterprise"), eq(&"Older, free")]);
    expect_that!(
        one.body["ranked"][0]["contributors"][0],
        eq(&json!({ "factor": "parameter", "label": "Customer: Enterprise", "share_percent": 100 }))
    );
    expect_that!(titles(&two), elements_are![eq(&"Older, free"), eq(&"Enterprise")]);
    for entry in two.body["ranked"].as_array().context("ranked")? {
        let factors: Vec<&Value> = entry["contributors"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|c| &c["factor"])
            .collect();
        expect_that!(factors, not(contains(eq(&&json!("parameter")))));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_team_lists_its_parameters_and_a_heap_lists_the_ones_it_uses_among_its_own() -> Result<()> {
    // Arrange
    let w = world().await?;
    let zeta = w.new_parameter("Zeta", &["A", "B"]).await?;
    w.new_parameter("alpha", &["A"]).await?;
    w.app
        .send(
            Method::POST,
            &format!("{}/parameters", w.app.heap_url(&w.first).await?),
            Some(&w.session),
            Some(json!({ "name": "Mid", "options": ["X"] })),
        )
        .await?;

    // Act
    let before = w.heap_parameters(&w.first).await?;
    let used = w.use_in(&w.first, &zeta).await?;
    let again = w.use_in(&w.first, &zeta).await?;
    let team = w.team_parameters().await?;

    // Assert
    expect_that!(zeta["heaps"], eq(&json!([])));
    expect_that!(zeta["allowed_actions"], eq(&json!(["edit", "archive", "view_history"])));
    expect_that!(names(&before.body, "parameters"), elements_are![eq(&"Mid")]);
    expect_that!(
        names(&before.body, "team_available"),
        elements_are![eq(&"alpha"), eq(&"Zeta")]
    );
    expect_that!(
        before.body["allowed_actions"],
        eq(&json!(["create", "use_team_parameter"]))
    );
    expect_that!(used.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body, eq(&used.body));
    expect_that!(names(&used.body, "parameters"), elements_are![eq(&"Mid"), eq(&"Zeta")]);
    expect_that!(names(&used.body, "team_available"), elements_are![eq(&"alpha")]);
    let listed = &used.body["parameters"][1];
    expect_that!(listed["kind"], eq(&json!("team")));
    expect_that!(listed["heap_id"], eq(&json!(w.first)));
    expect_that!(listed["weight"], eq(&json!(10)));
    expect_that!(listed["allowed_actions"], eq(&json!(["stop_using", "view_history"])));
    expect_that!(used.body["parameters"][0]["kind"], eq(&json!("heap")));
    expect_that!(
        names(&team.body, "parameters"),
        elements_are![eq(&"alpha"), eq(&"Zeta")]
    );
    expect_that!(team.body["parameters"][1]["heaps"][0]["id"], eq(&json!(w.first)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn names_clash_between_a_heap_and_its_team_ignoring_case_unless_archived() -> Result<()> {
    // Arrange
    let w = world().await?;
    let heap_parameters = format!("{}/parameters", w.app.heap_url(&w.second).await?);
    let in_second = |name: &'static str| {
        w.app.send(
            Method::POST,
            &heap_parameters,
            Some(&w.session),
            Some(json!({ "name": name })),
        )
    };
    let team_made = w.new_parameter("Customer", &["A"]).await?;
    let heap_made = in_second("Area").await?;

    // Act
    let heap_over_team = in_second("CUSTOMER").await?;
    let team_over_heap = w.make_parameter("area", &["A"]).await?;
    let team_over_team = w.make_parameter("customer", &["A"]).await?;
    let rename_heap = w
        .app
        .patch(
            &w.session,
            &format!("{heap_parameters}/{}", id(&heap_made.body)),
            &heap_made.body["version"],
            json!({ "name": "customer" }),
        )
        .await?;
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{heap_parameters}/{}/archive", id(&heap_made.body)),
            Some(&w.session),
            None,
        )
        .await?;
    let team_over_archived = w.make_parameter("AREA", &["A"]).await?;
    let archived_team = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/archive", w.team_path, id(&team_made)),
            Some(&w.session),
            None,
        )
        .await?;
    let heap_over_archived = in_second("customer").await?;

    // Assert
    for refused in [&heap_over_team, &team_over_heap, &team_over_team, &rename_heap] {
        expect_that!(refused.status, eq(StatusCode::CONFLICT));
        expect_that!(refused.body["code"], eq(&json!("parameter_name_taken")));
    }
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(team_over_archived.status, eq(StatusCode::CREATED));
    expect_that!(archived_team.status, eq(StatusCode::OK));
    expect_that!(heap_over_archived.status, eq(StatusCode::CREATED));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn values_are_accepted_only_while_the_heap_uses_the_parameter() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    let value = json!({ id(&customer): option_id(&customer, "Free") });

    // Act
    let refused = w
        .app
        .create_item(
            &w.session,
            &w.first,
            json!({ "title": "One", "parameter_values": value }),
        )
        .await?;
    let listed = w.heap_parameters(&w.first).await?;
    w.use_in(&w.first, &customer).await?;
    let one = w.item_with(&w.first, "One", &customer, "Free").await?;
    w.item_with(&w.first, "Two", &customer, "Enterprise").await?;
    let filtered = w
        .items(&w.first, &format!("param={}", option_id(&customer, "Free")))
        .await?;
    let sorted = w
        .items(&w.first, &format!("sort=parameter&sort_parameter={}", id(&customer)))
        .await?;
    let second = w
        .app
        .create_item(
            &w.session,
            &w.second,
            json!({ "title": "Elsewhere", "parameter_values": value }),
        )
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["code"], eq(&json!("parameter_not_in_heap")));
    expect_that!(names(&listed.body, "parameters"), is_empty());
    expect_that!(names(&listed.body, "team_available"), elements_are![eq(&"Customer")]);
    expect_that!(one["parameters"][0]["name"], eq(&json!("Customer")));
    expect_that!(titles(&filtered), elements_are![eq(&"One")]);
    expect_that!(titles(&sorted), elements_are![eq(&"Two"), eq(&"One")]);
    expect_that!(second.body["code"], eq(&json!("parameter_not_in_heap")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn stopping_is_refused_while_an_open_item_has_a_value_and_using_again_restores_the_weight() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    w.use_in(&w.first, &customer).await?;
    w.set_weight(&w.first, &customer, 40).await?;
    let item = w.item_with(&w.first, "One", &customer, "Free").await?;

    // Act
    let refused = w.stop_in(&w.first, &customer).await?;
    w.close(&item).await?;
    let stopped = w.stop_in(&w.first, &customer).await?;
    let stopped_again = w.stop_in(&w.first, &customer).await?;
    let closed = w
        .app
        .send(
            Method::GET,
            &format!("{}/items/closed", w.app.heap_url(&w.first).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let refused_value = w
        .app
        .create_item(
            &w.session,
            &w.first,
            json!({ "title": "Two", "parameter_values": { id(&customer): option_id(&customer, "Free") } }),
        )
        .await?;
    let reused = w.use_in(&w.first, &customer).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("parameter_in_use")));
    expect_that!(stopped.status, eq(StatusCode::OK));
    expect_that!(names(&stopped.body, "parameters"), is_empty());
    expect_that!(names(&stopped.body, "archived"), elements_are![eq(&"Customer")]);
    expect_that!(
        stopped.body["archived"][0]["allowed_actions"],
        eq(&json!(["view_history"]))
    );
    expect_that!(names(&stopped.body, "team_available"), elements_are![eq(&"Customer")]);
    expect_that!(stopped_again.body, eq(&stopped.body));
    expect_that!(
        closed.body["items"][0]["parameters"][0]["parameter_archived"],
        eq(&json!(true))
    );
    expect_that!(
        closed.body["items"][0]["parameters"][0]["option"]["label"],
        eq(&json!("Free"))
    );
    expect_that!(refused_value.body["code"], eq(&json!("parameter_not_in_heap")));
    expect_that!(reused.body["parameters"][0]["weight"], eq(&json!(40)));
    expect_that!(names(&reused.body, "archived"), is_empty());
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archiving_a_team_option_switches_open_items_in_every_heap_and_closed_ones_keep_it() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    w.use_in(&w.first, &customer).await?;
    w.use_in(&w.second, &customer).await?;
    let one = w.item_with(&w.first, "One", &customer, "Free").await?;
    let two = w.item_with(&w.second, "Two", &customer, "Free").await?;
    let closed = w.item_with(&w.first, "Closed", &customer, "Free").await?;
    w.close(&closed).await?;

    // Act
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{}/options/{}/archive", w.team_path, option_id(&customer, "Free")),
            Some(&w.session),
            Some(json!({ "replacement_option_id": option_id(&customer, "Enterprise") })),
        )
        .await?;
    let read = |item: &Value| {
        let (app, session, key) = (&w.app, &w.session, id(item));
        async move {
            let path = app.item_url(&key).await?;
            app.send(Method::GET, &path, Some(session), None).await
        }
    };
    let (one_after, two_after) = (read(&one).await?, read(&two).await?);
    let closed_page = w
        .app
        .send(
            Method::GET,
            &format!("{}/items/closed", w.app.heap_url(&w.first).await?),
            Some(&w.session),
            None,
        )
        .await?;
    let first_entries = w.app.audit_heaps(&id(&one)).await?;
    let second_entries = w.app.audit_heaps(&id(&two)).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["archived_options"][0]["label"], eq(&json!("Free")));
    for (after, before) in [(&one_after, &one), (&two_after, &two)] {
        expect_that!(after.body["parameters"][0]["option"]["label"], eq(&json!("Enterprise")));
        expect_that!(
            after.body["version"].as_u64(),
            some(eq(before["version"].as_u64().unwrap_or_default() + 1))
        );
    }
    expect_that!(
        closed_page.body["items"][0]["parameters"][0]["option"]["label"],
        eq(&json!("Free"))
    );
    expect_that!(
        first_entries.last(),
        some(eq(&("item_updated".to_owned(), Some(w.first.clone()))))
    );
    expect_that!(
        second_entries.last(),
        some(eq(&("item_updated".to_owned(), Some(w.second.clone()))))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_parameter_is_archived_only_once_no_open_item_in_any_heap_has_a_value() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise"]).await?;
    w.use_in(&w.first, &customer).await?;
    w.use_in(&w.second, &customer).await?;
    let in_second = w.item_with(&w.second, "Open", &customer, "Enterprise").await?;
    let archive_path = format!("{}/parameters/{}/archive", w.team_path, id(&customer));
    let archive = || w.app.send(Method::POST, &archive_path, Some(&w.session), None);

    // Act
    let refused = archive().await?;
    w.close(&in_second).await?;
    let archived = archive().await?;
    let listed = w.heap_parameters(&w.first).await?;
    let used_after = w.use_in(&w.first, &customer).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("parameter_in_use")));
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(archived.body["allowed_actions"], eq(&json!(["view_history"])));
    expect_that!(names(&listed.body, "parameters"), is_empty());
    expect_that!(names(&listed.body, "archived"), elements_are![eq(&"Customer")]);
    expect_that!(names(&listed.body, "team_available"), is_empty());
    expect_that!(used_after.body["code"], eq(&json!("parameter_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_heaps_paths_write_only_the_weight_of_a_team_parameter() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
    w.use_in(&w.first, &customer).await?;
    w.use_in(&w.second, &customer).await?;
    let heap_path = format!("{}/parameters/{}", w.app.heap_url(&w.first).await?, id(&customer));
    let version = &customer["version"];
    let heap_write = |method, tail: &str, body: Option<Value>| {
        let path = format!("{heap_path}{tail}");
        let (app, session) = (&w.app, &w.session);
        async move {
            let if_match = version.to_string();
            app.send_with(
                method,
                &path,
                Some(session),
                body,
                &[common::CSRF, ("if-match", &if_match)],
            )
            .await
        }
    };

    // Act
    let rename = heap_write(Method::PATCH, "", Some(json!({ "name": "Client" }))).await?;
    let weight = heap_write(Method::PATCH, "", Some(json!({ "weight": 50 }))).await?;
    let option = heap_write(Method::POST, "/options", Some(json!({ "label": "Pro" }))).await?;
    let archive = heap_write(Method::POST, "/archive", None).await?;
    let tags = heap_write(Method::PUT, "/tags", Some(json!({ "tag_ids": [] }))).await?;
    let option_archive = w
        .app
        .send(
            Method::POST,
            &format!(
                "{}/options/{}/archive",
                w.app.heap_url(&w.first).await?,
                option_id(&customer, "Free")
            ),
            Some(&w.session),
            Some(json!({ "replacement_option_id": option_id(&customer, "Enterprise") })),
        )
        .await?;
    let saved = w.set_weight(&w.first, &customer, 70).await?;
    let stale = w
        .app
        .patch(
            &w.session,
            &format!("{}/weights", w.app.heap_url(&w.second).await?),
            &w.app
                .send(Method::GET, &w.app.heap_url(&w.second).await?, Some(&w.session), None)
                .await?
                .body["version"],
            json!({ "parameters": [{ "id": id(&customer), "weight": 5, "version": 99 }] }),
        )
        .await?;
    let first = w.heap_parameters(&w.first).await?;
    let second = w.heap_parameters(&w.second).await?;
    let team = w.team_parameter(&customer).await?;

    // Assert
    for refused in [&rename, &weight, &option, &archive, &tags, &option_archive] {
        expect_that!(refused.status, eq(StatusCode::NOT_FOUND));
    }
    expect_that!(saved.status, eq(StatusCode::OK));
    expect_that!(saved.body["parameters"][0]["weight"], eq(&json!(70)));
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("version_conflict")));
    expect_that!(first.body["parameters"][0]["weight"], eq(&json!(70)));
    expect_that!(second.body["parameters"][0]["weight"], eq(&json!(10)));
    expect_that!(team["version"], eq(&customer["version"]));
    expect_that!(team["name"], eq(&json!("Customer")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_weights_save_refuses_a_team_parameter_the_heap_does_not_use() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise"]).await?;
    w.use_in(&w.first, &customer).await?;
    let path = w.app.heap_url(&w.second).await?;
    let version = w.app.send(Method::GET, &path, Some(&w.session), None).await?.body["version"].clone();

    // Act
    let refused = w
        .app
        .patch(
            &w.session,
            &format!("{path}/weights"),
            &version,
            json!({ "parameters": [{ "id": id(&customer), "weight": 5, "version": customer["version"] }] }),
        )
        .await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["code"], eq(&json!("parameter_not_in_heap")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn members_read_team_parameters_and_cannot_write_them_and_other_teams_are_not_found() -> Result<()> {
    // Arrange
    let w = world().await?;
    let customer = w.new_parameter("Customer", &["Enterprise", "Free"]).await?;
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
    let (other_session, _) = w.app.register("eve@example.test").await?;
    let (other_team, other_heap) = w.app.shared_heap(&other_session).await?;
    let other_team_path = w.app.team_url(&other_team).await?;
    let foreign = w
        .app
        .send(
            Method::POST,
            &format!("{other_team_path}/parameters"),
            Some(&other_session),
            Some(json!({ "name": "Customer", "options": ["A"] })),
        )
        .await?
        .body;
    let heap_path = w.app.heap_url(&w.first).await?;
    let as_member = |method, path: String, body: Option<Value>| {
        let (app, session) = (&w.app, &member_session);
        async move { app.send(method, &path, Some(session), body).await }
    };

    // Act
    let listed = as_member(Method::GET, format!("{}/parameters", w.team_path), None).await?;
    let created = as_member(
        Method::POST,
        format!("{}/parameters", w.team_path),
        Some(json!({ "name": "Area" })),
    )
    .await?;
    let used = as_member(
        Method::POST,
        format!("{heap_path}/parameters/{}/use", id(&customer)),
        None,
    )
    .await?;
    let stopped = as_member(
        Method::POST,
        format!("{heap_path}/parameters/{}/stop-using", id(&customer)),
        None,
    )
    .await?;
    let foreign_use = w.use_in(&w.first, &foreign).await?;
    let foreign_archive = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/archive", w.team_path, id(&foreign)),
            Some(&w.session),
            None,
        )
        .await?;
    let foreign_in_other_heap = w
        .app
        .send(
            Method::POST,
            &format!(
                "{}/parameters/{}/use",
                w.app.heap_url(&other_heap).await?,
                id(&customer)
            ),
            Some(&other_session),
            None,
        )
        .await?;
    let outsider = w
        .app
        .send(
            Method::GET,
            &format!("{}/parameters", w.team_path),
            Some(&other_session),
            None,
        )
        .await?;

    // Assert
    expect_that!(listed.status, eq(StatusCode::OK));
    expect_that!(names(&listed.body, "parameters"), elements_are![eq(&"Customer")]);
    expect_that!(listed.body["allowed_actions"], eq(&json!([])));
    expect_that!(listed.body["parameters"][0]["allowed_actions"], eq(&json!([])));
    for refused in [&created, &used, &stopped] {
        expect_that!(refused.status, eq(StatusCode::FORBIDDEN));
    }
    for missing in [&foreign_use, &foreign_archive, &foreign_in_other_heap, &outsider] {
        expect_that!(missing.status, eq(StatusCode::NOT_FOUND));
    }
    Ok(())
}
