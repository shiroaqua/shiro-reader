use async_trait::async_trait;
use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::bookshelf::folder::ports::FolderRepository,
    domain::library::bookshelf::{
        folder::{
            entity::Folder,
            value_objects::{FolderId, FolderName},
        },
        value_objects::BookshelfId,
    },
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::{FULL_FOLDERS_TABLE_COLUMNS, Folders},
        sqlite::{SqliteExecutor, SqliteRowExt, map_database_error, message_contains_columns},
    },
};



#[derive(Clone)]
pub struct SqliteFolderRepository {
    db: SqliteExecutor,
}

impl SqliteFolderRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: SqliteExecutor::new(pool),
        }
    }
}

#[async_trait]
impl FolderRepository for SqliteFolderRepository {
    async fn create(&self, folder: Folder) -> Result<Folder, RepositoryError> {
        let (sql, values) = Query::insert()
            .into_table(Folders::Table)
            .columns(FULL_FOLDERS_TABLE_COLUMNS)
            .values_panic([
                folder.id.to_string().into(),
                folder.bookshelf_id.to_string().into(),
                folder.parent_id.as_deref().map(|id| id.to_string()).into(),
                folder.name.as_str().into(),
                folder.created_at.into(),
                folder.updated_at.into(),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.db.execute(&sql, values, map_sqlx_error).await?;

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
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.to_string()))
            .and_where(Expr::col(Folders::Id).eq(folder_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(
                &sql,
                values,
                RepositoryError::FolderNotFound,
                map_sqlx_error,
            )
            .await?;
        Ok(())
    }

    async fn delete(
        &self,
        bookshelf_id: &BookshelfId,
        folder_id: &FolderId,
    ) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Folders::Table)
            .and_where(Expr::col(Folders::Id).eq(folder_id.to_string()))
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(
                &sql,
                values,
                RepositoryError::FolderNotFound,
                map_sqlx_error,
            )
            .await?;
        Ok(())
    }

    async fn find_by_id(
        &self,
        bookshelf_id: &BookshelfId,
        folder_id: &FolderId,
    ) -> Result<Folder, RepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_FOLDERS_TABLE_COLUMNS)
            .from(Folders::Table)
            .and_where(Expr::col(Folders::Id).eq(folder_id.to_string()))
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .fetch_optional(
                &sql,
                values,
                RepositoryError::FolderNotFound,
                map_sqlx_error,
            )
            .await
    }

    async fn list_root(&self, bookshelf_id: &BookshelfId) -> Result<Vec<Folder>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_FOLDERS_TABLE_COLUMNS)
            .from(Folders::Table)
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.to_string()))
            .and_where(Expr::col(Folders::ParentId).is_null())
            .build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }

    async fn list(&self, bookshelf_id: &BookshelfId) -> Result<Vec<Folder>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_FOLDERS_TABLE_COLUMNS)
            .from(Folders::Table)
            .and_where(Expr::col(Folders::BookshelfId).eq(bookshelf_id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for Folder {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Folder {
            id: row.get_uuid(&Folders::Id.to_string())?,
            bookshelf_id: row.get_uuid(&Folders::BookshelfId.to_string())?,
            parent_id: row.get_optional_uuid(&Folders::ParentId.to_string())?,
            name: row.get_string(&Folders::Name.to_string())?,
            created_at: row.get(&Folders::CreatedAt.to_string())?,
            updated_at: row.get(&Folders::UpdatedAt.to_string())?,
        })
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |database_error| {
        let is_nested_folder_conflict = message_contains_columns(
            database_error.message(),
            Folders::Table,
            [Folders::BookshelfId, Folders::ParentId, Folders::Name],
        );
        let is_root_folder_conflict = message_contains_columns(
            database_error.message(),
            Folders::Table,
            [Folders::BookshelfId, Folders::Name],
        );

        if database_error.is_unique_constraint()
            && (is_nested_folder_conflict || is_root_folder_conflict)
        {
            Some(RepositoryError::FolderNameConflict)
        } else if database_error.is_foreign_key_constraint() {
            Some(RepositoryError::ParentFolderNotFound)
        } else {
            None
        }
    })
}
