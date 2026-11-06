//! Forward-only schema migrations, embedded in the binary and run on startup. Never edit a released migration;
//! add a new one.

use sea_orm_migration::prelude::*;

mod m20261105_000001_initial;
mod m20261106_000002_tags;
mod m20261107_000003_blockers;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261105_000001_initial::Migration),
            Box::new(m20261106_000002_tags::Migration),
            Box::new(m20261107_000003_blockers::Migration),
        ]
    }
}
