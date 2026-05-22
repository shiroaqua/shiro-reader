use async_trait::async_trait;
use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::book::ports::BookRepository,
    domain::library::book::{
        entity::Book,
        value_objects::{BookId, BookTitle},
    },
    infrastructure::repositories::{
        errors::RepositoryError,
        sqlite::{SqliteExecutor, SqliteRowExt, map_database_error, map_invalid_data},
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

        self.db.execute(&sql, values, map_sqlx_error).await?;
        Ok(book)
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

    async fn rename(&self, id: &BookId, new_title: &BookTitle) -> Result<(), RepositoryError> {
        let (sql, values) = Query::update()
            .table(Books::Table)
            .value(Books::Title, new_title.as_str())
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .execute_affected(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await?;

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

        self.db
            .fetch_optional(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await
    }
}

impl TryFrom<&SqliteRow> for Book {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Book {
            id: row.get_uuid("id")?,
            title: BookTitle::from(row.get::<String>("title")?),
            hash: parse_hash(row.get::<String>("hash")?)?,
            bookshelf_id: row.get_uuid("bookshelf_id")?,
            folder_id: row.get_optional_uuid("folder_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
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
    map_database_error(error, |message| {
        let is_unique_failed = message.contains("UNIQUE constraint failed");
        let is_book_title_conflict = message
            .contains("books.bookshelf_id, books.folder_id, books.title")
            || message.contains("books.bookshelf_id, books.title");

        if is_unique_failed && is_book_title_conflict {
            Some(RepositoryError::BookTitleConflict)
        } else if message.contains("FOREIGN KEY constraint failed") {
            if message.contains("bookshel") {
                Some(RepositoryError::BookshelfNotFound)
            } else if message.contains("folder") {
                Some(RepositoryError::FolderNotFound)
            } else {
                None
            }
        } else {
            None
        }
    })
}

fn parse_hash(value: String) -> Result<blake3::Hash, RepositoryError> {
    blake3::Hash::from_hex(value).map_err(map_invalid_data)
}
