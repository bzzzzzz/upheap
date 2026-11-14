//! The Data page's figures: what a team's heaps spent, added and closed by day, for all of them and for one, and
//! which heap the figures are asked for.

mod common;

use anyhow::{Context, Result};
use axum::http::{Method, StatusCode};
use common::TestApp;
use domain::Duration;
use googletest::prelude::*;
use serde_json::{Value, json};

/// A team with two heaps and a member who owns it.
struct Team {
    app: TestApp,
    session: String,
    team_url: String,
    /// The heaps' IDs, then their `team/heap` references.
    heaps: [(String, String); 2],
}

async fn team() -> Result<Team> {
    let app = TestApp::start().await?;
    let (session, _) = app.register("ada@example.test").await?;
    let (team_id, first) = app.shared_heap(&session).await?;
    let team_url = app.team_url(&team_id).await?;
    let created = app
        .send(
            Method::POST,
            &format!("{team_url}/heaps"),
            Some(&session),
            Some(json!({ "name": "Second" })),
        )
        .await?;
    anyhow::ensure!(
        created.status == StatusCode::CREATED,
        "create heap failed: {}",
        created.body
    );
    let second = created.body["id"].as_str().context("no heap id")?.to_owned();
    let mut heaps = Vec::new();
    for id in [first, second] {
        let reference = app
            .heap_url(&id)
            .await?
            .strip_prefix("/api/v1/teams/")
            .context("heap url")?
            .replace("/heaps/", "/");
        heaps.push((id, reference));
    }
    let heaps = heaps.try_into().map_err(|_| anyhow::anyhow!("two heaps"))?;
    Ok(Team {
        app,
        session,
        team_url,
        heaps,
    })
}

impl Team {
    async fn add(&self, heap: usize, title: &str) -> Result<Value> {
        self.app
            .new_item(
                &self.session,
                &self.heaps[heap].0,
                json!({ "title": title, "description": "Some context" }),
            )
            .await
    }

    /// Sends the item to an agent, and has a run on it reach its goal or not, then finish with `finish`.
    async fn run(&self, heap: usize, item: &Value, reached: bool, finish: Value) -> Result<()> {
        let item_url = self.app.item_url(item["id"].as_str().context("no id")?).await?;
        let sent = self
            .app
            .send(
                Method::POST,
                &format!("{item_url}/turn"),
                Some(&self.session),
                Some(json!({ "reach": "one" })),
            )
            .await?;
        anyhow::ensure!(sent.status == StatusCode::CREATED, "send failed: {}", sent.body);
        let claimed = self
            .app
            .send(
                Method::POST,
                "/api/v1/work/runs",
                Some(&self.session),
                Some(json!({ "heaps": [self.heaps[heap].1] })),
            )
            .await?;
        anyhow::ensure!(claimed.status == StatusCode::CREATED, "claim failed: {}", claimed.body);
        let run = claimed.body["run"]["id"].as_str().context("no run")?;
        if reached {
            let said = self
                .app
                .send(
                    Method::POST,
                    &format!("/api/v1/work/runs/{run}/reached"),
                    Some(&self.session),
                    Some(json!({ "result": "Added the context" })),
                )
                .await?;
            anyhow::ensure!(said.status == StatusCode::OK, "reached failed: {}", said.body);
        }
        let finished = self
            .app
            .send(
                Method::POST,
                &format!("/api/v1/work/runs/{run}/finish"),
                Some(&self.session),
                Some(finish),
            )
            .await?;
        anyhow::ensure!(finished.status == StatusCode::OK, "finish failed: {}", finished.body);
        Ok(())
    }

    async fn close(&self, item: &Value, state: &str) -> Result<()> {
        let url = self.app.item_url(item["id"].as_str().context("no id")?).await?;
        let closed = self
            .app
            .patch(&self.session, &url, &item["version"], json!({ "state": state }))
            .await?;
        anyhow::ensure!(closed.status == StatusCode::OK, "close failed: {}", closed.body);
        Ok(())
    }

    async fn data(&self, query: &str) -> Result<common::TestResponse> {
        self.app
            .send(
                Method::GET,
                &format!("{}/data{query}", self.team_url),
                Some(&self.session),
                None,
            )
            .await
    }
}

