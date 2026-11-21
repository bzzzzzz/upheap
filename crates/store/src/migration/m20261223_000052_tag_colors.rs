//! A colour for every tag (`tags.color`, a code from the palette). The column is `NOT NULL`; it has a default, which
//! only lets `ADD COLUMN` work on rows that already exist, since SQLite can't add a `NOT NULL` column without one and
//! can't drop the default after. Every existing row is then given a colour, merged heap tags too (never shown), but
//! a merged tag is not counted: the server's pick skips them.
//!
//! The order: for each team, its tags in `(created_at, id)` order each take the least used colour among the team's
//! tags given one so far, the first in palette order on a tie. Then for each heap, its tags in the same order count the
//! team's tags plus its own given so far. That is how the server picks a colour for a new tag, applied in the order
//! the tags were made. The palette is this migration's own copy: a migration is frozen, and the application's palette
//! may change later.

use std::collections::BTreeMap;

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::ConnectionTrait;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The palette as it was when this migration was written, in the order a colour is picked in.
const PALETTE: [&str; 8] = ["blue", "sky", "teal", "green", "lime", "rose", "pink", "grey"];

const COLOR_LEN: u32 = 16;

#[derive(DeriveIden)]
enum Tags {
    Table,
    Id,
    HeapId,
    TeamId,
    CreatedAt,
    Color,
    MergedInto,
}

#[derive(DeriveIden)]
enum Heaps {
    Table,
    Id,
    TeamId,
}

/// How many tags have each colour, in palette order.
type Counts = [u32; PALETTE.len()];

/// Takes the least used colour: counts it, and returns its code.
fn take_least_used(counts: &mut Counts) -> &'static str {
    let mut least = 0;
    for (index, count) in counts.iter().enumerate() {
        if *count < counts[least] {
            least = index;
        }
    }
    counts[least] += 1;
    PALETTE[least]
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Tags::Table)
                    .add_column(
                        ColumnDef::new(Tags::Color)
                            .string_len(COLOR_LEN)
                            .not_null()
                            .default(PALETTE[PALETTE.len() - 1]),
                    )
                    .to_owned(),
            )
            .await?;
        backfill(manager.get_connection()).await
    }
}

async fn backfill(db: &impl ConnectionTrait) -> Result<(), DbErr> {
    let heap_teams: BTreeMap<String, String> = db
        .query_all(
            &Query::select()
                .columns([Heaps::Id, Heaps::TeamId])
                .from(Heaps::Table)
                .to_owned(),
        )
        .await?
        .into_iter()
        .map(|row| Ok((row.try_get("", "id")?, row.try_get("", "team_id")?)))
        .collect::<Result<_, DbErr>>()?;
    let rows = db
        .query_all(
            &Query::select()
                .columns([Tags::Id, Tags::HeapId, Tags::TeamId, Tags::MergedInto])
                .from(Tags::Table)
                .order_by(Tags::CreatedAt, Order::Asc)
                .order_by(Tags::Id, Order::Asc)
                .to_owned(),
        )
        .await?;

    // Each tag's id and whether it is merged, which is given a colour but doesn't count toward a heap's.
    let mut team_tags: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut heap_tags: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    for row in rows {
        let id: String = row.try_get("", "id")?;
        let heap: Option<String> = row.try_get("", "heap_id")?;
        let team: Option<String> = row.try_get("", "team_id")?;
        let merged: Option<String> = row.try_get("", "merged_into")?;
        match (heap, team) {
            (Some(heap), None) => heap_tags.entry(heap).or_default().push((id, merged.is_some())),
            (None, Some(team)) => team_tags.entry(team).or_default().push(id),
            _ => {
                return Err(DbErr::Migration(format!(
                    "tag {id} must have exactly one of heap_id and team_id"
                )));
            }
        }
    }

    let mut team_counts: BTreeMap<String, Counts> = BTreeMap::new();
    for (team, ids) in team_tags {
        let mut counts = Counts::default();
        for id in ids {
            set_color(db, &id, take_least_used(&mut counts)).await?;
        }
        team_counts.insert(team, counts);
    }
    for (heap, ids) in heap_tags {
        let team = heap_teams
            .get(&heap)
            .ok_or_else(|| DbErr::Migration(format!("a tag names the missing heap {heap}")))?;
        let mut counts = team_counts.get(team).copied().unwrap_or_default();
        for (id, merged) in ids {
            if merged {
                let mut uncounted = counts;
                set_color(db, &id, take_least_used(&mut uncounted)).await?;
            } else {
                set_color(db, &id, take_least_used(&mut counts)).await?;
            }
        }
    }
    Ok(())
}

async fn set_color(db: &impl ConnectionTrait, id: &str, color: &str) -> Result<(), DbErr> {
    db.execute(
        &Query::update()
            .table(Tags::Table)
            .value(Tags::Color, color)
            .and_where(Expr::col(Tags::Id).eq(id))
            .to_owned(),
    )
    .await?;
    Ok(())
}
