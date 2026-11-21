//! The team parameters migration on a database that already has parameters: each stays a heap parameter with its ID,
//! options and values, a team can then own parameters and its heaps use them, and a row that doesn't have exactly
//! one owner is refused when it is read.

use anyhow::{Context, Result};
use domain::parameter::{NewParameter, OptionLabel, Parameter, ParameterName, ParameterOption, ParameterOwner, Weight};
use domain::{HeapId, OptionId, ParameterId, TeamId, Timestamp};
use googletest::prelude::*;
use store::parameters::HeapUse;
use store::{ConnectionTrait, DatabaseConnection, Statement, StoreError};
use uuid::Uuid;

/// Migrations before the team parameters one.
const BEFORE_TEAM_PARAMETERS: u32 = 49;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

fn at(millis: i64) -> Result<Timestamp> {
    Timestamp::from_millis(millis).context("in range")
}

/// Team 1 with heap 2, the heap's parameter 3 with options 4 and 5, and item 6 with a value for it.
async fn old_database() -> Result<DatabaseConnection> {
    let db = store::testing::database_before_migration(BEFORE_TEAM_PARAMETERS).await?;
    run(
        &db,
        &format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, created_at, version) \
             VALUES ('{}', '{}', 'backlog', 'Backlog', 10, 10, 10, 0, 1)",
            id(2),
            id(1)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO parameters (id, heap_id, name, weight, created_at, version) \
             VALUES ('{}', '{}', 'Customer', 40, 0, 3)",
            id(3),
            id(2)
        ),
    )
    .await?;
    for (option, label, position) in [(4, "Enterprise", 0), (5, "Free", 1)] {
        run(
            &db,
            &format!(
                "INSERT INTO parameter_options (id, parameter_id, label, position) VALUES ('{}', '{}', '{label}', {position})",
                id(option),
                id(3)
            ),
        )
        .await?;
    }
    let key = if db.get_database_backend() == store::DbBackend::MySql {
        "`key`"
    } else {
        "\"key\""
    };
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, {key}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', 'k3x9a', 'Item', 'open', 'expected', 0, 1)",
            id(6),
            id(2)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO item_parameter_values (item_id, parameter_id, option_id) VALUES ('{}', '{}', '{}')",
            id(6),
            id(3),
            id(4)
        ),
    )
    .await?;
    Ok(db)
}

fn team_parameter(n: u128, name: &str) -> Result<Parameter> {
    Ok(Parameter::new(
        ParameterId::from_uuid(Uuid::from_u128(n)),
        ParameterOwner::Team(TeamId::from_uuid(Uuid::from_u128(1))),
        NewParameter {
            name: ParameterName::parse(name)?,
            weight: Weight::OFF,
            options: vec![ParameterOption {
                id: OptionId::from_uuid(Uuid::from_u128(n + 100)),
                label: OptionLabel::parse("High")?,
            }],
            tags: Default::default(),
        },
        at(0)?,
    )?)
}

#[gtest]
#[tokio::test]
async fn existing_parameters_stay_heap_parameters_and_a_team_can_have_its_own() -> Result<()> {
    // Arrange
    let db = old_database().await?;
    let heap = HeapId::from_uuid(Uuid::from_u128(2));

    // Act
    store::run_migrations(&db).await?;

    // Assert: the parameter reads back as the heap's, with its options, and the item still has its value.
    let parameter = store::parameters::find_parameter(&db, ParameterId::from_uuid(Uuid::from_u128(3)))
        .await?
        .context("parameter 3")?;
    expect_that!(parameter.owner, eq(ParameterOwner::Heap(heap)));
    expect_that!(parameter.name.as_str(), eq("Customer"));
    expect_that!(parameter.weight.get(), eq(40));
    expect_that!(parameter.version.get(), eq(3));
    expect_that!(
        parameter.options.iter().map(|o| o.label.as_str()).collect::<Vec<_>>(),
        elements_are![eq(&"Enterprise"), eq(&"Free")]
    );
    let backend = db.get_database_backend();
    let row = db
        .query_one_raw(Statement::from_string(
            backend,
            format!(
                "SELECT option_id FROM item_parameter_values WHERE item_id = '{}'",
                id(6)
            ),
        ))
        .await?
        .context("value row")?;
    expect_that!(row.try_get_by_index::<String>(0)?, eq(&id(4)));

    // And a team parameter can be inserted and read, and a heap sees it once it uses it, with its own weight.
    let team_parameter = team_parameter(7, "Tier")?;
    store::parameters::insert_parameter(&db, &team_parameter).await?;
    expect_that!(
        store::parameters::list_team_parameters(&db, TeamId::from_uuid(Uuid::from_u128(1))).await?,
        elements_are![eq(&team_parameter)]
    );
    expect_that!(
        store::parameters::list_parameters(&db, heap)
            .await?
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>(),
        elements_are![eq(&parameter.id)]
    );
    let used = HeapUse {
        heap_id: heap,
        parameter_id: team_parameter.id,
        weight: Weight::parse(25)?,
        stopped_at: None,
    };
    store::parameters::ensure_heap_use(&db, heap, team_parameter.id, at(1)?).await?;
    store::parameters::update_heap_use(&db, &used).await?;
    let seen = store::parameters::list_parameters(&db, heap).await?;
    expect_that!(
        seen.iter().map(|p| p.id).collect::<Vec<_>>(),
        elements_are![eq(&parameter.id), eq(&team_parameter.id)]
    );
    expect_that!(seen[1].weight.get(), eq(25));
    expect_that!(seen[1].is_archived(), is_false());
    let stopped = HeapUse {
        stopped_at: Some(at(2)?),
        ..used
    };
    store::parameters::update_heap_use(&db, &stopped).await?;
    let seen = store::parameters::list_parameters(&db, heap).await?;
    expect_that!(seen[1].archived_at, some(eq(at(2)?)));
    Ok(())
}

#[gtest]
#[tokio::test]
async fn a_parameter_with_both_owners_or_neither_is_a_corrupt_row() -> Result<()> {
    // Arrange
    let db = old_database().await?;
    store::run_migrations(&db).await?;
    for (n, heap, team) in [(8, Some(id(2)), Some(id(1))), (9, None, None)] {
        let value = |owner: Option<String>| owner.map_or("NULL".to_owned(), |owner| format!("'{owner}'"));
        run(
            &db,
            &format!(
                "INSERT INTO parameters (id, heap_id, team_id, name, weight, created_at, version) \
                 VALUES ('{}', {}, {}, 'Broken {n}', 0, 0, 1)",
                id(n),
                value(heap),
                value(team)
            ),
        )
        .await?;
    }

    // Act
    let both = store::parameters::find_parameter(&db, ParameterId::from_uuid(Uuid::from_u128(8))).await;
    let neither = store::parameters::find_parameter(&db, ParameterId::from_uuid(Uuid::from_u128(9))).await;

    // Assert
    expect_that!(both, err(matches_pattern!(StoreError::CorruptRow { .. })));
    expect_that!(neither, err(matches_pattern!(StoreError::CorruptRow { .. })));
    Ok(())
}
