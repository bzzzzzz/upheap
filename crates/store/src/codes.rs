//! Conversions between domain values and their stored form. Stored strings are stable and never renamed.

use domain::handle::{Handle, ItemKey};
use domain::validation::Field;
use domain::{Timestamp, Version};
use uuid::Uuid;

use crate::StoreError;

/// Reads a column as a typed value, reporting the row as corrupt if it doesn't parse.
pub(crate) struct RowReader<'a> {
    pub table: &'static str,
    pub id: &'a str,
}

impl RowReader<'_> {
    pub fn uuid(&self, raw: &str) -> Result<Uuid, StoreError> {
        Uuid::parse_str(raw).map_err(|_| self.corrupt(format!("invalid UUID {raw:?}")))
    }

    pub fn timestamp(&self, millis: i64) -> Result<Timestamp, StoreError> {
        Timestamp::from_millis(millis).ok_or_else(|| self.corrupt(format!("timestamp {millis} out of range")))
    }

    pub fn optional_timestamp(&self, millis: Option<i64>) -> Result<Option<Timestamp>, StoreError> {
        millis.map(|m| self.timestamp(m)).transpose()
    }

    pub fn version(&self, raw: i32) -> Result<Version, StoreError> {
        u32::try_from(raw)
            .ok()
            .filter(|v| *v >= 1)
            .map(Version::new)
            .ok_or_else(|| self.corrupt(format!("version {raw} is not positive")))
    }

    pub fn weight(&self, raw: i32) -> Result<u16, StoreError> {
        u16::try_from(raw).map_err(|_| self.corrupt(format!("weight {raw} out of range")))
    }

    pub fn handle(&self, raw: &str) -> Result<Handle, StoreError> {
        Handle::parse(raw, Field::Slug).map_err(|_| self.corrupt(format!("invalid handle {raw:?}")))
    }

    pub fn item_key(&self, raw: &str) -> Result<ItemKey, StoreError> {
        ItemKey::parse(raw).map_err(|_| self.corrupt(format!("invalid item key {raw:?}")))
    }

    pub fn corrupt(&self, problem: String) -> StoreError {
        StoreError::corrupt(self.table, self.id, problem)
    }
}

/// The stored form of a version. Versions that don't fit an `INT` can't come from a valid row.
pub(crate) fn version_to_db(version: Version, table: &'static str, id: &str) -> Result<i32, StoreError> {
    i32::try_from(version.get()).map_err(|_| StoreError::corrupt(table, id, "version overflow"))
}

pub(crate) fn id_to_db(uuid: &Uuid) -> String {
    uuid.hyphenated().to_string()
}

/// One group of a `GROUP BY` count, selected as `group_key` and `count`: the grouping column's stored value and how
/// many rows have it.
#[derive(Debug)]
pub(crate) struct GroupCount {
    pub group_key: String,
    pub count: u32,
}

impl sea_orm::FromQueryResult for GroupCount {
    fn from_query_result(row: &sea_orm::QueryResult, pre: &str) -> Result<Self, sea_orm::DbErr> {
        let group_key = row.try_get(pre, "group_key")?;
        // sqlx returns `COUNT` as a 64-bit integer on every backend. The local build's proxy types an expression
        // from its value, as a 32-bit integer while it fits, and SeaORM won't read that as `i64`.
        let count = row
            .try_get::<i64>(pre, "count")
            .or_else(|_| row.try_get::<i32>(pre, "count").map(i64::from))?;
        Ok(Self {
            group_key,
            count: u32::try_from(count).unwrap_or(u32::MAX),
        })
    }
}
