use std::{
    borrow::Cow,
    ops::{Deref, DerefMut},
    time::{SystemTime, UNIX_EPOCH},
};

use blake3::Hash;
use sea_query::Iden;
use sea_query_sqlx::SqlxValues;
use sqlx::{
    Decode, Row, Sqlite, SqlitePool, Transaction, Type,
    sqlite::{SqliteQueryResult, SqliteRow},
};
use uuid::Uuid;

pub(crate) fn split_timestamp_ns(timestamp: i128) -> (i64, i64) {
    let high = (timestamp >> 64) as i64;
    let low = (timestamp as u64 ^ (1_u64 << 63)) as i64;
    (high, low)
}

pub(crate) fn join_timestamp_ns(high: i64, low: i64) -> i128 {
    ((high as i128) << 64) | ((low as u64 ^ (1_u64 << 63)) as i128)
}

pub(crate) fn split_system_time(timestamp: SystemTime) -> (i64, i64) {
    match timestamp.duration_since(UNIX_EPOCH) {
        Ok(duration) => split_timestamp_ns(duration.as_nanos() as i128),
        Err(error) => split_timestamp_ns(-(error.duration().as_nanos() as i128)),
    }
}

#[cfg(test)]
mod timestamp_tests {
    use super::{join_timestamp_ns, split_timestamp_ns};

    #[test]
    fn split_timestamp_round_trips_across_signed_boundaries() {
        for value in [i128::MIN, -1, 0, i128::MAX] {
            let (high, low) = split_timestamp_ns(value);
            assert_eq!(join_timestamp_ns(high, low), value);
        }
    }
}

#[derive(Clone)]
pub struct SqliteExecutor {
    pool: SqlitePool,
}

pub struct SqliteTransaction<'a> {
    tx: Transaction<'a, Sqlite>,
}


pub struct VersionedRecord<T> {
    pub(super) value: T,
    pub(super) version: i64
}

impl<T> Deref for VersionedRecord<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for VersionedRecord<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl SqliteExecutor {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn begin<E>(
        &self,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<SqliteTransaction<'_>, E> {
        let tx = self.pool.begin().await.map_err(map_error)?;
        Ok(SqliteTransaction { tx })
    }

