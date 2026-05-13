use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    application::library::bookshelf::ports::BookshelfRepository,
    domain::library::bookshelf::{entity::Bookshelf, value_objects::BookshelfId},
    infrastructure::repositories::errors::RepositoryError,
};

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
        sqlx::query(
            r#"
            INSERT INTO bookshelves (id, name, created_at, updated_at)
            VALUES (?, ?, ?, ?)
            "#,
        )
        .bind(bookshelf.id.as_str())
        .bind(bookshelf.name.as_str())
        .bind(bookshelf.created_at)
        .bind(bookshelf.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(bookshelf)
    }

    async fn delete(&self, bookshelf_id: &BookshelfId) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            r#"
            DELETE FROM bookshelves
            WHERE id = ?
        "#,
        )
        .bind(bookshelf_id.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::BookshelfNotFound);
        }

        Ok(())
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();

            if message.contains("UNIQUE constraint failed: bookshelves.name") {
                RepositoryError::BookshelfNameConflict
            }  else {
                RepositoryError::Storage(anyhow::Error::new(error))
            }
        }
        _ => RepositoryError::Storage(anyhow::Error::new(error)),
    }
}
