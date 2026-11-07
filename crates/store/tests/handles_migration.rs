//! The handles migration on a database that already has users, teams, heaps and items.

use anyhow::{Context, Result};
use domain::handle::Handle;
use domain::validation::Field;
use domain::{HeapId, TeamId, UserId};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection};
use uuid::Uuid;

/// Migrations before the handles one.
const BEFORE_HANDLES: u32 = 8;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

async fn insert_user(db: &DatabaseConnection, n: u128, name: &str, created_at: i64) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO users (id, email, display_name, created_at, version) \
             VALUES ('{}', 'u{n}@example.test', '{name}', {created_at}, 1)",
            id(n)
        ),
    )
    .await
}

async fn insert_team(db: &DatabaseConnection, n: u128, name: &str, created_at: i64, members: &[u128]) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO teams (id, name, created_at, version) VALUES ('{}', '{name}', {created_at}, 1)",
            id(n)
        ),
    )
    .await?;
    for member in members {
        run(
            db,
            &format!(
                "INSERT INTO team_members (team_id, user_id, role, joined_at) VALUES ('{}', '{}', 'admin', {created_at})",
                id(n),
                id(*member)
            ),
        )
        .await?;
    }
    Ok(())
}

async fn insert_heap(db: &DatabaseConnection, n: u128, team: u128, name: &str, created_at: i64) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO heaps (id, team_id, name, age_weight, deadline_weight, value_weight, created_at, version) \
             VALUES ('{}', '{}', '{name}', 10, 10, 10, {created_at}, 1)",
            id(n),
            id(team)
        ),
    )
    .await
}

async fn insert_item(db: &DatabaseConnection, n: u128, heap: u128) -> Result<()> {
    run(
        db,
        &format!(
            "INSERT INTO items (id, heap_id, title, state, created_at, version) VALUES ('{}', '{}', 'Item', 'open', 0, 1)",
            id(n),
            id(heap)
        ),
    )
    .await
}

fn handle(raw: &str) -> Result<Handle> {
    Ok(Handle::parse(raw, Field::Slug)?)
}

#[gtest]
#[tokio::test]
async fn existing_rows_get_handles_and_every_user_a_personal_team() -> Result<()> {
    // Arrange: Ada and her untouched personal team; Bob, whose personal team was shared with Ada; and a shared team
    // whose name clashes with Ada's display name.
    let db = store::testing::database_before_migration(BEFORE_HANDLES).await?;
    insert_user(&db, 1, "Ada", 100).await?;
    insert_team(&db, 11, "Ada", 100, &[1]).await?;
    insert_user(&db, 2, "Bob", 200).await?;
    insert_team(&db, 12, "Bob", 200, &[2, 1]).await?;
    insert_team(&db, 13, "Ada", 300, &[1]).await?;
    insert_heap(&db, 21, 11, "Ada", 100).await?;
    insert_heap(&db, 22, 11, "Ada!", 150).await?;
    insert_heap(&db, 23, 12, "日本", 200).await?;
    for n in 0..40 {
        insert_item(&db, 100 + n, 21).await?;
    }

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let ada = store::users::find_user(&db, UserId::from_uuid(Uuid::from_u128(1)))
        .await?
        .context("Ada")?;
    let bob = store::users::find_user(&db, UserId::from_uuid(Uuid::from_u128(2)))
        .await?
        .context("Bob")?;
    expect_that!(ada.username.as_str(), eq("ada"));
    expect_that!(bob.username.as_str(), eq("bob"));

    let ada_team = store::teams::find_team(&db, TeamId::from_uuid(Uuid::from_u128(11)))
        .await?
        .context("Ada's team")?;
    expect_that!(ada_team.slug.as_str(), eq("ada"));
    expect_that!(ada_team.personal_user, some(eq(ada.id)));

    let shared = store::teams::find_team(&db, TeamId::from_uuid(Uuid::from_u128(12)))
        .await?
        .context("Bob's shared team")?;
    expect_that!(shared.personal_user, none());
    let bob_team = store::teams::find_team_by_slug(&db, &handle("bob")?)
        .await?
        .context("Bob's new team")?;
    expect_that!(bob_team.personal_user, some(eq(bob.id)));
    expect_that!(
        store::teams::list_memberships(&db, bob_team.id)
            .await?
            .iter()
            .map(|m| m.user_id)
            .collect::<Vec<_>>(),
        elements_are![eq(&bob.id)]
    );
    let other_ada = store::teams::find_team(&db, TeamId::from_uuid(Uuid::from_u128(13)))
        .await?
        .context("the other Ada team")?;
    expect_that!(other_ada.slug.as_str(), eq("ada-2"));

    let slug = |n: u128| {
        let db = &db;
        async move {
            let heap = store::heaps::find_heap(db, HeapId::from_uuid(Uuid::from_u128(n))).await?;
            Ok::<_, anyhow::Error>(heap.context("heap")?.slug.as_str().to_owned())
        }
    };
    expect_that!(slug(21).await?, eq("ada"));
    expect_that!(slug(22).await?, eq("ada-2"));
    expect_that!(slug(23).await?, eq("heap"));

    let items = store::items::list_items_in_states(
        &db,
        HeapId::from_uuid(Uuid::from_u128(21)),
        &[domain::item::StateKind::Open],
    )
    .await?;
    let keys: std::collections::BTreeSet<_> = items.iter().map(|item| item.key).collect();
    expect_that!(keys.len(), eq(40));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_user_who_left_their_own_team_gets_a_new_one_and_long_names_are_cut_to_fit_a_suffix() -> Result<()> {
    // Arrange: Cy left the team made at registration, which is now empty. Two users have the same 45-character name.
    let db = store::testing::database_before_migration(BEFORE_HANDLES).await?;
    let long = "Wilhelmina Featherstonehaugh-Cholmondeley Jr.";
    insert_user(&db, 1, "Cy", 100).await?;
    insert_team(&db, 11, "Cy", 100, &[]).await?;
    insert_user(&db, 2, long, 200).await?;
    insert_team(&db, 12, long, 200, &[2]).await?;
    insert_user(&db, 3, long, 300).await?;
    insert_team(&db, 13, long, 300, &[3]).await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let cy = store::users::find_user(&db, UserId::from_uuid(Uuid::from_u128(1)))
        .await?
        .context("Cy")?;
    expect_that!(cy.username.as_str(), eq("cy"));
    let cy_team = store::teams::find_team_by_slug(&db, &handle("cy")?)
        .await?
        .context("Cy's new team")?;
    expect_that!(cy_team.id, not(eq(TeamId::from_uuid(Uuid::from_u128(11)))));
    expect_that!(cy_team.personal_user, some(eq(cy.id)));
    let left = store::teams::find_team(&db, TeamId::from_uuid(Uuid::from_u128(11)))
        .await?
        .context("the team Cy left")?;
    expect_that!(left.personal_user, none());
    expect_that!(left.slug.as_str(), eq("cy-2"));

    let username = |n: u128| {
        let db = &db;
        async move {
            let user = store::users::find_user(db, UserId::from_uuid(Uuid::from_u128(n))).await?;
            Ok::<_, anyhow::Error>(user.context("user")?.username.as_str().to_owned())
        }
    };
    let first = username(2).await?;
    let second = username(3).await?;
    expect_that!(first, eq("wilhelmina-featherstonehaugh-cholmondel"));
    expect_that!(second, eq("wilhelmina-featherstonehaugh-cholmond-2"));
    expect_that!(second.len(), eq(Handle::MAX_CHARS));
    Ok(())
}
