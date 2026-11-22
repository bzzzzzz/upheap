//! One ranked list across all heaps (`GET /me/items`).

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use googletest::prelude::*;
use serde_json::{Value, json};

/// Makes an item of `need` in `heap` and returns the whole item.
async fn item(app: &TestApp, session: &str, heap: &str, title: &str, need: &str) -> Result<Value> {
    app.new_item(session, heap, json!({ "title": title, "need": need }))
        .await
}

/// `GET /me/items` with `query` (like `?need=critical`, or empty).
async fn across(app: &TestApp, session: &str, query: &str) -> Result<Value> {
    let response = app
        .send(Method::GET, &format!("/api/v1/me/items{query}"), Some(session), None)
        .await?;
    anyhow::ensure!(response.status == StatusCode::OK, "list failed: {}", response.body);
    Ok(response.body)
}

/// The titles of the rows of `group`, in order.
fn titles(list: &Value, group: &str) -> Vec<String> {
    list[group]
        .as_array()
        .map(|rows| {
            rows.iter()
                .map(|row| row["entry"]["item"]["title"].as_str().unwrap_or_default().to_owned())
                .collect()
        })
        .unwrap_or_default()
}

/// Makes a heap called `name` in the team of `team_id`, and returns its ID.
async fn make_heap(app: &TestApp, session: &str, team_id: &str, name: &str) -> Result<String> {
    let team_url = app.team_url(team_id).await?;
    let made = app
        .send(
            Method::POST,
            &format!("{team_url}/heaps"),
            Some(session),
            Some(json!({ "name": name })),
        )
        .await?;
    anyhow::ensure!(made.status == StatusCode::CREATED, "heap failed: {}", made.body);
    Ok(made.body["id"].as_str().context("heap id")?.to_owned())
}

async fn patch_state(app: &TestApp, session: &str, item: &Value, body: Value) -> Result<Value> {
    let saved = app
        .patch(
            session,
            &app.item_url(item["id"].as_str().context("id")?).await?,
            &item["version"],
            body,
        )
        .await?;
    anyhow::ensure!(saved.status == StatusCode::OK, "patch failed: {}", saved.body);
    Ok(saved.body)
}

#[gtest]
#[tokio::test]
async fn nothing_listed_is_an_empty_list() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(0)));
    expect_that!(list["pinned"], eq(&json!([])));
    expect_that!(list["ranked"], eq(&json!([])));
    expect_that!(list["blocked"], eq(&json!([])));
    expect_that!(list["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_limits_the_list_to_its_heaps_in_the_same_relative_order() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "A critical", "critical").await?;
    item(&app, &session, &shared, "B expected", "expected").await?;
    item(&app, &session, &personal, "A idea", "idea").await?;
    item(&app, &session, &shared, "B nice", "nice_to_have").await?;

    // Act
    let all = across(&app, &session, "").await?;
    let scoped = across(&app, &session, "?team=shared").await?;

    // Assert
    expect_that!(all["count"], eq(&json!(4)));
    expect_that!(scoped["count"], eq(&json!(2)));
    let in_all: Vec<String> = titles(&all, "ranked")
        .into_iter()
        .filter(|title| title.starts_with('B'))
        .collect();
    expect_that!(titles(&scoped, "ranked"), eq(&in_all));
    expect_that!(titles(&scoped, "ranked").len(), eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_the_caller_is_not_in_is_not_found() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    app.register("bob@example.test").await?;

    // Act
    let other = app
        .send(Method::GET, "/api/v1/me/items?team=bob", Some(&session), None)
        .await?;
    let missing = app
        .send(Method::GET, "/api/v1/me/items?team=no-such-team", Some(&session), None)
        .await?;
    let malformed = app
        .send(
            Method::GET,
            "/api/v1/me/items?team=Not%20A%20Handle",
            Some(&session),
            None,
        )
        .await?;

    // Assert
    for response in [other, missing, malformed] {
        expect_that!(response.status, eq(StatusCode::NOT_FOUND));
        expect_that!(response.body["code"], eq(&json!("not_found")));
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn items_are_ordered_by_score_across_heaps_each_with_its_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "A critical", "critical").await?;
    item(&app, &session, &personal, "A idea", "idea").await?;
    item(&app, &session, &shared, "B expected", "expected").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(3)));
    expect_that!(
        titles(&list, "ranked"),
        elements_are![eq("A critical"), eq("B expected"), eq("A idea")]
    );
    let rows = list["ranked"].as_array().context("rows")?;
    expect_that!(rows[0]["heap"]["id"], eq(&json!(personal)));
    expect_that!(rows[1]["heap"]["id"], eq(&json!(shared)));
    expect_that!(rows[1]["heap"]["slug"], eq(&json!("shared")));
    expect_that!(rows[1]["heap"]["team_slug"], eq(&json!("shared")));
    expect_that!(rows[1]["heap"]["team_name"], eq(&json!("Shared")));
    expect_that!(
        rows.iter().map(|row| row["position"].clone()).collect::<Vec<_>>(),
        elements_are![eq(&json!(1)), eq(&json!(2)), eq(&json!(3))]
    );
    // The entry's own position is within its heap.
    expect_that!(
        rows.iter()
            .map(|row| row["entry"]["position"].clone())
            .collect::<Vec<_>>(),
        elements_are![eq(&json!(1)), eq(&json!(1)), eq(&json!(2))]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn equal_scores_in_two_heaps_order_by_heap_name_whatever_order_the_heaps_were_made_in() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, _) = app.shared_heap(&session).await?;
    // Made in reverse, so neither creation order nor ID order is the name order.
    let beta = make_heap(&app, &session, &team, "Beta").await?;
    let alpha = make_heap(&app, &session, &team, "Alpha").await?;
    item(&app, &session, &beta, "In Beta", "expected").await?;
    item(&app, &session, &alpha, "In Alpha", "expected").await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("In Alpha"), eq("In Beta")]);
    Ok(())
}

