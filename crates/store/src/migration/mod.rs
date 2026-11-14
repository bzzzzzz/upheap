//! Forward-only schema migrations, embedded in the binary and run on startup. Never edit a released migration;
//! add a new one.

use sea_orm_migration::prelude::*;

mod m20261105_000001_initial;
mod m20261106_000002_tags;
mod m20261107_000003_blockers;
mod m20261108_000004_saved_views;
mod m20261109_000005_parameters;
mod m20261110_000006_audit_entity;
mod m20261111_000007_sign_in_attempts;
mod m20261111_000008_exact_email;
mod m20261112_000009_handles;
mod m20261113_000010_need;
mod m20261114_000011_item_creator;
mod m20261115_000012_item_closer;
mod m20261116_000013_oauth;
mod m20261117_000014_token_family_device;
mod m20261118_000015_item_kind_and_stage;
mod m20261119_000016_invite_links;
mod m20261120_000017_item_steps;
mod m20261121_000018_heap_context;
mod m20261122_000019_long_text_on_mysql;
mod m20261123_000020_item_questions;
mod m20261124_000021_item_turns;
mod m20261125_000022_password_reset_links;
mod m20261126_000023_item_runs;
mod m20261127_000024_item_arrives_at;
mod m20261128_000025_item_comments;
mod m20261129_000026_reviews_and_redo;
mod m20261130_000027_item_origin;
mod m20261201_000028_blocked_reason;
mod m20261201_000029_item_split;
mod m20261202_000030_run_tool_times;
mod m20261203_000031_step_origin;
mod m20261204_000032_heap_origin;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20261105_000001_initial::Migration),
            Box::new(m20261106_000002_tags::Migration),
            Box::new(m20261107_000003_blockers::Migration),
            Box::new(m20261108_000004_saved_views::Migration),
            Box::new(m20261109_000005_parameters::Migration),
            Box::new(m20261110_000006_audit_entity::Migration),
            Box::new(m20261111_000007_sign_in_attempts::Migration),
            Box::new(m20261111_000008_exact_email::Migration),
            Box::new(m20261112_000009_handles::Migration),
            Box::new(m20261113_000010_need::Migration),
            Box::new(m20261114_000011_item_creator::Migration),
            Box::new(m20261115_000012_item_closer::Migration),
            Box::new(m20261116_000013_oauth::Migration),
            Box::new(m20261117_000014_token_family_device::Migration),
            Box::new(m20261118_000015_item_kind_and_stage::Migration),
            Box::new(m20261119_000016_invite_links::Migration),
            Box::new(m20261120_000017_item_steps::Migration),
            Box::new(m20261121_000018_heap_context::Migration),
            Box::new(m20261122_000019_long_text_on_mysql::Migration),
            Box::new(m20261123_000020_item_questions::Migration),
            Box::new(m20261124_000021_item_turns::Migration),
            Box::new(m20261125_000022_password_reset_links::Migration),
            Box::new(m20261126_000023_item_runs::Migration),
            Box::new(m20261127_000024_item_arrives_at::Migration),
            Box::new(m20261128_000025_item_comments::Migration),
            Box::new(m20261129_000026_reviews_and_redo::Migration),
            Box::new(m20261130_000027_item_origin::Migration),
            Box::new(m20261201_000028_blocked_reason::Migration),
            Box::new(m20261201_000029_item_split::Migration),
            Box::new(m20261202_000030_run_tool_times::Migration),
            Box::new(m20261203_000031_step_origin::Migration),
            Box::new(m20261204_000032_heap_origin::Migration),
        ]
    }
}
