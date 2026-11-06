//! Conversions between domain values and their stored form. Stored strings are stable and never renamed.

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
