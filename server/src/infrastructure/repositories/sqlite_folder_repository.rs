use async_trait::async_trait;
use sqlx::SqlitePool;

use crate::{
    application::library::bookshelf::folder::ports::FolderRepository,
    domain::library::bookshelf::folder::{entity::Folder, value_objects::FolderId},
    domain::library::bookshelf::value_objects::BookshelfId,
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

    async fn delete(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId) -> Result<(), RepositoryError> {
        let result = sqlx::query(
            r#"
            DELETE FROM folders
            WHERE id = ? AND bookshelf_id = ?
        "#,
        )
        .bind(folder_id.as_str())
        .bind(bookshelf_id.as_str())
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::FolderNotFound);
        }

        Ok(())
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();
            let is_unique_failed = message.contains("UNIQUE constraint failed");
            let is_folder_conflict = message.contains("folders.bookshelf_id, folders.parent_id, folders.name")
                || message.contains("folders.bookshelf_id, folders.name");

            if is_unique_failed && is_folder_conflict {
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
