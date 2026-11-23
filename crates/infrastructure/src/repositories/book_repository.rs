use std::collections::HashSet;

use shiro_application::library::ports::{BookRepository, BookRepositoryError, BookTransaction};
use shiro_domain::library::{
    book::{
        entity::{Book, BookChange, BookFields},
        value_objects::{BookId, BookStatus},
    },
    tag::entity::{BookTag, BookTagChange, BookTagFields},
};
use toasty::{Db, Executor, Transaction};
use uuid::Uuid;

use crate::db::{
    models::{BookRecord, BookTagRecord},
    scalars::DbTimestamp,
    sqlite::sqlite_error,
};

#[derive(Clone)]
pub struct SqliteBookRepository {
    db: Db,
}

impl SqliteBookRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl BookRepository for SqliteBookRepository {
    type Tx<'a> = SqliteBookTransaction<'a>;

    async fn create(&mut self, book: Book) -> Result<Book, BookRepositoryError> {
        let record = BookRecord::from(book)
            .into_create()
            .exec(&mut self.db)
            .await
            .map_err(map_book_db_error)?;
        Book::try_from(record).map_err(BookRepositoryError::Storage)
    }

    async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        find_book_by_id(&mut self.db, id).await
    }

    async fn list_active(&mut self, ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        let books = if ids.is_empty() {
            BookRecord::all()
                .exec(&mut self.db)
                .await
                .map_err(map_book_db_error)?
                .into_iter()
                .map(Book::try_from)
                .collect::<Result<Vec<_>, _>>()
                .map_err(BookRepositoryError::Storage)?
        } else {
            find_books_by_ids(&mut self.db, ids).await?
        };
        Ok(books
            .into_iter()
            .filter(|book| book.status() == BookStatus::Active)
            .collect())
    }

    async fn list_trashed(&mut self) -> Result<Vec<Book>, BookRepositoryError> {
        let books = BookRecord::all()
            .exec(&mut self.db)
            .await
            .map_err(map_book_db_error)?
            .into_iter()
            .map(Book::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(BookRepositoryError::Storage)?;
        Ok(books
            .into_iter()
            .filter(|book| book.status() != BookStatus::Active)
            .collect())
    }

    async fn begin_transaction(
        &mut self,
    ) -> Result<SqliteBookTransaction<'_>, BookRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_book_db_error)?;
        Ok(SqliteBookTransaction { tx })
    }
}

pub struct SqliteBookTransaction<'a> {
    tx: Transaction<'a>,
}