    pub async fn execute<E>(
        &self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<SqliteQueryResult, E> {
        sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .execute(&self.pool)
            .await
            .map_err(map_error)
    }

    pub async fn execute_affected<E>(
        &self,
        sql: &str,
        values: SqlxValues,
        not_found: E,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<(), E> {
        let result = self.execute(sql, values, map_error).await?;

        if result.rows_affected() == 0 {
            return Err(not_found);
        }

        Ok(())
    }

    pub async fn fetch_optional<T, E>(
        &self,
        sql: &str,
        values: SqlxValues,
        not_found: E,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<T, E>
    where
        T: for<'row> TryFrom<&'row SqliteRow, Error = E>,
    {
        let row = sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?
            .ok_or(not_found)?;

        T::try_from(&row)
    }

    pub async fn fetch_exists<E>(
        &self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<bool, E> {
        let row = sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?;

        Ok(row.is_some())
    }

    pub async fn fetch_all<T, E>(
        &self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<Vec<T>, E>
    where
        T: for<'row> TryFrom<&'row SqliteRow, Error = E>,
    {
        let rows = sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;

        rows.iter().map(T::try_from).collect()
    }
}

impl SqliteTransaction<'_> {
    pub async fn execute<E>(
        &mut self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<SqliteQueryResult, E> {
        sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .execute(self.tx.as_mut())
            .await
            .map_err(map_error)
    }

   pub async fn execute_affected<E>(
        &mut self,
        sql: &str,
        values: SqlxValues,
        not_found: E,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<(), E> {
        let result = self.execute(sql, values, map_error).await?;

        if result.rows_affected() == 0 {
            return Err(not_found);
        }

        Ok(())
    }

    pub async fn fetch_exists<E>(
        &mut self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<bool, E> {
        let row = sqlx::query_with(sqlx::AssertSqlSafe(sql), values)
            .fetch_optional(self.tx.as_mut())
            .await
            .map_err(map_error)?;

        Ok(row.is_some())
    }

    pub async fn commit<E>(
        self,
        map_error: impl Fn(sqlx::Error) -> E,
    ) -> Result<(), E> {
        self.tx.commit().await.map_err(map_error)
    }
}

pub fn map_invalid_data(error: impl Into<anyhow::Error>) -> anyhow::Error {
    error.into()
}

pub fn map_database_error<E>(
    error: sqlx::Error,
    classify: impl FnOnce(SqliteDatabaseError<'_>) -> Option<E>,
) -> E
where
    E: From<anyhow::Error>,
{
    if let sqlx::Error::Database(database_error) = &error {
        let database_error = SqliteDatabaseError {
            code: database_error.code(),
            message: database_error.message(),
        };

        if let Some(error) = classify(database_error) {
            return error;
        }
    }

    map_storage_error(error).into()
}

pub struct SqliteDatabaseError<'a> {
    code: Option<Cow<'a, str>>,
    message: &'a str,
}

impl SqliteDatabaseError<'_> {
    pub fn message(&self) -> &str {
        self.message
    }

    pub fn is_unique_constraint(&self) -> bool {
        self.has_code("2067")
    }

    pub fn is_foreign_key_constraint(&self) -> bool {
        self.has_code("787")
    }
    
    pub fn is_trigger_constraint(&self) -> bool {
        self.has_code("1811")
    }

    fn has_code(&self, expected: &str) -> bool {
        self.code.as_deref() == Some(expected)
    }
}

pub fn message_contains_columns<T, C>(
    message: &str,
    table: T,
    columns: impl IntoIterator<Item = C>,
) -> bool
where
    T: Iden,
    C: Iden,
{
    let table = table.to_string();
    let columns = columns
        .into_iter()
        .map(|column| format!("{}.{}", table, column.to_string()))
        .collect::<Vec<_>>()
        .join(", ");

    message.contains(&columns)
}

pub fn map_storage_error(error: sqlx::Error) -> anyhow::Error {
    anyhow::Error::new(error)
}

pub trait SqliteRowExt {
    fn get<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        for<'decode> T: Decode<'decode, Sqlite> + Type<Sqlite>;

    fn get_string<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<String>;

    fn get_uuid<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<Uuid>;

    fn get_optional_uuid<T>(&self, column: &str) -> Result<Option<T>, anyhow::Error>
    where
        T: From<Uuid>;

    fn get_hash<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<Hash>;
}

impl SqliteRowExt for SqliteRow {
    fn get<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        for<'decode> T: Decode<'decode, Sqlite> + Type<Sqlite>,
    {
        self.try_get(column).map_err(map_storage_error)
    }

    fn get_string<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<String>,
    {
        let value = SqliteRowExt::get::<String>(self, column)?;
        Ok(T::from(value))
    }

    fn get_uuid<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<Uuid>,
    {
        let value = SqliteRowExt::get::<String>(self, column)?;
        parse_uuid(value)
    }

    fn get_optional_uuid<T>(&self, column: &str) -> Result<Option<T>, anyhow::Error>
    where
        T: From<Uuid>,
    {
        let value = SqliteRowExt::get::<Option<String>>(self, column)?;
        value.map(parse_uuid).transpose()
    }

    fn get_hash<T>(&self, column: &str) -> Result<T, anyhow::Error>
    where
        T: From<Hash>,
    {
        let value = SqliteRowExt::get::<String>(self, column)?;
        blake3::Hash::from_hex(value)
            .map(T::from)
            .map_err(map_invalid_data)
    }
}

fn parse_uuid<T>(value: String) -> Result<T, anyhow::Error>
where
    T: From<Uuid>,
{
    Uuid::parse_str(&value)
        .map(T::from)
        .map_err(map_invalid_data)
}
