use async_trait::async_trait;
use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::{sqlite::SqliteRow, Row, SqlitePool};

use crate::{
    application::library::book::ports::BookRepository,
    domain::library::{
        book::{
            entity::Book,
            value_objects::{BookId, BookTitle},
        },
        bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId},
    },
    infrastructure::repositories::errors::RepositoryError,
};

#[derive(Clone)]
pub struct SqliteBookRepository {
    pool: SqlitePool,
}

impl SqliteBookRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn execute(
        &self,
        sql: &str,
        values: sea_query_binder::SqlxValues,
    ) -> Result<sqlx::sqlite::SqliteQueryResult, RepositoryError> {
        let result = sqlx::query_with(sql, values)
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(RepositoryError::BookNotFound);
        }

        Ok(result)
    }
}

#[async_trait]
impl BookRepository for SqliteBookRepository {
    async fn create(&self, book: Book) -> Result<Book, RepositoryError> {
        let (sql, values) = Query::insert()
            .into_table(Books::Table)
            .columns([
                Books::Id,
                Books::Title,
                Books::Hash,
                Books::BookshelfId,
                Books::FolderId,
                Books::CreatedAt,
                Books::UpdatedAt,
            ])
            .values_panic([
                book.id.to_string().into(),
                book.title.to_string().into(),
                book.hash.to_hex().to_string().into(),
                book.bookshelf_id.to_string().into(),
                book.folder_id.as_ref().map(ToString::to_string).into(),
                book.created_at.into(),
                book.updated_at.into(),
            ])
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;
        Ok(book)
    }

    async fn delete(&self, id: &BookId) -> Result<(), RepositoryError> {
        let (sql, values) = Query::delete()
            .from_table(Books::Table)
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.execute(&sql, values).await?;
        Ok(())
    }

    async fn find_by_id(&self, id: &BookId) -> Result<Book, RepositoryError> {
        let (sql, values) = Query::select()
            .columns([
                Books::Id,
                Books::Title,
                Books::Hash,
                Books::BookshelfId,
                Books::FolderId,
                Books::CreatedAt,
                Books::UpdatedAt,
            ])
            .from(Books::Table)
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        Ok(Book::try_from(
            &sqlx::query_with(&sql, values)
                .fetch_optional(&self.pool)
                .await
                .map_err(map_sqlx_error)?
                .ok_or(RepositoryError::BookNotFound)?,
        )?)
    }
}

impl TryFrom<&SqliteRow> for Book {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        let id = row.try_get::<String, _>("id").map_err(map_sqlx_error)?;
        let title =row.try_get::<String, _>("title").map_err(map_sqlx_error)?;
        let hash = row.try_get::<String, _>("hash").map_err(map_sqlx_error)?;
        let bookshelf_id = row.try_get::<String, _>("bookshelf_id").map_err(map_sqlx_error)?;
        let folder_id = row.try_get::<Option<String>, _>("folder_id").map_err(map_sqlx_error)?;
        let created_at = row.try_get("created_at").map_err(map_sqlx_error)?;
        let updated_at = row.try_get("updated_at").map_err(map_sqlx_error)?;

        Ok(Book {
            id: BookId::from(uuid::Uuid::parse_str(&id).unwrap()),
            title: BookTitle::from(title),
            hash: blake3::Hash::from_hex(hash).unwrap(),
            bookshelf_id: BookshelfId::from(uuid::Uuid::parse_str(&bookshelf_id).unwrap()),
            folder_id: folder_id.map(|id|FolderId::from(uuid::Uuid::parse_str(&id).unwrap())),
            created_at: created_at,
            updated_at: updated_at,
        })
    }
}

#[derive(Iden)]
enum Books {
    #[iden = "books"]
    Table,
    Id,
    Title,
    Hash,
    BookshelfId,
    FolderId,
    CreatedAt,
    UpdatedAt,
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    match &error {
        sqlx::Error::Database(database_error) => {
            let message = database_error.message();
            let is_unique_failed = message.contains("UNIQUE constraint failed");
            let is_book_title_conflict = message
                .contains("books.bookshelf_id, books.folder_id, books.title")
                || message.contains("books.bookshelf_id, books.title");

            if is_unique_failed && is_book_title_conflict {
                RepositoryError::BookTitleConflict
            } else if message.contains("FOREIGN KEY constraint failed") {
                RepositoryError::BookLocationNotFound
            } else {
                RepositoryError::Storage(anyhow::Error::new(error))
            }
        }
        _ => RepositoryError::Storage(anyhow::Error::new(error)),
    }
}