impl BookTransaction for SqliteBookTransaction<'_> {
    async fn find_by_id(&mut self, id: &BookId) -> Result<Book, BookRepositoryError> {
        find_book_by_id(&mut self.tx, id).await
    }

    async fn find_by_ids(&mut self, ids: &[BookId]) -> Result<Vec<Book>, BookRepositoryError> {
        find_books_by_ids(&mut self.tx, ids).await
    }

    async fn list_active_titles(&mut self) -> Result<HashSet<String>, BookRepositoryError> {
        Ok(BookRecord::all()
            .exec(&mut self.tx)
            .await
            .map_err(map_book_db_error)?
            .into_iter()
            .filter_map(|record| record.deleted_at.is_none().then_some(record.title))
            .collect())
    }

    async fn apply_book_change(
        &mut self,
        change: BookChange<'_>,
    ) -> Result<(), BookRepositoryError> {
        if change.is_empty() {
            return Ok(());
        }
        let entity = change.entity();
        let fields = change.fields();
        let mut update =
            BookRecord::filter(BookRecord::fields().id().eq(entity.id().as_uuid())).update();
        if fields.contains(BookFields::TITLE) {
            update.set_title(entity.title().as_str().to_owned());
        }
        if fields.contains(BookFields::UPDATED_AT) {
            update.set_updated_at(DbTimestamp(entity.updated_at()));
        }
        if fields.contains(BookFields::DELETED_AT) {
            update.set_deleted_at(entity.deleted_at().map(DbTimestamp));
        }
        update.exec(&mut self.tx).await.map_err(map_book_db_error)
    }

    async fn delete_books(&mut self, books: Vec<Book>) -> Result<(), BookRepositoryError> {
        let ids: Vec<Uuid> = books.into_iter().map(|book| book.id().as_uuid()).collect();
        match ids.as_slice() {
            [] => Ok(()),
            [id] => BookRecord::filter(BookRecord::fields().id().eq(*id))
                .delete()
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error),
            _ => BookRecord::filter(BookRecord::fields().id().in_list(ids))
                .delete()
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error),
        }
    }

    async fn list_tag_bindings(
        &mut self,
        book_ids: &[BookId],
    ) -> Result<Vec<BookTag>, BookRepositoryError> {
        let records = match book_ids {
            [] => Vec::new(),
            [id] => BookTagRecord::filter(BookTagRecord::fields().book_id().eq(id.as_uuid()))
                .exec(&mut self.tx)
                .await
                .map_err(map_book_db_error)?,
            ids => BookTagRecord::filter(
                BookTagRecord::fields()
                    .book_id()
                    .in_list(book_uuid_list(ids)),
            )
            .exec(&mut self.tx)
            .await
            .map_err(map_book_db_error)?,
        };
        Ok(records.into_iter().map(BookTag::from).collect())
    }

    async fn apply_tag_binding_change(
        &mut self,
        change: BookTagChange<'_>,
    ) -> Result<(), BookRepositoryError> {
        apply_book_tag_change(&mut self.tx, change)
            .await
            .map_err(map_book_db_error)
    }

    async fn commit(self) -> Result<(), BookRepositoryError> {
        self.tx.commit().await.map_err(map_book_db_error)
    }
}

pub(crate) async fn apply_book_tag_change(
    executor: &mut impl Executor,
    change: BookTagChange<'_>,
) -> toasty::Result<()> {
    if change.is_empty() {
        return Ok(());
    }
    let entity = change.entity();
    let fields = change.fields();
    let filter = BookTagRecord::fields()
        .tag_id()
        .eq(entity.tag_id().as_uuid())
        .and(
            BookTagRecord::fields()
                .book_id()
                .eq(entity.book_id().as_uuid()),
        );
    let mut update = BookTagRecord::filter(filter).update();
    if fields.contains(BookTagFields::VALUE) {
        update.set_value(entity.value().map(str::to_owned));
    }
    if fields.contains(BookTagFields::UPDATED_AT) {
        update.set_updated_at(DbTimestamp(entity.updated_at()));
    }
    if fields.contains(BookTagFields::DELETED_AT) {
        update.set_deleted_at(entity.deleted_at().map(DbTimestamp));
    }
    update.exec(executor).await
}

async fn find_book_by_id(
    executor: &mut impl Executor,
    id: &BookId,
) -> Result<Book, BookRepositoryError> {
    let record = BookRecord::get_by_id(executor, id.as_uuid())
        .await
        .map_err(map_book_db_error)?;
    Book::try_from(record).map_err(BookRepositoryError::Storage)
}

async fn find_books_by_ids(
    executor: &mut impl Executor,
    ids: &[BookId],
) -> Result<Vec<Book>, BookRepositoryError> {
    let records = match ids {
        [] => Vec::new(),
        [id] => BookRecord::filter(BookRecord::fields().id().eq(id.as_uuid()))
            .exec(executor)
            .await
            .map_err(map_book_db_error)?,
        ids => BookRecord::filter(BookRecord::fields().id().in_list(book_uuid_list(ids)))
            .exec(executor)
            .await
            .map_err(map_book_db_error)?,
    };
    records
        .into_iter()
        .map(Book::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(BookRepositoryError::Storage)
}

fn book_uuid_list(ids: &[BookId]) -> Vec<Uuid> {
    ids.iter().map(BookId::as_uuid).collect()
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
