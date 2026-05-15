use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    application::library::bookshelf::ports::BookshelfRepository,
    domain::library::bookshelf::{entity::Bookshelf, value_objects::BookshelfId},
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
}


#[async_trait]
impl BookshelfRepository for SqliteBookshelfRepository {
    async fn create(&self, bookshelf: Bookshelf) -> Result<Bookshelf, RepositoryError> {
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

        sqlx::query_with(&sql, values)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        Ok(bookshelf)
    }

    async fn delete(&self, bookshelf_id: &BookshelfId) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(bookshelf_id.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        let result = sqlx::query_with(&sql, values)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::BookshelfNotFound);
        }

        Ok(())
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
