//! The former handles migration on a database that already has moved items: each move leaves the handle the item
//! left, found from the audit log.

use anyhow::{Context, Result};
use domain::handle::ItemKey;
use domain::{HeapId, ItemId};
use googletest::prelude::*;
use store::{ConnectionTrait, DatabaseConnection, DbBackend};
use uuid::Uuid;

/// Migrations before the former handles one.
const BEFORE_FORMER_HANDLES: u32 = 44;

async fn run(db: &DatabaseConnection, sql: &str) -> Result<()> {
    db.execute_unprepared(sql).await.with_context(|| sql.to_owned())?;
    Ok(())
}

fn id(n: u128) -> String {
    Uuid::from_u128(n).hyphenated().to_string()
}

async fn insert_moved(
    db: &DatabaseConnection,
    n: u128,
    from: u128,
    to: u128,
    key: Option<(&str, &str)>,
    at: i64,
) -> Result<()> {
    let key_diff = key.map_or(String::new(), |(before, after)| {
        format!(r#", "key": {{"before": "{before}", "after": "{after}"}}"#)
    });
    run(
        db,
        &format!(
            "INSERT INTO audit_log (id, team_id, heap_id, actor_user_id, action, entity_type, entity_id, diff, created_at) \
             VALUES ('{}', '{}', '{}', '{}', 'item_moved', 'item', '{}', '{{\"heap\": {{\"before\": \"{}\", \"after\": \"{}\"}}{key_diff}}}', {at})",
            id(1000 + u128::try_from(at)?),
            id(1),
            id(to),
            id(5),
            id(n),
            id(from),
            id(to),
        ),
    )
    .await
}

#[gtest]
#[tokio::test]
async fn earlier_moves_leave_their_handles_from_the_audit_log() -> Result<()> {
    // Arrange: item 10 was k3x9a in heap 2, moved to heap 3 keeping the key, then to heap 4 as m4n5p.
    let db = store::testing::database_before_migration(BEFORE_FORMER_HANDLES).await?;
    let column = match db.get_database_backend() {
        DbBackend::MySql => "`key`",
        _ => "\"key\"",
    };
    run(
        &db,
        &format!(
            "INSERT INTO users (id, email, display_name, created_at, version) VALUES ('{}', 'a@example.test', 'Ada', 0, 1)",
            id(5)
        ),
    )
    .await?;
    run(
        &db,
        &format!(
            "INSERT INTO teams (id, slug, name, created_at, version) VALUES ('{}', 'infra', 'Infra', 0, 1)",
            id(1)
        ),
    )
    .await?;
    for (n, slug) in [(2, "one"), (3, "two"), (4, "three")] {
        run(
            &db,
            &format!(
                "INSERT INTO heaps (id, team_id, slug, name, age_weight, deadline_weight, value_weight, created_at, version) \
                 VALUES ('{}', '{}', '{slug}', '{slug}', 10, 10, 10, 0, 1)",
                id(n),
                id(1)
            ),
        )
        .await?;
    }
    run(
        &db,
        &format!(
            "INSERT INTO items (id, heap_id, {column}, title, state, need, created_at, version) \
             VALUES ('{}', '{}', 'm4n5p', 'Item', 'open', 'expected', 0, 3)",
            id(10),
            id(4)
        ),
    )
    .await?;
    insert_moved(&db, 10, 2, 3, None, 100).await?;
    insert_moved(&db, 10, 3, 4, Some(("k3x9a", "m4n5p")), 200).await?;

    // Act
    store::run_migrations(&db).await?;

    // Assert
    let find = |heap: u128, key: &str| {
        store::items::find_item_id_by_former_key(
            &db,
            HeapId::from_uuid(Uuid::from_u128(heap)),
            ItemKey::parse(key).expect("a valid key"),
        )
    };
    let item = Some(ItemId::from_uuid(Uuid::from_u128(10)));
    expect_that!(find(2, "k3x9a").await?, eq(item));
    expect_that!(find(3, "k3x9a").await?, eq(item));
    expect_that!(find(4, "m4n5p").await?, none());
    expect_that!(find(2, "m4n5p").await?, none());
    Ok(())
}
