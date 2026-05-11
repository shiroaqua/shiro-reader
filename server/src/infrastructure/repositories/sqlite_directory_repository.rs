use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    application::bookshelf::ports::DirectoryRepository,
    domain::bookshelf::{
        entity::Directory,
    },
    infrastructure::repositories::errors::RepositoryError,
};

#[derive(Clone)]
pub struct SqliteDirectoryRepository {
    pool: SqlitePool,
}

impl SqliteDirectoryRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DirectoryRepository for SqliteDirectoryRepository {
    async fn create(&self, directory: Directory) -> Result<Directory, RepositoryError> {
        let parent_id = directory
            .parent_id
            .as_ref()
            .map(|id| id.as_str())
            .ok_or(RepositoryError::ParentDirectoryNotFound)?;

        sqlx::query(
            r#"
            INSERT INTO directories (id, parent_id, name, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(directory.id.as_str())
        .bind(parent_id)
        .bind(directory.name.as_str())
        .bind(directory.created_at)
        .bind(directory.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(directory)
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();
            if message.contains("UNIQUE constraint failed: directories.parent_id, directories.name") {
                RepositoryError::DirectoryNameConflict
            } else if message.contains("FOREIGN KEY constraint failed") {
                RepositoryError::ParentDirectoryNotFound
            } else {
                RepositoryError::Storage(anyhow::Error::new(error))
            }
        }
        _ => RepositoryError::Storage(anyhow::Error::new(error)),
    }
}
