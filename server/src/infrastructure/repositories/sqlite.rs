use blake3::Hash;
use sea_query_binder::SqlxValues;
use sqlx::{
    sqlite::{SqliteQueryResult, SqliteRow},
    Decode, Row, Sqlite, SqlitePool, Transaction, Type,
};
use uuid::Uuid;

use crate::infrastructure::repositories::errors::RepositoryError;

#[derive(Clone)]
pub struct SqliteExecutor {
    pool: SqlitePool,
}

pub struct SqliteTransaction<'a> {
    tx: Transaction<'a, Sqlite>,
}

impl SqliteExecutor {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub async fn begin(
        &self,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<SqliteTransaction<'_>, RepositoryError> {
        let tx = self.pool.begin().await.map_err(map_error)?;
        Ok(SqliteTransaction { tx })
    }

    pub async fn execute(
        &self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<SqliteQueryResult, RepositoryError> {
        sqlx::query_with(sql, values)
            .execute(&self.pool)
            .await
            .map_err(map_error)
    }

    pub async fn execute_affected(
        &self,
        sql: &str,
        values: SqlxValues,
        not_found: RepositoryError,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<(), RepositoryError> {
        let result = self.execute(sql, values, map_error).await?;

        if result.rows_affected() == 0 {
            return Err(not_found);
        }

        Ok(())
    }

    pub async fn fetch_optional<T>(
        &self,
        sql: &str,
        values: SqlxValues,
        not_found: RepositoryError,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<T, RepositoryError>
    where
        T: for<'row> TryFrom<&'row SqliteRow, Error = RepositoryError>,
    {
        let row = sqlx::query_with(sql, values)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?
            .ok_or(not_found)?;

        T::try_from(&row)
    }

    pub async fn fetch_all<T>(
        &self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<Vec<T>, RepositoryError>
    where
        T: for<'row> TryFrom<&'row SqliteRow, Error = RepositoryError>,
    {
        let rows = sqlx::query_with(sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(map_error)?;

        rows.iter().map(T::try_from).collect()
    }
}

impl SqliteTransaction<'_> {
    pub async fn execute(
        &mut self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<SqliteQueryResult, RepositoryError> {
        sqlx::query_with(sql, values)
            .execute(self.tx.as_mut())
            .await
            .map_err(map_error)
    }

    pub async fn fetch_exists(
        &mut self,
        sql: &str,
        values: SqlxValues,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<bool, RepositoryError> {
        let row = sqlx::query_with(sql, values)
            .fetch_optional(self.tx.as_mut())
            .await
            .map_err(map_error)?;

        Ok(row.is_some())
    }

    pub async fn commit(
        self,
        map_error: impl Fn(sqlx::Error) -> RepositoryError,
    ) -> Result<(), RepositoryError> {
        self.tx.commit().await.map_err(map_error)
    }
}

pub fn map_invalid_data(error: impl Into<anyhow::Error>) -> RepositoryError {
    RepositoryError::Storage(error.into())
}

pub fn map_database_error(
    error: sqlx::Error,
    classify: impl FnOnce(&str) -> Option<RepositoryError>,
) -> RepositoryError {
    if let sqlx::Error::Database(database_error) = &error {
        if let Some(error) = classify(database_error.message()) {
            return error;
        }
    }

    map_storage_error(error)
}

pub fn map_storage_error(error: sqlx::Error) -> RepositoryError {
    RepositoryError::Storage(anyhow::Error::new(error))
}

pub trait SqliteRowExt {
    fn get<T>(&self, column: &str) -> Result<T, RepositoryError>
    where
        for<'decode> T: Decode<'decode, Sqlite> + Type<Sqlite>;

    fn get_uuid<T>(&self, column: &str) -> Result<T, RepositoryError>
    where
        T: From<Uuid>;

    fn get_optional_uuid<T>(&self, column: &str) -> Result<Option<T>, RepositoryError>
    where
        T: From<Uuid>;

    fn get_hash<T>(&self, column: &str) -> Result<T, RepositoryError>
    where 
        T: From<Hash>;
}

impl SqliteRowExt for SqliteRow {
    fn get<T>(&self, column: &str) -> Result<T, RepositoryError>
    where
        for<'decode> T: Decode<'decode, Sqlite> + Type<Sqlite>,
    {
        self.try_get(column).map_err(map_storage_error)
    }

    fn get_uuid<T>(&self, column: &str) -> Result<T, RepositoryError>
    where
        T: From<Uuid>,
    {
        let value = SqliteRowExt::get::<String>(self, column)?;
        parse_uuid(value)
    }

    fn get_optional_uuid<T>(&self, column: &str) -> Result<Option<T>, RepositoryError>
    where
        T: From<Uuid>,
    {
        let value = SqliteRowExt::get::<Option<String>>(self, column)?;
        value.map(parse_uuid).transpose()
    }

    fn get_hash<T>(&self, column: &str) -> Result<T, RepositoryError>
    where 
        T: From<Hash>, {    
        let value = SqliteRowExt::get::<String>(self, column)?;
        blake3::Hash::from_hex(value).map(T::from).map_err(map_invalid_data)
    }


}

fn parse_uuid<T>(value: String) -> Result<T, RepositoryError>
where
    T: From<Uuid>,
{
    Uuid::parse_str(&value)
        .map(T::from)
        .map_err(map_invalid_data)
}
