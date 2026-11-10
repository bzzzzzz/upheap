//! What a proxy connection needs to know about the schema.
//!
//! SeaORM's proxy rows must hold exactly the value type each entity field reads. SQLite can't say it: it declares
//! `i32` and `i64` columns both as `integer`. The entities can, so this lists every column's type from them.

use sea_orm::{ColumnTrait, ColumnType, DbErr, EntityTrait, IdenStatic, Iterable, RuntimeErr};

use crate::entities::*;

/// A column of a table, with the type its entity reads it as.
#[derive(Clone, Debug)]
pub struct ColumnSpec {
    pub table: &'static str,
    pub column: &'static str,
    pub column_type: ColumnType,
}

/// Every column of every table, SeaORM's migration table included.
pub fn list_column_specs() -> Vec<ColumnSpec> {
    let mut specs = Vec::new();
    add_entity::<audit_log::Entity>(&mut specs);
    add_entity::<heaps::Entity>(&mut specs);
    add_entity::<item_parameter_values::Entity>(&mut specs);
    add_entity::<item_questions::Entity>(&mut specs);
    add_entity::<item_steps::Entity>(&mut specs);
    add_entity::<item_tags::Entity>(&mut specs);
    add_entity::<items::Entity>(&mut specs);
    add_entity::<parameter_options::Entity>(&mut specs);
    add_entity::<parameter_tags::Entity>(&mut specs);
    add_entity::<parameters::Entity>(&mut specs);
    add_entity::<password_credentials::Entity>(&mut specs);
    add_entity::<saved_views::Entity>(&mut specs);
    add_entity::<sessions::Entity>(&mut specs);
    add_entity::<sign_in_attempts::Entity>(&mut specs);
    add_entity::<tags::Entity>(&mut specs);
    add_entity::<team_members::Entity>(&mut specs);
    add_entity::<teams::Entity>(&mut specs);
    add_entity::<users::Entity>(&mut specs);
    add_entity::<sea_orm_migration::seaql_migrations::Entity>(&mut specs);
    specs
}

fn add_entity<E: EntityTrait>(specs: &mut Vec<ColumnSpec>) {
    let table = E::default().table_name();
    for column in E::Column::iter() {
        specs.push(ColumnSpec {
            table,
            column: column.as_str(),
            column_type: column.def().get_column_type().clone(),
        });
    }
}

/// Marks an error as a unique-key violation.
const UNIQUE_VIOLATION: &str = "unique key violated";

/// The error a proxy connection returns when a write breaks a unique key. SeaORM recognises unique violations
/// only in sqlx's errors, so a proxy has to mark them for [`crate::StoreError::Duplicate`] to still be raised.
pub fn unique_violation(message: &str) -> DbErr {
    DbErr::Query(RuntimeErr::Internal(format!("{UNIQUE_VIOLATION}: {message}")))
}

pub(crate) fn is_unique_violation(err: &DbErr) -> bool {
    matches!(
        err,
        DbErr::Query(RuntimeErr::Internal(message)) | DbErr::Exec(RuntimeErr::Internal(message))
            if message.starts_with(UNIQUE_VIOLATION)
    )
}