/// The figures of a bucket as `[spend, without cost, added, done, wont_do, became, reached, asked, failed, other]`.
fn figures(bucket: &Value) -> Vec<Value> {
    vec![
        bucket["spend_usd"].clone(),
        bucket["runs_without_cost"].clone(),
        bucket["added"].clone(),
        bucket["closed"]["done"].clone(),
        bucket["closed"]["wont_do"].clone(),
        bucket["closed"]["became"].clone(),
        bucket["runs"]["reached"].clone(),
        bucket["runs"]["asked"].clone(),
        bucket["runs"]["failed"].clone(),
        bucket["runs"]["other"].clone(),
    ]
}

fn as_numbers(values: Vec<Value>) -> Vec<f64> {
    values.iter().filter_map(Value::as_f64).collect()
}

#[gtest]
#[tokio::test]
async fn a_team_reads_what_happened_by_day_for_all_heaps_and_for_one() -> Result<()> {
    // Arrange: one item in each heap on the first day; on the second, one more in the first, a run that reached with
    // a cost and closed its item done, a won't do, and a failed run in the second heap that reported no cost.
    let t = team().await?;
    let reached = t.add(0, "Rotate keys").await?;
    t.add(1, "Water plants").await?;
    t.app.clock.advance(Duration::from_days(1));
    let wont_do = t.add(0, "Paint the shed").await?;
    t.run(0, &reached, true, json!({ "cost_usd": 0.5 })).await?;
    let reached = t
        .app
        .send(
            Method::GET,
            &t.app.item_url(reached["id"].as_str().context("id")?).await?,
            Some(&t.session),
            None,
        )
        .await?
        .body;
    t.close(&reached, "done").await?;
    t.close(&wont_do, "wont_do").await?;
    let failing = t.add(1, "Fix the leak").await?;
    t.run(1, &failing, false, json!({ "failure": "The tool crashed" }))
        .await?;

    // Act
    let all = t.data("?days=7").await?;
    let first = t.data(&format!("?days=7&heap={}", heap_slug(&t, 0).await?)).await?;
    let second = t.data(&format!("?days=7&heap={}", heap_slug(&t, 1).await?)).await?;
    let unknown = t.data("?heap=no-such-heap").await?;
    let bad_days = t.data("?days=8").await?;

    // Assert
    expect_that!(all.status, eq(StatusCode::OK));
    let buckets = all.body["buckets"].as_array().context("buckets")?;
    expect_that!(buckets.len(), eq(7));
    expect_that!(buckets[5]["label"], eq(&json!("1 Jan")));
    expect_that!(buckets[6]["label"], eq(&json!("2 Jan")));
    expect_that!(buckets[5]["partial"], eq(false));
    expect_that!(buckets[6]["partial"], eq(true));
    expect_that!(as_numbers(figures(&buckets[0])), eq(&vec![0.0; 10]));
    expect_that!(
        as_numbers(figures(&buckets[5])),
        eq(&[0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0].to_vec())
    );
    expect_that!(
        as_numbers(figures(&buckets[6])),
        eq(&[0.5, 1.0, 2.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0].to_vec())
    );
    expect_that!(
        as_numbers(figures(&all.body["totals"])),
        eq(&[0.5, 1.0, 4.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0].to_vec())
    );

    expect_that!(
        as_numbers(figures(&first.body["totals"])),
        eq(&[0.5, 0.0, 2.0, 1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 0.0].to_vec())
    );
    expect_that!(
        as_numbers(figures(&second.body["totals"])),
        eq(&[0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0].to_vec())
    );

    expect_that!(unknown.status, eq(StatusCode::NOT_FOUND));
    expect_that!(bad_days.status, eq(StatusCode::BAD_REQUEST));
    expect_that!(bad_days.body["code"], eq(&json!("invalid_query")));
    Ok(())
}

async fn heap_slug(t: &Team, heap: usize) -> Result<String> {
    let url = t.app.heap_url(&t.heaps[heap].0).await?;
    Ok(url.rsplit('/').next().context("slug")?.to_owned())
}
