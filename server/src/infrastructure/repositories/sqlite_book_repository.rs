use async_trait::async_trait;
use sea_query::{Expr, ExprTrait, Iden, Query, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::book::{dto::GetBookOutput, ports::BookRepository},
    domain::library::{
        book::{
            entity::Book,
            value_objects::{BookId, BookTitle},
        },
    },
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::{Books, FULL_BOOKS_TABLE_COLUMNS, INSERT_BOOKS_COLUMNS},
        sqlite::{
            join_timestamp_ns, split_system_time, SqliteExecutor, SqliteRowExt, VersionedRecord, map_database_error, map_invalid_data,
            message_contains_columns,
        },
    },
};

#[derive(Clone)]
pub struct SqliteBookRepository {
    db: SqliteExecutor,
}

impl SqliteBookRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            db: SqliteExecutor::new(pool),
        }
    }
}

#[async_trait]
impl BookRepository for SqliteBookRepository {
    async fn create(&self, book: &Book) -> Result<(), RepositoryError> {
        let created_at = split_system_time(book.created_at());
        let updated_at = split_system_time(book.updated_at());

        let (sql, values) = Query::insert()
            .into_table(Books::Table)
            .columns(INSERT_BOOKS_COLUMNS)
            .values_panic([
                Expr::val(book.id().to_string()),
                Expr::val(book.title().to_string()),
                Expr::val(book.hash().to_hex().to_string()),
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

    async fn delete(&self, id: &BookId) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Books::Table)
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &BookId) -> Result<VersionedRecord<Book>, RepositoryError> {
        let (sql, values) = Query::select()
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .from(Books::Table)
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .fetch_optional(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await
    }

    async fn commit(&self, book: VersionedRecord<Book>) -> Result<(), RepositoryError> {
        let updated_at = split_system_time(book.updated_at());
        let (sql, values) = Query::update()
            .table(Books::Table)
            .values([
                (Books::Title, book.title().to_string().into()),
                (Books::UpdatedAtHigh, updated_at.0.into()),
                (Books::UpdatedAtLow, updated_at.1.into()),
                (Books::Version, (book.version + 1).into()),
            ])
            .and_where(Expr::col(Books::Id).eq(book.id().to_string()))
            .and_where(Expr::col(Books::Version).eq(book.version))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await
    }

    async fn list(
        &self,
        book_ids: Vec<BookId>,
    ) -> Result<Vec<GetBookOutput>, RepositoryError> {
        let mut query = Query::select();
        query
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .from(Books::Table);

        if !book_ids.is_empty() {
            query.and_where(Expr::col(Books::Id).is_in(book_ids.iter().map(ToString::to_string)));
        }

        let (sql, values) = query.build_sqlx(SqliteQueryBuilder);
        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for VersionedRecord<Book> {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(VersionedRecord {
            value: Book::reconstitute(
                row.get_uuid(&Books::Id.to_string())?,
                BookTitle::parse(row.get_string::<String>(&Books::Title.to_string())?)
                    .map_err(map_invalid_data)?,
                row.get_hash(&Books::Hash.to_string())?,
                join_timestamp_ns(row.get(&Books::CreatedAtHigh.to_string())?, row.get(&Books::CreatedAtLow.to_string())?),
                join_timestamp_ns(row.get(&Books::UpdatedAtHigh.to_string())?, row.get(&Books::UpdatedAtLow.to_string())?),
            ),
            version: row.get(&Books::Version.to_string())?,
        })
    }
}

impl TryFrom<&SqliteRow> for GetBookOutput {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(GetBookOutput {
            id: row.get_string(&Books::Id.to_string())?,
            title: row.get_string(&Books::Title.to_string())?,
            hash: row.get_string(&Books::Hash.to_string())?,
            file_type: String::new(),
            created_at: join_timestamp_ns(row.get(&Books::CreatedAtHigh.to_string())?, row.get(&Books::CreatedAtLow.to_string())?),
            updated_at: join_timestamp_ns(row.get(&Books::UpdatedAtHigh.to_string())?, row.get(&Books::UpdatedAtLow.to_string())?),
        })
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |database_error| {
        let is_root_book_title_conflict = message_contains_columns(
            database_error.message(),
            Books::Table,
            [Books::Title],
        );

        if database_error.is_unique_constraint() && is_root_book_title_conflict {
            Some(RepositoryError::BookTitleConflict)
        } else {
            None
        }
    })
}
