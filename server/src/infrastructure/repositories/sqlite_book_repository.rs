use async_trait::async_trait;
use sea_query::{Expr, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::{sqlite::SqliteRow, SqlitePool};

use crate::{
    application::library::book::ports::BookRepository,
    domain::library::book::{
        entity::Book,
        value_objects::{BookId, BookTitle},
    },
    infrastructure::repositories::{
        errors::RepositoryError, idens::{Books, Bookshelves, Folders}, sqlite::{SqliteExecutor, SqliteRowExt, map_database_error}
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
        let mut tx = self.db.begin(map_sqlx_error).await?;
        let (sql, values) = Query::select()
            .expr(Expr::val(1))
            .from(Bookshelves::Table)
            .and_where(Expr::col(Bookshelves::Id).eq(book.bookshelf_id.to_string()))
            .limit(1)
            .build_sqlx(SqliteQueryBuilder);

        if !tx.fetch_exists(&sql, values, map_sqlx_error).await? {
            return Err(RepositoryError::BookshelfNotFound);
        }

        if let Some(folder_id) = &book.folder_id {
            let (sql, values) = Query::select()
                .expr(Expr::val(1))
                .from(Folders::Table)
                .and_where(Expr::col(Folders::Id).eq(folder_id.to_string()))
                .and_where(Expr::col(Folders::BookshelfId).eq(book.bookshelf_id.to_string()))
                .limit(1)
                .build_sqlx(SqliteQueryBuilder);

            if !tx.fetch_exists(&sql, values, map_sqlx_error).await? {
                return Err(RepositoryError::FolderNotFound);
            }
        }

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

        tx.execute(&sql, values, map_sqlx_error).await?;
        tx.commit(map_sqlx_error).await?;
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
            hash: row.get_hash("hash")?,
            bookshelf_id: row.get_uuid("bookshelf_id")?,
            folder_id: row.get_optional_uuid("folder_id")?,
            created_at: row.get("created_at")?,
            updated_at: row.get("updated_at")?,
        })
    }
}


fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |message| {
        let is_unique_failed = message.contains("UNIQUE constraint failed");
        let is_book_title_conflict = message
            .contains("books.bookshelf_id, books.folder_id, books.title")
            || message.contains("books.bookshelf_id, books.title");

        if is_unique_failed && is_book_title_conflict {
            Some(RepositoryError::BookTitleConflict)
        } else {
            None
        }
    })
}
