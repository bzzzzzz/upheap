//! Making a heap parameter a team parameter: the parameter becomes its team's in place, and the team's same-named heap
//! parameters merge into it, in every heap of the team, once the caller has confirmed what it reaches.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::{TestApp, TestResponse};
use domain::team::Role;
use googletest::prelude::*;
use serde_json::{Value, json};

/// A team with three heaps, A, B and C, and its admin.
struct World {
    app: TestApp,
    team: String,
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
        team,
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

fn option_id(parameter: &Value, label: &str) -> String {
    parameter["options"]
        .as_array()
        .and_then(|options| options.iter().find(|o| o["label"] == label))
        .and_then(|o| o["id"].as_str())
        .unwrap_or_default()
        .to_owned()
}

fn labels(parameter: &Value, field: &str) -> Vec<String> {
    parameter[field]
        .as_array()
        .map(|options| {
            options
                .iter()
                .filter_map(|o| o["label"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// The item's values as `(parameter name, option label)`, for the parameters it has a value for.
fn values(item: &Value) -> Vec<(String, String)> {
    item["parameters"]
        .as_array()
        .map(|parameters| {
            parameters
                .iter()
                .filter_map(|p| {
                    Some((
                        p["name"].as_str()?.to_owned(),
                        p["option"]["label"].as_str()?.to_owned(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn actions(history: &Value) -> Vec<&str> {
    history["entries"]
        .as_array()
        .map(|entries| entries.iter().filter_map(|e| e["action"].as_str()).collect())
        .unwrap_or_default()
}

fn token(refused: &TestResponse) -> String {
    refused.body["parameter_merge_reach"]["token"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

impl World {
    /// A heap parameter that must succeed.
    async fn heap_parameter(&self, heap: &str, name: &str, options: &[&str], weight: u32) -> Result<Value> {
        let made = self
            .app
            .send(
                Method::POST,
                &format!("{}/parameters", self.app.heap_url(heap).await?),
                Some(&self.session),
                Some(json!({ "name": name, "options": options, "weight": weight })),
            )
            .await?;
        anyhow::ensure!(
            made.status == StatusCode::CREATED,
            "make parameter failed: {}",
            made.body
        );
        Ok(made.body)
    }

    async fn make_team(&self, heap: &str, parameter: &Value, confirm: Option<&str>) -> Result<TestResponse> {
        self.make_team_as(&self.session, heap, id(parameter), confirm).await
    }

    async fn make_team_as(
        &self,
        session: &str,
        heap: &str,
        parameter: &str,
        confirm: Option<&str>,
    ) -> Result<TestResponse> {
        let query = confirm
            .map(|token| format!("?confirm_reach={token}"))
            .unwrap_or_default();
        self.app
            .send(
                Method::POST,
                &format!("{}/parameters/{parameter}/team{query}", self.app.heap_url(heap).await?),
                Some(session),
                None,
            )
            .await
    }

    /// Asks, then confirms with the token the first answer gave.
    async fn make_team_confirmed(&self, heap: &str, parameter: &Value) -> Result<TestResponse> {
        let refused = self.make_team(heap, parameter, None).await?;
        anyhow::ensure!(
            refused.status == StatusCode::CONFLICT,
            "not asked to confirm: {}",
            refused.body
        );
        self.make_team(heap, parameter, Some(&token(&refused))).await
    }

    async fn item_with(&self, heap: &str, title: &str, parameter: &Value, label: &str) -> Result<Value> {
        self.app
            .new_item(
                &self.session,
                heap,
                json!({ "title": title, "parameter_values": { id(parameter): option_id(parameter, label) } }),
            )
            .await
    }

    async fn get(&self, path: &str) -> Result<TestResponse> {
        self.app.send(Method::GET, path, Some(&self.session), None).await
    }

    async fn read_item(&self, item: &Value) -> Result<Value> {
        Ok(self.get(&self.app.item_url(id(item)).await?).await?.body)
    }

    async fn close(&self, item: &Value) -> Result<()> {
        let done = self
            .app
            .patch(
                &self.session,
                &self.app.item_url(id(item)).await?,
                &item["version"],
                json!({ "state": "done" }),
            )
            .await?;
        anyhow::ensure!(done.status == StatusCode::OK, "close failed: {}", done.body);
        Ok(())
    }

    async fn heap_parameters(&self, heap: &str) -> Result<Value> {
        Ok(self
            .get(&format!("{}/parameters", self.app.heap_url(heap).await?))
            .await?
            .body)
    }

    /// The heap's parameter in use or archived with `parameter_id`.
    async fn listed(&self, heap: &str, parameter_id: &str) -> Result<Value> {
        let list = self.heap_parameters(heap).await?;
        list["parameters"]
            .as_array()
            .into_iter()
            .chain(list["archived"].as_array())
            .flatten()
            .find(|p| p["id"] == json!(parameter_id))
            .cloned()
            .context("parameter not listed")
    }

    async fn history(&self, heap: &str, entity: &str) -> Result<Value> {
        Ok(self
            .get(&format!("{}/audit?entity_id={entity}", self.app.heap_url(heap).await?))
            .await?
            .body)
    }

    async fn save_view(&self, heap: &str, body: Value) -> Result<Value> {
        let saved = self
            .app
            .send(
                Method::POST,
                &format!("{}/views", self.app.heap_url(heap).await?),
                Some(&self.session),
                Some(body),
            )
            .await?;
        anyhow::ensure!(saved.status == StatusCode::CREATED, "save view failed: {}", saved.body);
        Ok(saved.body)
    }

    async fn set_archived(&self, heap: &str, archived: bool) -> Result<TestResponse> {
        let action = if archived { "archive" } else { "unarchive" };
        self.app
            .send(
                Method::POST,
                &format!("{}/{action}", self.app.heap_url(heap).await?),
                Some(&self.session),
                None,
            )
            .await
    }
}

#[gtest]
#[tokio::test]
async fn a_heap_parameter_becomes_the_teams_and_the_same_named_one_merges_into_it() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 30).await?;
    let tier_b = w
        .heap_parameter(&w.b, "customer tier", &["silver", "gold", "Platinum"], 5)
        .await?;
    let a_gold = w.item_with(&w.a, "A gold", &tier_a, "Gold").await?;
    let a_silver = w.item_with(&w.a, "A silver", &tier_a, "Silver").await?;
    let b_silver = w.item_with(&w.b, "B silver", &tier_b, "silver").await?;
    let b_platinum = w.item_with(&w.b, "B platinum", &tier_b, "Platinum").await?;
    let b_closed = w.item_with(&w.b, "B closed", &tier_b, "gold").await?;
    w.close(&b_closed).await?;
    let view = w
        .save_view(
            &w.b,
            json!({ "name": "Golds", "view": {
                "param": [option_id(&tier_b, "gold")], "sort": "parameter", "sort_parameter": id(&tier_b),
            } }),
        )
        .await?;

    // Act
    let refused = w.make_team(&w.a, &tier_a, None).await?;
    let confirmed = w.make_team(&w.a, &tier_a, Some(&token(&refused))).await?;
    let (a_list, b_list) = (w.heap_parameters(&w.a).await?, w.heap_parameters(&w.b).await?);
    let team = w.get(&format!("{}/parameters", w.team_path)).await?;
    let items = [
        w.read_item(&a_gold).await?,
        w.read_item(&a_silver).await?,
        w.read_item(&b_silver).await?,
        w.read_item(&b_platinum).await?,
        w.read_item(&b_closed).await?,
    ];
    let views = w.get(&format!("{}/views", w.app.heap_url(&w.b).await?)).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(refused.body["code"], eq(&json!("parameter_merge_unconfirmed")));
    let reach = &refused.body["parameter_merge_reach"]["entries"];
    expect_that!(reach.as_array().map(Vec::len), some(eq(2)));
    expect_that!(reach[0]["heap"]["id"], eq(&json!(w.a)));
    expect_that!(reach[0]["parameter_id"], eq(&json!(id(&tier_a))));
    expect_that!(reach[0]["parameter_name"], eq(&json!("Customer tier")));
    expect_that!(reach[0]["items"], eq(&json!(2)));
    expect_that!(reach[0]["added_options"], eq(&json!([])));
    expect_that!(reach[0]["starting"], eq(&json!(true)));
    expect_that!(reach[1]["heap"]["id"], eq(&json!(w.b)));
    expect_that!(reach[1]["archived"], eq(&json!(false)));
    expect_that!(reach[1]["parameter_name"], eq(&json!("customer tier")));
    expect_that!(reach[1]["items"], eq(&json!(3)));
    expect_that!(reach[1]["added_options"], eq(&json!(["Platinum"])));
    expect_that!(reach[1]["starting"], eq(&json!(false)));

    expect_that!(confirmed.status, eq(StatusCode::OK));
    expect_that!(confirmed.body["id"], eq(&json!(id(&tier_a))));
    expect_that!(confirmed.body["name"], eq(&json!("Customer tier")));
    expect_that!(
        labels(&confirmed.body, "options"),
        elements_are![eq(&"Gold"), eq(&"Silver"), eq(&"Platinum")]
    );
    let using: Vec<&str> = confirmed.body["heaps"]
        .as_array()
        .map(|heaps| heaps.iter().filter_map(|h| h["id"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(using, unordered_elements_are![eq(&w.a.as_str()), eq(&w.b.as_str())]);
    expect_that!(confirmed.body["tag_ids"], eq(&json!([])));
    let team_ids: Vec<&str> = team.body["parameters"]
        .as_array()
        .map(|all| all.iter().filter_map(|p| p["id"].as_str()).collect())
        .unwrap_or_default();
    expect_that!(team_ids, elements_are![eq(&id(&tier_a))]);

    // Each heap lists it as a team parameter at the weight it had.
    for (list, weight) in [(&a_list, 30), (&b_list, 5)] {
        let parameter = list["parameters"]
            .as_array()
            .and_then(|all| all.iter().find(|p| p["id"] == json!(id(&tier_a))));
        expect_that!(parameter.map(|p| p["kind"].clone()), some(eq(&json!("team"))));
        expect_that!(parameter.map(|p| p["weight"].clone()), some(eq(&json!(weight))));
        expect_that!(
            parameter.map(|p| p["allowed_actions"].clone()),
            some(eq(&json!(["link_tags", "stop_using", "view_history"])))
        );
    }
    // The merged parameter is archived in its heap.
    let archived = b_list["archived"]
        .as_array()
        .and_then(|all| all.iter().find(|p| p["id"] == json!(id(&tier_b))));
    expect_that!(archived.map(|p| p["kind"].clone()), some(eq(&json!("heap"))));
    expect_that!(archived.map(|p| p["archived_at"].is_number()), some(eq(true)));
    expect_that!(
        b_list["parameters"]
            .as_array()
            .map(|all| all.iter().any(|p| p["id"] == json!(id(&tier_b)))),
        some(eq(false))
    );

    // Every item's value is the matched team option, the closed one included.
    let tier = |label: &str| ("Customer tier".to_owned(), label.to_owned());
    expect_that!(values(&items[0]), elements_are![eq(&tier("Gold"))]);
    expect_that!(values(&items[1]), elements_are![eq(&tier("Silver"))]);
    expect_that!(values(&items[2]), elements_are![eq(&tier("Silver"))]);
    expect_that!(values(&items[3]), elements_are![eq(&tier("Platinum"))]);
    expect_that!(values(&items[4]), elements_are![eq(&tier("Gold"))]);
    expect_that!(items[3]["parameters"][0]["parameter_id"], eq(&json!(id(&tier_a))));

    // The view filters by the team's "Gold" and sorts by the team parameter.
    let saved = views.body["views"]
        .as_array()
        .and_then(|all| all.iter().find(|v| v["id"] == json!(id(&view))))
        .context("view not listed")?;
    expect_that!(saved["view"]["param"], eq(&json!([option_id(&confirmed.body, "Gold")])));
    expect_that!(saved["view"]["sort_parameter"], eq(&json!(id(&tier_a))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_view_follows_the_merge_too() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 30).await?;
    let tier_b = w.heap_parameter(&w.b, "customer tier", &["gold"], 5).await?;
    let view = w
        .save_view(
            &w.b,
            json!({ "name": "Golds", "view": {
                "param": [option_id(&tier_b, "gold")], "sort": "parameter", "sort_parameter": id(&tier_b),
            } }),
        )
        .await?;
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{}/views/{}/archive", w.app.heap_url(&w.b).await?, id(&view)),
            Some(&w.session),
            None,
        )
        .await?;

    // Act
    let confirmed = w.make_team_confirmed(&w.a, &tier_a).await?;
    let stored = w.app.stored_view(id(&view)).await?;

    // Assert
    expect_that!(archived.status, eq(StatusCode::OK));
    expect_that!(confirmed.status, eq(StatusCode::OK));
    expect_that!(stored.archived_at.is_some(), eq(true));
    let options: Vec<String> = stored
        .list_view
        .filter
        .options
        .iter()
        .map(ToString::to_string)
        .collect();
    expect_that!(options, elements_are![eq(&option_id(&confirmed.body, "Gold"))]);
    let team_parameter = domain::ParameterId::from_uuid(id(&tier_a).parse()?);
    expect_that!(
        stored.list_view.sort,
        eq(domain::list_view::SortKey::Parameter(team_parameter))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_merge_is_in_the_history_of_the_items_it_moved_and_of_the_team() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 30).await?;
    let tier_b = w.heap_parameter(&w.b, "customer tier", &["silver", "gold"], 5).await?;
    let in_a = w.item_with(&w.a, "In A", &tier_a, "Gold").await?;
    let in_b = w.item_with(&w.b, "In B", &tier_b, "silver").await?;

    // Act
    let made = w.make_team_confirmed(&w.a, &tier_a).await?;
    let b_history = w.history(&w.b, id(&in_b)).await?;
    let a_history = w.history(&w.a, id(&in_a)).await?;
    let team_log = w.get(&format!("{}/audit", w.team_path)).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(
        actions(&b_history),
        elements_are![eq(&"parameter_merged"), eq(&"item_created")]
    );
    let merged = &b_history["entries"][0];
    expect_that!(merged["phrase"], eq(&json!("merged a parameter on")));
    expect_that!(
        merged["changes"],
        eq(&json!([
            { "field": format!("parameters.{}", id(&tier_a)), "label": "Customer tier", "before": null, "after": "Silver", "text_diff": null },
            { "field": format!("parameters.{}", id(&tier_b)), "label": "customer tier", "before": "silver", "after": null, "text_diff": null },
        ]))
    );
    // The entry that made the value still names the old parameter and option.
    let earlier = b_history["entries"][1]["changes"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    expect_that!(
        earlier,
        contains(eq(&json!({
            "field": format!("parameters.{}", id(&tier_b)), "label": "customer tier", "before": null, "after": "silver", "text_diff": null,
        })))
    );
    // Heap A's item didn't change.
    expect_that!(actions(&a_history), elements_are![eq(&"item_created")]);
    let team_entry = team_log.body["entries"]
        .as_array()
        .and_then(|entries| entries.iter().find(|e| e["action"] == json!("parameter_merged")))
        .context("no merge entry in the team's log")?;
    expect_that!(team_entry["phrase"], eq(&json!("made a team parameter of")));
    expect_that!(
        team_entry["changes"][0]["after"],
        eq(&json!(
            "Shared's \u{201c}Customer tier\u{201d} (1 item), B's \u{201c}customer tier\u{201d} (1 item)"
        ))
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_stale_token_is_refused_with_the_new_reach() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let tier_b = w.heap_parameter(&w.b, "Customer tier", &["Gold"], 10).await?;
    w.item_with(&w.b, "One", &tier_b, "Gold").await?;
    let first = w.make_team(&w.a, &tier_a, None).await?;
    w.item_with(&w.b, "Two", &tier_b, "Gold").await?;

    // Act
    let stale = w.make_team(&w.a, &tier_a, Some(&token(&first))).await?;
    let confirmed = w.make_team(&w.a, &tier_a, Some(&token(&stale))).await?;

    // Assert
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("parameter_merge_unconfirmed")));
    expect_that!(
        stale.body["parameter_merge_reach"]["entries"][1]["items"],
        eq(&json!(2))
    );
    expect_that!(token(&stale), not(eq(&token(&first))));
    expect_that!(confirmed.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn tag_links_carry_over_as_each_heaps_own_links_and_suggest_only_there() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let tier_b = w.heap_parameter(&w.b, "customer tier", &["Gold"], 10).await?;
    let local = w.app.new_tag_id(&w.session, &w.a, "vip").await?;
    let team_tag = w
        .app
        .send(
            Method::POST,
            &format!("{}/tags", w.team_path),
            Some(&w.session),
            Some(json!({ "name": "billing" })),
        )
        .await?;
    anyhow::ensure!(
        team_tag.status == StatusCode::CREATED,
        "team tag failed: {}",
        team_tag.body
    );
    let billing = id(&team_tag.body).to_owned();
    for (heap, parameter, tag) in [(&w.a, &tier_a, &local), (&w.b, &tier_b, &billing)] {
        let linked = w
            .app
            .send_with(
                Method::PUT,
                &format!("{}/parameters/{}/tags", w.app.heap_url(heap).await?, id(parameter)),
                Some(&w.session),
                Some(json!({ "tag_ids": [tag] })),
                &[common::CSRF, ("if-match", &parameter["version"].to_string())],
            )
            .await?;
        anyhow::ensure!(linked.status == StatusCode::OK, "link failed: {}", linked.body);
    }

    // Act
    let refused = w.make_team(&w.a, &tier_a, None).await?;
    let made = w.make_team(&w.a, &tier_a, Some(&token(&refused))).await?;
    let used = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/use", w.app.heap_url(&w.c).await?, id(&tier_a)),
            Some(&w.session),
            None,
        )
        .await?;
    let (in_a, in_b, in_c) = (
        w.listed(&w.a, id(&tier_a)).await?,
        w.listed(&w.b, id(&tier_a)).await?,
        w.listed(&w.c, id(&tier_a)).await?,
    );
    let team = w.get(&format!("{}/parameters", w.team_path)).await?.body["parameters"][0].clone();

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(
        refused.body["parameter_merge_reach"]["entries"][0]["linked_tags"],
        eq(&json!(["vip"]))
    );
    expect_that!(
        refused.body["parameter_merge_reach"]["entries"][1]["linked_tags"],
        eq(&json!(["billing"]))
    );
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(used.status, eq(StatusCode::OK));
    expect_that!(team["tag_ids"], eq(&json!([])));
    // A suggests it for its own tag, B for the team tag only B linked; C, which uses it too, for neither.
    expect_that!(in_a["tag_ids"], eq(&json!([local])));
    expect_that!(in_a["heap_tag_ids"], eq(&json!([local])));
    expect_that!(in_b["tag_ids"], eq(&json!([billing])));
    expect_that!(in_b["heap_tag_ids"], eq(&json!([billing])));
    expect_that!(in_c["tag_ids"], eq(&json!([])));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_alone_moves_to_the_team_with_its_items_and_weight() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 42).await?;
    let item = w.item_with(&w.a, "One", &tier, "Silver").await?;

    // Act
    let refused = w.make_team(&w.a, &tier, None).await?;
    let made = w.make_team(&w.a, &tier, Some(&token(&refused))).await?;
    let again = w.make_team(&w.a, &tier, Some(&token(&refused))).await?;
    let listed = w.listed(&w.a, id(&tier)).await?;
    let item = w.read_item(&item).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::CONFLICT));
    expect_that!(
        refused.body["parameter_merge_reach"]["entries"]
            .as_array()
            .map(Vec::len),
        some(eq(1))
    );
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(made.body["heaps"][0]["id"], eq(&json!(w.a)));
    expect_that!(listed["kind"], eq(&json!("team")));
    expect_that!(listed["weight"], eq(&json!(42)));
    expect_that!(
        values(&item),
        elements_are![eq(&("Customer tier".to_owned(), "Silver".to_owned()))]
    );
    // Repeating the write succeeds with the team parameter and changes nothing.
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body, eq(&made.body));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn only_an_admin_may_make_one_and_only_from_a_live_parameter_in_a_live_heap() -> Result<()> {
    // Arrange
    let w = world().await?;
    let (member, _) = w.app.register("bob@example.test").await?;
    let me = w.app.send(Method::GET, "/api/v1/me", Some(&member), None).await?;
    let bob = me.body["user"]["id"].as_str().context("no user id")?.to_owned();
    w.app.add_member(&w.team, &bob, Role::Member).await?;
    let tier = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let old = w.heap_parameter(&w.a, "Old", &["Gold"], 10).await?;
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/archive", w.app.heap_url(&w.a).await?, id(&old)),
            Some(&w.session),
            None,
        )
        .await?;
    anyhow::ensure!(archived.status == StatusCode::OK, "archive failed: {}", archived.body);
    let team_parameter = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters", w.team_path),
            Some(&w.session),
            Some(json!({ "name": "Area", "options": ["Web"] })),
        )
        .await?
        .body;
    let other = w.heap_parameter(&w.b, "Elsewhere", &["Gold"], 10).await?;

    // Act
    let by_member = w.make_team_as(&member, &w.a, id(&tier), None).await?;
    let for_archived = w.make_team(&w.a, &old, None).await?;
    let for_team_parameter = w.make_team(&w.a, &team_parameter, None).await?;
    let for_other_heap = w.make_team(&w.a, &other, None).await?;
    let unknown = w
        .make_team_as(&w.session, &w.a, "00000000-0000-7000-8000-000000000000", None)
        .await?;
    let listed = w.listed(&w.a, id(&tier)).await?;
    let asked = w.make_team(&w.a, &tier, None).await?;
    w.set_archived(&w.a, true).await?;
    let in_archived_heap = w.make_team(&w.a, &tier, Some(&token(&asked))).await?;

    // Assert
    expect_that!(by_member.status, eq(StatusCode::FORBIDDEN));
    expect_that!(for_archived.status, eq(StatusCode::CONFLICT));
    expect_that!(for_archived.body["code"], eq(&json!("parameter_archived")));
    for refused in [&for_team_parameter, &for_other_heap, &unknown] {
        expect_that!(refused.status, eq(StatusCode::NOT_FOUND));
    }
    expect_that!(
        listed["allowed_actions"]
            .as_array()
            .is_some_and(|actions| actions.contains(&json!("make_team"))),
        is_true()
    );
    expect_that!(in_archived_heap.status, eq(StatusCode::CONFLICT));
    expect_that!(in_archived_heap.body["code"], eq(&json!("heap_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_merged_parameter_is_archived_and_its_id_is_refused_afterwards() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let tier_b = w.heap_parameter(&w.b, "Customer tier", &["Gold"], 10).await?;

    // Act
    let made = w.make_team_confirmed(&w.a, &tier_a).await?;
    let again = w.make_team(&w.b, &tier_b, None).await?;
    let current = w.listed(&w.b, id(&tier_b)).await?;
    let renamed = w
        .app
        .patch(
            &w.session,
            &format!("{}/parameters/{}", w.app.heap_url(&w.b).await?, id(&tier_b)),
            &current["version"],
            json!({ "name": "Other" }),
        )
        .await?;
    let name_taken = w.heap_parameter_named_like_the_team_one(&w.c).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(again.status, eq(StatusCode::CONFLICT));
    expect_that!(again.body["code"], eq(&json!("parameter_archived")));
    expect_that!(renamed.status, eq(StatusCode::CONFLICT));
    expect_that!(renamed.body["code"], eq(&json!("parameter_archived")));
    // The team parameter holds the name now.
    expect_that!(name_taken.status, eq(StatusCode::CONFLICT));
    expect_that!(name_taken.body["code"], eq(&json!("parameter_name_taken")));
    Ok(())
}

impl World {
    async fn heap_parameter_named_like_the_team_one(&self, heap: &str) -> Result<TestResponse> {
        self.app
            .send(
                Method::POST,
                &format!("{}/parameters", self.app.heap_url(heap).await?),
                Some(&self.session),
                Some(json!({ "name": "customer TIER", "options": ["Gold"] })),
            )
            .await
    }
}

#[gtest]
#[tokio::test]
async fn a_parameter_of_an_archived_heap_is_merged_and_an_archived_one_is_left_alone() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let tier_b = w.heap_parameter(&w.b, "Customer tier", &["Gold", "Bronze"], 7).await?;
    let old_c = w.heap_parameter(&w.c, "Customer tier", &["Gold"], 10).await?;
    let item = w.item_with(&w.b, "In B", &tier_b, "Bronze").await?;
    let archived_parameter = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/archive", w.app.heap_url(&w.c).await?, id(&old_c)),
            Some(&w.session),
            None,
        )
        .await?;
    anyhow::ensure!(
        archived_parameter.status == StatusCode::OK,
        "archive failed: {}",
        archived_parameter.body
    );
    let archived_heap = w.set_archived(&w.b, true).await?;

    // Act
    let refused = w.make_team(&w.a, &tier_a, None).await?;
    let made = w.make_team(&w.a, &tier_a, Some(&token(&refused))).await?;
    let unarchived = w.set_archived(&w.b, false).await?;
    let item = w.read_item(&item).await?;
    let c_list = w.heap_parameters(&w.c).await?;

    // Assert
    expect_that!(archived_heap.status, eq(StatusCode::OK));
    expect_that!(unarchived.status, eq(StatusCode::OK));
    let entries = &refused.body["parameter_merge_reach"]["entries"];
    expect_that!(entries.as_array().map(Vec::len), some(eq(2)));
    expect_that!(entries[1]["heap"]["id"], eq(&json!(w.b)));
    expect_that!(entries[1]["archived"], eq(&json!(true)));
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(labels(&made.body, "options"), elements_are![eq(&"Gold"), eq(&"Bronze")]);
    expect_that!(
        values(&item),
        elements_are![eq(&("Customer tier".to_owned(), "Bronze".to_owned()))]
    );
    let left = c_list["archived"]
        .as_array()
        .and_then(|all| all.iter().find(|p| p["id"] == json!(id(&old_c))));
    expect_that!(left.map(|p| p["kind"].clone()), some(eq(&json!("heap"))));
    expect_that!(
        c_list["parameters"].as_array().map(Vec::len),
        some(eq(0)),
        "heap C doesn't use the team parameter"
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn open_items_end_on_an_option_in_use_and_closed_ones_keep_the_archived_one() -> Result<()> {
    // Arrange: the team's options are Gold and an archived "Silver" that Gold replaced; heap B has a live "Silver".
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 10).await?;
    let archiving = w
        .app
        .send(
            Method::POST,
            &format!(
                "{}/options/{}/archive",
                w.app.heap_url(&w.a).await?,
                option_id(&tier_a, "Silver")
            ),
            Some(&w.session),
            Some(json!({ "replacement_option_id": option_id(&tier_a, "Gold") })),
        )
        .await?;
    anyhow::ensure!(
        archiving.status == StatusCode::OK,
        "archive option failed: {}",
        archiving.body
    );
    let tier_b = w
        .heap_parameter(&w.b, "Customer tier", &["Silver", "Bronze"], 10)
        .await?;
    let open = w.item_with(&w.b, "Open", &tier_b, "Silver").await?;
    let closed = w.item_with(&w.b, "Closed", &tier_b, "Silver").await?;
    w.close(&closed).await?;

    // Act
    let asked = w.make_team(&w.a, &tier_a, None).await?;
    let made = w.make_team(&w.a, &tier_a, Some(&token(&asked))).await?;
    let (open, closed) = (w.read_item(&open).await?, w.read_item(&closed).await?);

    // Assert
    let entries = &asked.body["parameter_merge_reach"]["entries"];
    expect_that!(entries[0]["switched"], eq(&json!([])));
    expect_that!(entries[1]["switched"], eq(&json!([{ "from": "Silver", "to": "Gold" }])));
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(labels(&made.body, "options"), elements_are![eq(&"Gold"), eq(&"Bronze")]);
    expect_that!(labels(&made.body, "archived_options"), elements_are![eq(&"Silver")]);
    expect_that!(open["parameters"][0]["option"]["label"], eq(&json!("Gold")));
    expect_that!(open["parameters"][0]["option"]["archived"], eq(&json!(false)));
    expect_that!(closed["parameters"][0]["option"]["label"], eq(&json!("Silver")));
    expect_that!(closed["parameters"][0]["option"]["archived"], eq(&json!(true)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_token_taken_before_the_team_parameter_archives_an_option_is_refused_afterwards() -> Result<()> {
    // Arrange: heap B's live "Silver" matches a live option of the parameter when the token is taken.
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 10).await?;
    let tier_b = w
        .heap_parameter(&w.b, "Customer tier", &["Silver", "Bronze"], 10)
        .await?;
    w.item_with(&w.b, "Open", &tier_b, "Silver").await?;
    let before = w.make_team(&w.a, &tier_a, None).await?;
    let archiving = w
        .app
        .send(
            Method::POST,
            &format!(
                "{}/options/{}/archive",
                w.app.heap_url(&w.a).await?,
                option_id(&tier_a, "Silver")
            ),
            Some(&w.session),
            Some(json!({ "replacement_option_id": option_id(&tier_a, "Gold") })),
        )
        .await?;
    anyhow::ensure!(archiving.status == StatusCode::OK, "archive failed: {}", archiving.body);

    // Act
    let stale = w.make_team(&w.a, &tier_a, Some(&token(&before))).await?;
    let confirmed = w.make_team(&w.a, &tier_a, Some(&token(&stale))).await?;

    // Assert
    expect_that!(
        before.body["parameter_merge_reach"]["entries"][1]["switched"],
        eq(&json!([]))
    );
    expect_that!(stale.status, eq(StatusCode::CONFLICT));
    expect_that!(stale.body["code"], eq(&json!("parameter_merge_unconfirmed")));
    expect_that!(token(&stale), not(eq(&token(&before))));
    expect_that!(
        stale.body["parameter_merge_reach"]["entries"][1]["switched"],
        eq(&json!([{ "from": "Silver", "to": "Gold" }]))
    );
    expect_that!(confirmed.status, eq(StatusCode::OK));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn repeating_the_write_on_a_team_parameter_the_heap_uses_succeeds_and_changes_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let made = w.make_team_confirmed(&w.a, &tier_a).await?;
    anyhow::ensure!(made.status == StatusCode::OK, "make failed: {}", made.body);
    let history = w.history(&w.a, id(&tier_a)).await?;

    // Act
    let again = w.make_team(&w.a, &tier_a, None).await?;
    let history_after = w.history(&w.a, id(&tier_a)).await?;
    let from_heap_c = w.make_team_as(&w.session, &w.c, id(&tier_a), None).await?;

    // Assert
    expect_that!(again.status, eq(StatusCode::OK));
    expect_that!(again.body["id"], eq(&json!(id(&tier_a))));
    expect_that!(again.body["heaps"], eq(&made.body["heaps"]));
    expect_that!(history_after, eq(&history));
    expect_that!(from_heap_c.status, eq(StatusCode::NOT_FOUND));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn repeating_the_write_on_an_archived_team_parameter_is_refused() -> Result<()> {
    // Arrange
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold"], 10).await?;
    let made = w.make_team_confirmed(&w.a, &tier_a).await?;
    anyhow::ensure!(made.status == StatusCode::OK, "make failed: {}", made.body);
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters/{}/archive", w.team_path, id(&tier_a)),
            Some(&w.session),
            None,
        )
        .await?;
    anyhow::ensure!(archived.status == StatusCode::OK, "archive failed: {}", archived.body);

    // Act
    let again = w.make_team(&w.a, &tier_a, None).await?;

    // Assert
    expect_that!(again.body["code"], eq(&json!("parameter_archived")));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_item_moved_into_the_merged_heap_keeps_its_value() -> Result<()> {
    // Arrange: an item of heap A moved to heap B keeps a cleared row for A's parameter, and takes B's value.
    let w = world().await?;
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &["Gold", "Silver"], 10).await?;
    let tier_b = w.heap_parameter(&w.b, "Customer tier", &["Gold", "Silver"], 10).await?;
    let item = w.item_with(&w.a, "Mover", &tier_a, "Silver").await?;
    let moved = w
        .app
        .send_with(
            Method::POST,
            &format!("{}/move", w.app.item_url(id(&item)).await?),
            Some(&w.session),
            Some(json!({ "heap_id": w.b })),
            &[common::CSRF, ("if-match", &item["version"].to_string())],
        )
        .await?;
    anyhow::ensure!(moved.status == StatusCode::OK, "move failed: {}", moved.body);
    let in_b = w.read_item(&item).await?;
    anyhow::ensure!(
        values(&in_b) == [("Customer tier".to_owned(), "Silver".to_owned())],
        "the move kept no value: {in_b}"
    );
    anyhow::ensure!(
        in_b["parameters"][0]["parameter_id"] == json!(id(&tier_b)),
        "not B's value"
    );

    // Act
    let made = w.make_team_confirmed(&w.a, &tier_a).await?;
    let item = w.read_item(&item).await?;

    // Assert
    expect_that!(made.status, eq(StatusCode::OK));
    expect_that!(
        values(&item),
        elements_are![eq(&("Customer tier".to_owned(), "Silver".to_owned()))]
    );
    expect_that!(item["parameters"][0]["parameter_id"], eq(&json!(id(&tier_a))));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_merge_that_leaves_too_many_options_changes_nothing() -> Result<()> {
    // Arrange
    let w = world().await?;
    let many: Vec<String> = (0..50).map(|n| format!("Option {n}")).collect();
    let many: Vec<&str> = many.iter().map(String::as_str).collect();
    let tier_a = w.heap_parameter(&w.a, "Customer tier", &many, 10).await?;
    let tier_b = w.heap_parameter(&w.b, "Customer tier", &["Extra"], 10).await?;
    let item = w.item_with(&w.b, "One", &tier_b, "Extra").await?;

    // Act
    let refused = w.make_team(&w.a, &tier_a, None).await?;
    let b_list = w.heap_parameters(&w.b).await?;
    let item = w.read_item(&item).await?;

    // Assert
    expect_that!(refused.status, eq(StatusCode::UNPROCESSABLE_ENTITY));
    expect_that!(refused.body["code"], eq(&json!("too_many_options")));
    expect_that!(b_list["parameters"][0]["id"], eq(&json!(id(&tier_b))));
    expect_that!(
        values(&item),
        elements_are![eq(&("Customer tier".to_owned(), "Extra".to_owned()))]
    );
    Ok(())
}
