use async_trait::async_trait;
use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::SqlitePool;

use crate::{
    application::library::bookshelf::folder::ports::FolderRepository,
    domain::library::bookshelf::{
        folder::{
            entity::Folder,
            value_objects::{FolderId, FolderName},
        },
        value_objects::BookshelfId,
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
            return Err(RepositoryError::FolderNotFound);
        }

        Ok(result)
    }
}

#[async_trait]
impl FolderRepository for SqliteFolderRepository {
    async fn create(&self, folder: Folder) -> Result<Folder, RepositoryError> {
        let (sql, values) = Query::insert()
            .into_table(Folders::Table)
            .columns([
                Folders::Id,
                Folders::BookshelfId,
                Folders::ParentId,
                Folders::Name,
                Folders::CreatedAt,
                Folders::UpdatedAt,
            ])
            .values_panic([
                folder.id.as_str().into(),
                folder.bookshelf_id.as_str().into(),
                folder.parent_id.as_deref().map(|id| id.as_str()).into(),
                folder.name.as_str().into(),
                folder.created_at.into(),
                folder.updated_at.into(),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;

        Ok(folder)
    }

    async fn rename(
        &self,
        bookshelf_id: &BookshelfId,
        folder_id: &FolderId,
        new_name: &FolderName,
    ) -> Result<(), RepositoryError> {
        let (sql, values) = Query::update()
            .table(Folders::Table)
            .value(Folders::Name, new_name.as_str())
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.as_str()))
            .and_where(Expr::col(Folders::Id).eq(folder_id.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;
        Ok(())
    }

    async fn delete(
        &self,
        bookshelf_id: &BookshelfId,
        folder_id: &FolderId,
    ) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Folders::Table)
            .and_where(Expr::col(Folders::Id).eq(folder_id.as_str()))
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.as_str()))
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;
        Ok(())
    }
}

#[derive(Iden)]
enum Folders {
    #[iden = "folders"]
    Table,
    Id,
    BookshelfId,
    ParentId,
    Name,
    CreatedAt,
    UpdatedAt,
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();
            let is_unique_failed = message.contains("UNIQUE constraint failed");
            let is_folder_conflict = message
                .contains("folders.bookshelf_id, folders.parent_id, folders.name")
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
