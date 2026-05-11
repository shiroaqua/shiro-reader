use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    application::library::bookshelf::folder::ports::FolderRepository,
    domain::library::bookshelf::folder::{
        entity::Folder,
    },
    infrastructure::repositories::errors::RepositoryError,
};

#[derive(Clone)]
pub struct SqliteFolderRepository {
    pool: SqlitePool,
}

impl SqliteFolderRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl FolderRepository for SqliteFolderRepository {
    async fn create(&self, folder: Folder) -> Result<Folder, RepositoryError> {
        sqlx::query(
            r#"
            INSERT INTO folders (id, bookshelf_id, parent_id, name, created_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(folder.id.as_str())
        .bind(folder.bookshelf_id.as_str())
        .bind(folder.parent_id.as_deref())
        .bind(folder.name.as_str())
        .bind(folder.created_at)
        .bind(folder.updated_at)
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(folder)
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();
            if message.contains("UNIQUE constraint failed: folder.bookshelf_id, folder.parent_id, folder.name") {
                RepositoryError::FolderNameConflict
            } else if message.contains("FOREIGN KEY constraint failed") {
                RepositoryError::ParentFolderNotFound
            } else {
                RepositoryError::Storage(anyhow::Error::new(error))
            }
        }
        _ => RepositoryError::Storage(anyhow::Error::new(error)),
    }
}
