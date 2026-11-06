use sea_orm::{DbErr, SqlErr};

/// What went wrong talking to the database. Classification into HTTP responses happens in `api`.
#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database is unavailable")]
    Unavailable(#[source] DbErr),
    #[error("a row with the same unique key already exists")]
    Duplicate(#[source] DbErr),
    #[error("database operation failed")]
    Database(#[source] DbErr),
    #[error("row {id} in {table} is invalid: {problem}")]
    CorruptRow {
        table: &'static str,
        id: String,
        problem: String,
    },
}

impl From<DbErr> for StoreError {
    fn from(err: DbErr) -> Self {
        if matches!(err, DbErr::ConnectionAcquire(_) | DbErr::Conn(_)) {
            return Self::Unavailable(err);
        }
        if matches!(err.sql_err(), Some(SqlErr::UniqueConstraintViolation(_))) {
            return Self::Duplicate(err);
        }
        Self::Database(err)
    }
}

impl StoreError {
    pub(crate) fn corrupt(table: &'static str, id: &str, problem: impl Into<String>) -> Self {
        Self::CorruptRow {
            table,
            id: id.to_owned(),
            problem: problem.into(),
        }
    }
}
