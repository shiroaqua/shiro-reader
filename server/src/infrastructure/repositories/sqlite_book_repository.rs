use async_trait::async_trait;
use sea_query::{Expr, Iden, Query, SqliteQueryBuilder};
use sea_query_binder::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::book::ports::BookRepository,
    domain::library::{book::{
        entity::Book,
        value_objects::{BookId, BookTitle},
    }, bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId}},
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::{Books, Bookshelves, FULL_BOOKS_TABLE_COLUMNS, Folders},
        sqlite::{SqliteExecutor, SqliteRowExt, map_database_error, message_contains_columns},
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
            .columns(FULL_BOOKS_TABLE_COLUMNS)
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
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .from(Books::Table)
            .and_where(Expr::col(Books::Id).eq(id.to_string()))
            .build_sqlx(SqliteQueryBuilder);

        self.db
            .fetch_optional(&sql, values, RepositoryError::BookNotFound, map_sqlx_error)
            .await
    }

    async fn list(
        &self,
        bookshelf_id: &BookshelfId,
        folder_id: Option<&FolderId>,
    ) -> Result<Vec<Book>, RepositoryError> {
        let mut query = Query::select();
        query
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .from(Books::Table)
            .and_where(Expr::col(Books::BookshelfId).eq(bookshelf_id.to_string()));
            
        if let Some(folder_id) = folder_id {
            query.and_where(Expr::col(Books::FolderId).eq(folder_id.to_string()));
        }
        else {
            query.and_where(Expr::col(Books::FolderId).is_null());
        }

        let (sql, values) = query.build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for Book {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {

        Ok(Book {
            id: row.get_uuid(&Books::Id.to_string())?,
            title: row.get_string(&Books::Title.to_string())?,
            hash: row.get_hash(&Books::Hash.to_string())?,
            bookshelf_id: row.get_uuid(&Books::BookshelfId.to_string())?,
            folder_id: row.get_optional_uuid(&Books::FolderId.to_string())?,
            created_at: row.get(&Books::CreatedAt.to_string())?,
            updated_at: row.get(&Books::UpdatedAt.to_string())?,
        })
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |database_error| {
        let is_folder_book_title_conflict = message_contains_columns(
            database_error.message(),
            Books::Table,
            [Books::BookshelfId, Books::FolderId, Books::Title],
        );
        let is_root_book_title_conflict = message_contains_columns(
            database_error.message(),
            Books::Table,
            [Books::BookshelfId, Books::Title],
        );

        if database_error.is_unique_constraint()
            && (is_folder_book_title_conflict || is_root_book_title_conflict)
        {
            Some(RepositoryError::BookTitleConflict)
        } else {
            None
        }
    })
}
