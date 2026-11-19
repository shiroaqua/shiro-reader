use std::collections::HashSet;

use toasty::{Db, Executor, Transaction};

use crate::{
    domain::{
        ChangeTarget, EntityCreate,
        library::{
            book::{
                entity::Book,
                value_objects::{BookId, BookStatus},
            },
            tag::entity::BookTag,
        },
    },
    infrastructure::db::sqlite::sqlite_error,
};

#[derive(Debug, thiserror::Error)]
pub enum BookRepositoryError {
    #[error("book not found")]
    NotFound,

    #[error("book title conflict")]
    TitleConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct BookRepository {
    db: Db,
}

impl BookRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create(&mut self, book: Book) -> Result<Book, BookRepositoryError> {
        book.create()
            .exec(&mut self.db)
            .await
            .map_err(map_book_db_error)
    }

    pub async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        find_book_by_id(&mut self.db, id).await
    }

    pub async fn list_active(&mut self, ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        let books = if ids.is_empty() {
            Book::all()
                .exec(&mut self.db)
                .await
                .map_err(map_book_db_error)?
        } else {
            find_books_by_ids(&mut self.db, ids).await?
        };
        Ok(books
            .into_iter()
            .filter(|book| book.status() == BookStatus::Active)
            .collect())
    }

    pub async fn list_trashed(&mut self) -> Result<Vec<Book>, BookRepositoryError> {
        Ok(Book::all()
            .exec(&mut self.db)
            .await
            .map_err(map_book_db_error)?
            .into_iter()
            .filter(|book| book.status() != BookStatus::Active)
            .collect())
    }

    pub async fn begin_transaction(&mut self) -> Result<BookTransaction<'_>, BookRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_book_db_error)?;
        Ok(BookTransaction { tx })
    }
}

pub struct BookTransaction<'a> {
    tx: Transaction<'a>,
}

impl BookTransaction<'_> {
    pub async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        find_book_by_id(&mut self.tx, id).await
    }

    pub async fn find_by_ids(&mut self, ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        find_books_by_ids(&mut self.tx, ids).await
    }

    pub async fn list_active_titles(&mut self) -> Result<HashSet<String>, BookRepositoryError> {
        Ok(Book::all()
            .exec(&mut self.tx)
            .await
            .map_err(map_book_db_error)?
            .into_iter()
            .filter_map(|book| {
                (book.status() == BookStatus::Active)
                    .then(|| -> String { book.into_inner().title.into() })
            })
            .collect())
    }

    pub async fn delete_books(&mut self, books: Vec<Book>) -> Result<(), BookRepositoryError> {
        let mut iter = books.into_iter();
        let Some(first) = iter.next() else {
            return Ok(());
        };
        if iter.len() == 0 {
            return first
                .delete()
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error);
        }
        let ids = std::iter::once(first)
            .chain(iter)
            .map(|book| book.into_inner().id.into())
            .collect::<Vec<uuid::Uuid>>();
        Book::filter(Book::fields().id().inner().in_list(ids))
            .delete()
            .exec(&mut self.tx)
            .await
            .map_err(map_book_db_error)
    }

    pub async fn list_tag_bindings(
        &mut self,
        book_ids: &[BookId],
    ) -> Result<Vec<BookTag>, BookRepositoryError> {
        match book_ids {
            [] => Ok(Vec::new()),
            [id] => BookTag::filter(BookTag::fields().book_id().eq(id.clone()))
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error),
            ids => BookTag::filter(BookTag::fields().book_id().inner().in_list(book_uuid_list(ids)))
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error),
        }
    }

    pub async fn commit(self) -> Result<(), BookRepositoryError> {
        self.tx.commit().await.map_err(map_book_db_error)
    }
}

impl ChangeTarget<Book> for BookTransaction<'_> {
    type Error = BookRepositoryError;

    fn executor(&mut self) -> &mut dyn toasty::codegen_support::Executor {
        &mut self.tx
    }

    fn map_error(error: toasty::Error) -> Self::Error {
        map_book_db_error(error)
    }
}

impl ChangeTarget<BookTag> for BookTransaction<'_> {
    type Error = BookRepositoryError;

    fn executor(&mut self) -> &mut dyn toasty::codegen_support::Executor {
        &mut self.tx
    }

    fn map_error(error: toasty::Error) -> Self::Error {
        map_book_db_error(error)
    }
}

async fn find_book_by_id(
    executor: &mut impl Executor,
    id: &BookId,
) -> Result<Book, BookRepositoryError> {
    Book::get_by_id(executor, id)
        .await
        .map_err(map_book_db_error)
}

async fn find_books_by_ids(
    executor: &mut impl Executor,
    ids: &[BookId],
) -> Result<Vec<Book>, BookRepositoryError> {
    match ids {
        [] => Ok(Vec::new()),
        [id] => Book::filter(Book::fields().id().eq(id.clone()))
            .exec(executor)
            .await
            .map_err(map_book_db_error),
        ids => Book::filter(Book::fields().id().inner().in_list(book_uuid_list(ids)))
            .exec(executor)
            .await
            .map_err(map_book_db_error),
    }
}

fn book_uuid_list(ids: &[BookId]) -> Vec<uuid::Uuid> {
    ids.iter().map(|id| *id.as_ref()).collect()
}

fn map_book_db_error(error: toasty::Error) -> BookRepositoryError {
    if error.is_record_not_found() {
        return BookRepositoryError::NotFound;
    }
    if let Some(rusqlite::Error::SqliteFailure(code, message)) = sqlite_error(&error)
        && code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
        && message
            .as_deref()
            .is_some_and(|msg| msg.contains("books.title"))
    {
        return BookRepositoryError::TitleConflict;
    }
    BookRepositoryError::Storage(error.into())
}
