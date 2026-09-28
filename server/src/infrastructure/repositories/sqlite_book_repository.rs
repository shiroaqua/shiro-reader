use async_trait::async_trait;
use sea_query::{Expr, ExprTrait, Iden, Query, SqliteQueryBuilder};
use sea_query_sqlx::SqlxBinder;
use sqlx::{SqlitePool, sqlite::SqliteRow};

use crate::{
    application::library::book::ports::BookRepository,
    domain::library::{
        book::{
            entity::Book,
            value_objects::{BookId, BookTitle},
        },
        bookshelf::value_objects::BookshelfId,
    },
    infrastructure::repositories::{
        errors::RepositoryError,
        idens::{Books, Bookshelves, FULL_BOOKS_TABLE_COLUMNS},
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
            .and_where(Expr::col(Bookshelves::Id).eq(book.bookshelf_id().to_string()))
            .limit(1)
            .build_sqlx(SqliteQueryBuilder);

        if !tx.fetch_exists(&sql, values, map_sqlx_error).await? {
            return Err(RepositoryError::BookshelfNotFound);
        }

        let (sql, values) = Query::insert()
            .into_table(Books::Table)
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .values_panic([
                book.id().to_string().into(),
                book.title().to_string().into(),
                book.hash().to_hex().to_string().into(),
                book.bookshelf_id().to_string().into(),
                book.created_at().into(),
                book.updated_at().into(),
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
    ) -> Result<Vec<Book>, RepositoryError> {
        let mut query = Query::select();
        query
            .columns(FULL_BOOKS_TABLE_COLUMNS)
            .from(Books::Table)
            .and_where(Expr::col(Books::BookshelfId).eq(bookshelf_id.to_string()));

        let (sql, values) = query.build_sqlx(SqliteQueryBuilder);

        self.db.fetch_all(&sql, values, map_sqlx_error).await
    }
}

impl TryFrom<&SqliteRow> for Book {
    type Error = RepositoryError;

    fn try_from(row: &SqliteRow) -> Result<Self, Self::Error> {
        Ok(Book::new(
            row.get_uuid(&Books::Id.to_string())?,
            row.get_string(&Books::Title.to_string())?,
            row.get_hash(&Books::Hash.to_string())?,
            row.get_uuid(&Books::BookshelfId.to_string())?,
            row.get(&Books::CreatedAt.to_string())?,
            row.get(&Books::UpdatedAt.to_string())?,
        ))
    }
}

fn map_sqlx_error(error: sqlx::Error) -> RepositoryError {
    map_database_error(error, |database_error| {
        let is_root_book_title_conflict = message_contains_columns(
            database_error.message(),
            Books::Table,
            [Books::BookshelfId, Books::Title],
        );

        if database_error.is_unique_constraint()
            && is_root_book_title_conflict
        {
            Some(RepositoryError::BookTitleConflict)
        } else {
            None
        }
    })
}
