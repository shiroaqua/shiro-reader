use async_trait::async_trait;
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::{
    application::library::bookshelf::ports::BookshelfRepository,
    domain::library::bookshelf::{
        entity::Bookshelf,
        value_objects::{BookshelfId, BookshelfName},
    },
    infrastructure::repositories::errors::RepositoryError,
};

use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;

#[derive(Clone)]
pub struct SqliteBookshelfRepository {
    pool: SqlitePool,
}

impl SqliteBookshelfRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn execute(
        &self,
        sql: &str,
        values: sea_query_binder::SqlxValues,
    ) -> Result<sqlx::sqlite::SqliteQueryResult, RepositoryError> {
        let result = sqlx::query_with(&sql, values)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::BookshelfNotFound);
        }

        Ok(result)
    }
}

#[async_trait]
impl BookshelfRepository for SqliteBookshelfRepository {
    async fn create(&self, bookshelf: &Bookshelf) -> Result<Bookshelf, RepositoryError> {
        let (sql, values) = Query::insert()
            .into_table(Bookshelves::Table)
            .columns([
                Bookshelves::Id,
                Bookshelves::Name,
                Bookshelves::CreatedAt,
                Bookshelves::UpdatedAt,
            ])
            .values_panic([
                bookshelf.id.as_str().into(),
                bookshelf.name.as_str().into(),
                bookshelf.created_at.into(),
                bookshelf.updated_at.into(),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;

        Ok(bookshelf.clone())
    }

    async fn rename(
        &self,
        id: &BookshelfId,
        new_name: &BookshelfName,
    ) -> Result<(), RepositoryError> {
        let (sql, values) = Query::update()
            .table(Bookshelves::Table)
            .value(Bookshelves::Name, new_name.as_str())
            .and_where(Expr::col(Bookshelves::Id).eq(id.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;

        Ok(())
    }

    async fn delete(&self, id: &BookshelfId) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(id.as_str()))
            .build_sqlx(SqliteQueryBuilder);
        
        self.execute(&sql, values).await?;

        Ok(())
    }

    async fn get(&self, id: &BookshelfId) -> Result<Bookshelf, RepositoryError> {
        let (sql, values) = Query::select()
            .columns([
                Bookshelves::Id,
                Bookshelves::Name,
                Bookshelves::CreatedAt,
                Bookshelves::UpdatedAt,
            ])
            .from(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(id.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        Ok(Bookshelf::try_from(
            &sqlx::query_with(&sql, values)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?
                .ok_or(RepositoryError::BookshelfNotFound)?,
        )?)
    }

    async fn get_all(&self) -> Result<Vec<Bookshelf>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns([
                Bookshelves::Id,
                Bookshelves::Name,
                Bookshelves::CreatedAt,
                Bookshelves::UpdatedAt,
            ])
            .from(Bookshelves::Table)
            .build_sqlx(SqliteQueryBuilder);

        let rows = sqlx::query_with(&sql, values)
            .fetch_all(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(rows
            .iter()
            .map(|r| r.try_into())
            .collect::<Result<Vec<Bookshelf>, _>>()?)
    }
}

impl TryFrom<&SqliteRow> for Bookshelf {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Bookshelf {
            id: BookshelfId::from(row.try_get::<String, _>("id").map_err(map_sqlx_error)?),
            name: BookshelfName::from(row.try_get::<String, _>("name").map_err(map_sqlx_error)?),
            created_at: row.try_get("created_at").map_err(map_sqlx_error)?,
            updated_at: row.try_get("updated_at").map_err(map_sqlx_error)?,
        })
    }
}

#[derive(Iden)]
enum Bookshelves {
    #[iden = "bookshelves"]
    Table,
    Id,
    Name,
    CreatedAt,
    UpdatedAt,
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();

            if message.contains("UNIQUE constraint failed: bookshelves.name") {
                RepositoryError::BookshelfNameConflict
            } else {
                RepositoryError::Storage(anyhow::Error::new(error))
            }
        }
        _ => RepositoryError::Storage(anyhow::Error::new(error)),
    }
}
