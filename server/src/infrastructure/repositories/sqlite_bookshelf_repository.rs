use async_trait::async_trait;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::bookshelf::ports::BookshelfRepository,
    domain::library::bookshelf::{
        entity::Bookshelf,
        value_objects::{BookshelfId, BookshelfName},
    },
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::Bookshelves,
        sqlite::{SqliteExecutor, SqliteRowExt, map_database_error, message_contains_columns},
    },
};

use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;

#[derive(Clone)]
pub struct SqliteBookshelfRepository {
    db: SqliteExecutor,
}

impl SqliteBookshelfRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: SqliteExecutor::new(pool),
        }
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
                bookshelf.id.to_string().into(),
                bookshelf.name.to_string().into(),
                bookshelf.created_at.into(),
                bookshelf.updated_at.into(),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.db.execute(&sql, values, map_sqlx_error).await?;

        Ok(bookshelf)
    }

    async fn rename(
        &self,
        id: &BookshelfId,
        new_name: &BookshelfName,
    ) -> Result<(), RepositoryError> {
        let (sql, values) = Query::update()
            .table(Bookshelves::Table)
            .value(Bookshelves::Name, new_name.as_str())
            .and_where(Expr::col(Bookshelves::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(
                &sql,
                values,
                RepositoryError::BookshelfNotFound,
                map_sqlx_error,
            )
            .await?;

        Ok(())
    }

    async fn delete(&self, id: &BookshelfId) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(
                &sql,
                values,
                RepositoryError::BookshelfNotFound,
                map_sqlx_error,
            )
            .await?;

        Ok(())
    }

    async fn find_by_id(&self, id: &BookshelfId) -> Result<Bookshelf, RepositoryError> {
        let (sql, values) = Query::select()
            .columns([
                Bookshelves::Id,
                Bookshelves::Name,
                Bookshelves::CreatedAt,
                Bookshelves::UpdatedAt,
            ])
            .from(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .fetch_optional(
                &sql,
                values,
                RepositoryError::BookshelfNotFound,
                map_sqlx_error,
            )
            .await
    }

    async fn list(&self) -> Result<Vec<Bookshelf>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns([
                Bookshelves::Id,
                Bookshelves::Name,
                Bookshelves::CreatedAt,
                Bookshelves::UpdatedAt,
            ])
            .from(Bookshelves::Table)
            .build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for Bookshelf {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Bookshelf {
            id: row.get_uuid(&Bookshelves::Id.to_string())?,
            name: row.get_string(&Bookshelves::Name.to_string())?,
            created_at: row.get(&Bookshelves::CreatedAt.to_string())?,
            updated_at: row.get(&Bookshelves::UpdatedAt.to_string())?,
        })
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |database_error| {
        let is_name_conflict = message_contains_columns(
            database_error.message(),
            Bookshelves::Table,
            [Bookshelves::Name],
        );

        if database_error.is_unique_constraint() && is_name_conflict {
            Some(RepositoryError::BookshelfNameConflict)
        } else {
            None
        }
    })
}
