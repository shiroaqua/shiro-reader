use async_trait::async_trait;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::bookshelf::{dto::GetBookshelfOutput, ports::BookshelfRepository},
    domain::library::bookshelf::{entity::Bookshelf, value_objects::BookshelfId},
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::{Bookshelves, FULL_BOOKSHELVES_TABLE_COLUMNS},
        sqlite::{
            join_timestamp_ns, split_system_time, SqliteExecutor, SqliteRowExt, VersionedRecord, map_database_error,
            message_contains_columns,
        },
    },
};

use sea_query::{Expr, ExprTrait, Iden, Query, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;

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
    async fn create(&self, bookshelf: &Bookshelf) -> Result<(), RepositoryError> {
        let created_at = split_system_time(bookshelf.created_at());
        let updated_at = split_system_time(bookshelf.updated_at());
        let (sql, values) = Query::insert()
            .into_table(Bookshelves::Table)
            .columns(FULL_BOOKSHELVES_TABLE_COLUMNS)
            .values_panic([
                Expr::val(bookshelf.id().to_string()),
                Expr::val(bookshelf.name().to_string()),
                Expr::val(created_at.0),
                Expr::val(created_at.1),
                Expr::val(updated_at.0),
                Expr::val(updated_at.1),
                Expr::val(0),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.db.execute(&sql, values, map_sqlx_error).await?;

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

    async fn list(
        &self,
        ids: Vec<BookshelfId>,
    ) -> Result<Vec<GetBookshelfOutput>, RepositoryError> {
        let mut query = Query::select();
        query
            .columns(FULL_BOOKSHELVES_TABLE_COLUMNS)
            .from(Bookshelves::Table);

        if !ids.is_empty() {
            query.and_where(Expr::col(Bookshelves::Id).is_in(ids.iter().map(|id| id.to_string())));
        }

        let (sql, values) = query.build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }

    async fn find_by_id(
        &self,
        id: &BookshelfId,
    ) -> Result<VersionedRecord<Bookshelf>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_BOOKSHELVES_TABLE_COLUMNS)
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

    async fn commit(&self, bookshelf: VersionedRecord<Bookshelf>) -> Result<(), RepositoryError> {
        let updated_at = split_system_time(bookshelf.updated_at());
        let (sql, values) = Query::update()
            .table(Bookshelves::Table)
            .values([
                (Bookshelves::Name, bookshelf.name().to_string().into()),
                (Bookshelves::UpdatedAtHigh, updated_at.0.into()),
                (Bookshelves::UpdatedAtLow, updated_at.1.into()),
                (Bookshelves::Version, (bookshelf.version + 1).into()),
            ])
            .and_where(Expr::col(Bookshelves::Id).eq(bookshelf.id().to_string()))
            .and_where(Expr::col(Bookshelves::Version).eq(bookshelf.version))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(
                &sql,
                values,
                RepositoryError::BookshelfNotFound, // 此处错误处理存在歧义，到底是根本不存在还是单纯因为版本号不对呢？
                map_sqlx_error,
            )
            .await?;
        Ok(())
    }
}

impl TryFrom<&SqliteRow> for VersionedRecord<Bookshelf> {
    type Error = RepositoryError;
    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(VersionedRecord {
            value: Bookshelf::reconstitute(
                row.get_uuid(&Bookshelves::Id.to_string())?,
                row.get_string(&Bookshelves::Name.to_string())?,
                join_timestamp_ns(row.get(&Bookshelves::CreatedAtHigh.to_string())?, row.get(&Bookshelves::CreatedAtLow.to_string())?),
                join_timestamp_ns(row.get(&Bookshelves::UpdatedAtHigh.to_string())?, row.get(&Bookshelves::UpdatedAtLow.to_string())?),
            ),
            version: row.get(&Bookshelves::Version.to_string())?,
        })
    }
}

impl TryFrom<&SqliteRow> for GetBookshelfOutput {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(GetBookshelfOutput {
            id: row.get_string(&Bookshelves::Id.to_string())?,
            name: row.get_string(&Bookshelves::Name.to_string())?,
            created_at: join_timestamp_ns(row.get(&Bookshelves::CreatedAtHigh.to_string())?, row.get(&Bookshelves::CreatedAtLow.to_string())?),
            updated_at: join_timestamp_ns(row.get(&Bookshelves::UpdatedAtHigh.to_string())?, row.get(&Bookshelves::UpdatedAtLow.to_string())?),
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