#[gtest]
#[tokio::test]
async fn in_progress_items_are_pinned_and_blocked_ones_hidden_unless_shown() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    let started = item(&app, &session, &personal, "Started", "idea").await?;
    patch_state(&app, &session, &started, json!({ "state": "in_progress" })).await?;
    let also_started = item(&app, &session, &shared, "Also started", "critical").await?;
    patch_state(&app, &session, &also_started, json!({ "state": "in_progress" })).await?;
    item(&app, &session, &personal, "Plain", "expected").await?;
    let blocker = item(&app, &session, &personal, "The blocker", "idea").await?;
    for (heap, title) in [(&personal, "Blocked here"), (&shared, "Blocked there")] {
        let waiting = item(&app, &session, heap, title, "expected").await?;
        let on = if heap == &personal {
            blocker.clone()
        } else {
            item(&app, &session, heap, "Its blocker", "idea").await?
        };
        patch_state(
            &app,
            &session,
            &waiting,
            json!({ "state": "blocked", "blocked_by_item_id": on["id"] }),
        )
        .await?;
    }

    // Act
    let by_default = across(&app, &session, "").await?;
    let shown = across(&app, &session, "?blocked=shown").await?;

    // Assert
    expect_that!(
        titles(&by_default, "pinned"),
        elements_are![eq("Also started"), eq("Started")]
    );
    expect_that!(titles(&by_default, "blocked"), is_empty());
    expect_that!(by_default["hidden_count"], eq(&json!(2)));
    expect_that!(by_default["in_progress_count"], eq(&json!(2)));
    expect_that!(
        titles(&shown, "blocked"),
        unordered_elements_are![eq("Blocked here"), eq("Blocked there")]
    );
    expect_that!(shown["hidden_count"], eq(&json!(0)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_filter_applies_to_every_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &personal, "Critical here", "critical").await?;
    item(&app, &session, &personal, "Idea here", "idea").await?;
    item(&app, &session, &shared, "Critical there", "critical").await?;
    item(&app, &session, &shared, "Expected there", "expected").await?;

    // Act
    let list = across(&app, &session, "?need=critical").await?;

    // Assert
    expect_that!(list["count"], eq(&json!(2)));
    expect_that!(
        titles(&list, "ranked"),
        unordered_elements_are![eq("Critical here"), eq("Critical there")]
    );
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_agent_filter_applies_to_every_heap() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let sent = item(&app, &session, &personal, "Sent", "expected").await?;
    item(&app, &session, &personal, "Unsent", "expected").await?;
    let turn = app
        .send(
            Method::POST,
            &format!("{}/turn", app.item_url(sent["id"].as_str().context("id")?).await?),
            Some(&session),
            Some(json!({ "reach": "one" })),
        )
        .await?;
    anyhow::ensure!(turn.status == StatusCode::CREATED, "send failed: {}", turn.body);

    // Act
    let list = across(&app, &session, "?agent=queued").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("Sent")]);
    expect_that!(list["count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn archived_heaps_and_other_peoples_heaps_are_left_out() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, personal) = app.register("ada@example.test").await?;
    let (team, _) = app.shared_heap(&session).await?;
    let (other, bobs_heap) = app.register("bob@example.test").await?;
    let archived = make_heap(&app, &session, &team, "Archived").await?;
    item(&app, &session, &personal, "Mine", "expected").await?;
    item(&app, &session, &archived, "In an archived heap", "critical").await?;
    item(&app, &other, &bobs_heap, "Bob's", "critical").await?;
    app.set_heap_archived(&archived, true).await?;

    // Act
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("Mine")]);
    expect_that!(list["count"], eq(&json!(1)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_of_a_single_heap_list_is_refused() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;

    for query in [
        "?tag=00000000-0000-0000-0000-000000000000",
        "?sort=due",
        "?sort_parameter=00000000-0000-0000-0000-000000000000",
        "?param=00000000-0000-0000-0000-000000000000",
        "?nope=1",
    ] {
        // Act
        let response = app
            .send(Method::GET, &format!("/api/v1/me/items{query}"), Some(&session), None)
            .await?;

        // Assert
        expect_that!(response.status, eq(StatusCode::BAD_REQUEST), "{query}");
        expect_that!(response.body["code"], eq(&json!("invalid_query")), "{query}");
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn the_count_says_how_many_are_listed_when_the_cap_holds_back_rows() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, heap) = app.register("ada@example.test").await?;
    let me = app.send(Method::GET, "/api/v1/me", Some(&session), None).await?;
    let user_id = me.body["user"]["id"].as_str().context("no id")?.to_owned();
    for title in ["First", "Second", "Third"] {
        item(&app, &session, &heap, title, "expected").await?;
    }

    // Act
    let (count, rows) = app.across_heaps_capped(&user_id, 2).await?;

    // Assert
    expect_that!(count, eq(3));
    expect_that!(rows, eq(2));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_row_matches_the_entry_its_heaps_own_list_gives() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (_, shared) = app.shared_heap(&session).await?;
    item(&app, &session, &shared, "Critical", "critical").await?;
    item(&app, &session, &shared, "Expected", "expected").await?;
    let one_heap = app
        .send(
            Method::GET,
            &format!("{}/items", app.heap_url(&shared).await?),
            Some(&session),
            None,
        )
        .await?;

    // Act
    let list = across(&app, &session, &format!("?as_of={}", one_heap.body["as_of"])).await?;

    // Assert
    expect_that!(list["as_of"], eq(&one_heap.body["as_of"]));
    for (row, own) in list["ranked"]
        .as_array()
        .context("rows")?
        .iter()
        .zip(one_heap.body["ranked"].as_array().context("own rows")?)
    {
        expect_that!(row["entry"], eq(own));
    }
    expect_that!(list["ranked"].as_array().map(Vec::len), some(eq(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heaps_priority_multiplies_its_scores_in_the_merge() -> Result<()> {
    // Arrange: heap A's only item outscores heap B's.
    let app = TestApp::start().await?;
    let (session, a) = app.register("ada@example.test").await?;
    let (_, b) = app.shared_heap(&session).await?;
    item(&app, &session, &a, "In A", "critical").await?;
    item(&app, &session, &b, "In B", "expected").await?;
    let before = across(&app, &session, "").await?;

    // Act
    let set = app.set_priority(&session, &b, 10).await?;
    let after = across(&app, &session, "").await?;

    // Assert
    expect_that!(set.status, eq(StatusCode::OK));
    expect_that!(titles(&before, "ranked"), elements_are![eq("In A"), eq("In B")]);
    expect_that!(titles(&after, "ranked"), elements_are![eq("In B"), eq("In A")]);
    let rows = after["ranked"].as_array().context("rows")?;
    expect_that!(rows[0]["heap_priority"], eq(&json!(10)));
    expect_that!(rows[1]["heap_priority"], eq(&json!(1)));
    expect_that!(before["ranked"][0]["heap_priority"], eq(&json!(1)));
    // Positions within a heap and the count don't change.
    expect_that!(after["count"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heaps_priority_leaves_the_order_within_the_heap_alone() -> Result<()> {
    // Arrange
    let app = TestApp::start().await?;
    let (session, a) = app.register("ada@example.test").await?;
    item(&app, &session, &a, "Low", "idea").await?;
    item(&app, &session, &a, "High", "critical").await?;

    // Act
    app.set_priority(&session, &a, 7).await?;
    let list = across(&app, &session, "").await?;

    // Assert
    expect_that!(titles(&list, "ranked"), elements_are![eq("High"), eq("Low")]);
    Ok(())
}

/// A team parameter `Tier` with options Gold and Silver, used by `first` only, and a team tag `Ops`; items in both
/// heaps of the team. Returns the pieces the filter tests name.
struct TeamFilterWorld {
    app: TestApp,
    session: String,
    team: String,
    first: String,
    second: String,
    tag: String,
    tier: String,
    gold: String,
}

async fn team_filter_world() -> Result<TeamFilterWorld> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team, first) = app.shared_heap(&session).await?;
    let team_url = app.team_url(&team).await?;
    let second = make_heap(&app, &session, &team, "Second").await?;
    let tag = app
        .send(
            Method::POST,
            &format!("{team_url}/tags"),
            Some(&session),
            Some(json!({ "name": "Ops" })),
        )
        .await?;
    anyhow::ensure!(tag.status == StatusCode::CREATED, "team tag failed: {}", tag.body);
    let parameter = app
        .send(
            Method::POST,
            &format!("{team_url}/parameters"),
            Some(&session),
            Some(json!({ "name": "Tier", "options": ["Gold", "Silver"] })),
        )
        .await?;
    anyhow::ensure!(
        parameter.status == StatusCode::CREATED,
        "team parameter failed: {}",
        parameter.body
    );
    let used = app
        .send(
            Method::POST,
            &format!(
                "{}/parameters/{}/use",
                app.heap_url(&first).await?,
                parameter.body["id"].as_str().context("parameter id")?
            ),
            Some(&session),
            None,
        )
        .await?;
    anyhow::ensure!(used.status == StatusCode::OK, "use failed: {}", used.body);
    let option = |label: &str| {
        parameter.body["options"]
            .as_array()
            .and_then(|options| options.iter().find(|o| o["label"] == label))
            .and_then(|o| o["id"].as_str())
            .map(str::to_owned)
            .context("option")
    };
    let (gold, silver) = (option("Gold")?, option("Silver")?);
    let parameter_id = parameter.body["id"].as_str().context("parameter id")?;
    let tag_id = tag.body["id"].as_str().context("tag id")?.to_owned();
    for (heap, title, need, tagged, value) in [
        (&first, "A gold", "critical", true, Some(&gold)),
        (&first, "A silver", "expected", false, Some(&silver)),
        (&first, "A gold tagged", "idea", true, Some(&gold)),
        (&second, "B tagged", "expected", true, None),
        (&second, "B plain", "critical", false, None),
    ] {
        let mut body = json!({ "title": title, "need": need });
        if tagged {
            body["tag_ids"] = json!([tag_id]);
        }
        if let Some(option) = value {
            body["parameter_values"] = json!({ parameter_id: option });
        }
        app.new_item(&session, heap, body).await?;
    }
    let tier = parameter_id.to_owned();
    Ok(TeamFilterWorld {
        app,
        session,
        team,
        first,
        second,
        tag: tag_id,
        tier,
        gold,
    })
}

#[gtest]
#[tokio::test]
async fn a_team_parameters_option_keeps_the_items_of_the_heaps_that_use_it_in_the_unfiltered_order() -> Result<()> {
    // Arrange
    let w = team_filter_world().await?;

    // Act
    let all = across(&w.app, &w.session, "?team=shared").await?;
    let filtered = across(&w.app, &w.session, &format!("?team=shared&param={}", w.gold)).await?;

    // Assert
    let in_all: Vec<String> = titles(&all, "ranked")
        .into_iter()
        .filter(|title| title.starts_with("A gold"))
        .collect();
    expect_that!(in_all.len(), eq(2));
    expect_that!(titles(&filtered, "ranked"), eq(&in_all));
    expect_that!(filtered["count"], eq(&json!(2)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_team_tag_keeps_the_tagged_items_of_every_heap_in_the_unfiltered_order() -> Result<()> {
    // Arrange
    let w = team_filter_world().await?;

    // Act
    let all = across(&w.app, &w.session, "?team=shared").await?;
    let filtered = across(&w.app, &w.session, &format!("?team=shared&tag={}", w.tag)).await?;

    // Assert
    let in_all: Vec<String> = titles(&all, "ranked")
        .into_iter()
        .filter(|title| title.contains("tagged") || title == "A gold")
        .collect();
    expect_that!(in_all.len(), eq(3));
    expect_that!(titles(&filtered, "ranked"), eq(&in_all));
    expect_that!(filtered["count"], eq(&json!(3)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_tag_or_option_that_isnt_the_teams_is_refused_with_a_team() -> Result<()> {
    // Arrange
    let w = team_filter_world().await?;
    let heap_tag = w.app.new_tag_id(&w.session, &w.first, "Local").await?;
    let heap_parameter = w
        .app
        .send(
            Method::POST,
            &format!("{}/parameters", w.app.heap_url(&w.first).await?),
            Some(&w.session),
            Some(json!({ "name": "Own", "options": ["One"] })),
        )
        .await?;
    anyhow::ensure!(
        heap_parameter.status == StatusCode::CREATED,
        "heap parameter failed: {}",
        heap_parameter.body
    );
    let heap_option = heap_parameter.body["options"][0]["id"].as_str().context("option")?;

    for (query, code, field) in [
        (format!("?team=shared&tag={heap_tag}"), "tag_not_in_team", Some("tag")),
        (
            "?team=shared&tag=00000000-0000-0000-0000-000000000000".to_owned(),
            "tag_not_in_team",
            Some("tag"),
        ),
        (
            format!("?team=shared&param={heap_option}"),
            "option_not_in_team",
            Some("param"),
        ),
        (format!("?param={}", w.gold), "invalid_query", None),
        (format!("?tag={}", w.tag), "invalid_query", None),
    ] {
        // Act
        let response = w
            .app
            .send(Method::GET, &format!("/api/v1/me/items{query}"), Some(&w.session), None)
            .await?;

        // Assert
        expect_that!(response.body["code"], eq(&json!(code)), "{query}");
        if let Some(field) = field {
            expect_that!(response.body["field"], eq(&json!(field)), "{query}");
        }
    }
    Ok(())
}

#[gtest]
#[tokio::test]
async fn an_archived_team_parameters_option_is_accepted_and_filters_nothing() -> Result<()> {
    // Arrange
    let w = team_filter_world().await?;
    let team_url = w.app.team_url(&w.team).await?;
    let parameter = w
        .app
        .send(
            Method::POST,
            &format!("{team_url}/parameters"),
            Some(&w.session),
            Some(json!({ "name": "Size", "options": ["Big"] })),
        )
        .await?;
    anyhow::ensure!(
        parameter.status == StatusCode::CREATED,
        "team parameter failed: {}",
        parameter.body
    );
    let id = parameter.body["id"].as_str().context("parameter id")?;
    let big = parameter.body["options"][0]["id"].as_str().context("option")?;
    let archived = w
        .app
        .send(
            Method::POST,
            &format!("{team_url}/parameters/{id}/archive"),
            Some(&w.session),
            None,
        )
        .await?;
    anyhow::ensure!(archived.status == StatusCode::OK, "archive failed: {}", archived.body);

    // Act
    let all = across(&w.app, &w.session, "?team=shared").await?;
    let filtered = across(&w.app, &w.session, &format!("?team=shared&param={big}")).await?;

    // Assert
    expect_that!(titles(&filtered, "ranked"), eq(&titles(&all, "ranked")));
    expect_that!(filtered["count"], eq(&all["count"]));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_heap_that_stopped_using_a_team_parameter_has_no_rows_under_its_option() -> Result<()> {
    // Arrange: a stopped use reads as an archived parameter in that heap, whose options its own list ignores.
    let w = team_filter_world().await?;
    let second_url = w.app.heap_url(&w.second).await?;
    for action in ["use", "stop-using"] {
        let response = w
            .app
            .send(
                Method::POST,
                &format!("{second_url}/parameters/{}/{action}", w.tier),
                Some(&w.session),
                None,
            )
            .await?;
        anyhow::ensure!(response.status == StatusCode::OK, "{action} failed: {}", response.body);
    }

    // Act
    let filtered = across(&w.app, &w.session, &format!("?team=shared&param={}", w.gold)).await?;

    // Assert
    expect_that!(
        titles(&filtered, "ranked"),
        elements_are![eq("A gold"), eq("A gold tagged")]
    );
    Ok(())
}
