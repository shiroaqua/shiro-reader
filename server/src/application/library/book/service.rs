use std::sync::Arc;

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;
use toasty::Db;

use crate::{
    application::{
        MapDbResult,
        errors::MapToastyError,
        library::{
            book::{
                commands::{
                    BookIdCommand, BookIdsCommand, CreateBookCommand, DeleteBookCommand,
                    DeleteBooksCommand, GetBookCoverCommand, RenameBookCommand,
                },
                dto::{CreateBookOutput, GetBookOutput, TrashedBookOutput},
                file::{errors::BookFileApplicationError, service::BookFileService},
                queries::GetBooksQuery,
            },
            errors::LibraryApplicationError,
            book::errors::BookApplicationError,
        },
    },
    domain::{
        EntityCreate, EntityId, RequiresVerificationEntity,
        library::{
            book::{entity::Book, value_objects::{BookId, BookStatus, BookTitle}},
            tag::{entity::BookTag, value_objects::BookTagStatus},
        },
    },
};

#[derive(new)]
pub struct BookService {
    db: Db,
    bookfile: Arc<BookFileService>,
}

async fn available_restore_title(
    tx: &mut impl toasty::Executor,
    original: String,
) -> Result<BookTitle, LibraryApplicationError> {
    let occupied = Book::all().exec(tx).await.map_err(Book::map_db_err)?
        .into_iter().filter(|book| book.status() == BookStatus::Active)
        .map(|book| book.title().to_string()).collect::<std::collections::HashSet<_>>();
    if !occupied.contains(&original) {
        return Ok(BookTitle::parse(original)?);
    }
    for suffix in 1..=1024 {
        let candidate = format!("{original} ({suffix})");
        if !occupied.contains(&candidate) {
            return Ok(BookTitle::parse(candidate)?);
        }
    }
    Err(BookApplicationError::TitleConflict.into())
}

impl BookService {
    pub async fn create_book(
        &mut self,
        command: CreateBookCommand,
    ) -> Result<CreateBookOutput, LibraryApplicationError> {
        let book = Book::new(
            BookId::new(),
            BookTitle::parse(command.title)?,
            self.bookfile.parse_existing_hash(&command.hash)?,
        )?;
        let book = book.create().exec(&mut self.db).await.map_result()?;

        Ok(CreateBookOutput {
            id: book.id().to_string(),
            title: book.title().to_string(),
            hash: book.hash().to_hex().to_string(),
            file_type: self
                .bookfile
                .get_book_type(&book.hash())
                .ok_or(BookFileApplicationError::Unsupported)?
                .to_string(),
            created_at: book.created_at(),
            updated_at: book.updated_at(),
        })
    }

    pub async fn delete_book(
        &mut self,
        command: DeleteBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        self.delete_books(DeleteBooksCommand { ids: vec![command.id] }).await
    }

    pub async fn delete_books(
        &mut self,
        command: DeleteBooksCommand,
    ) -> Result<(), LibraryApplicationError> {
        let ids = command.ids.iter().map(BookId::parse).collect::<Result<Vec<_>, _>>()?;
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        for id in &ids {
            Book::get_by_id(&mut tx, id).await.map_result()?;
        }
        for id in ids {
            let mut book = Book::get_by_id(&mut tx, &id).await.map_result()?;
            book.mark_deleted()?.exec(&mut tx).await.map_err(Book::map_db_err)?;
            let bindings = BookTag::filter(BookTag::fields().book_id().eq(id.clone()))
                .exec(&mut tx).await.map_err(BookTag::map_db_err)?;
            for mut binding in bindings {
                binding.mark_deleted()?.exec(&mut tx)
                    .await.map_err(BookTag::map_db_err)?;
            }
        }
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn restore_book(
        &mut self,
        command: BookIdCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let mut book = Book::get_by_id(&mut tx, &id).await.map_result()?;
        let restored_title = available_restore_title(&mut tx, book.title().to_string()).await?;
        if restored_title != *book.title() {
            book.rename(restored_title)?.exec(&mut tx).await.map_err(Book::map_db_err)?;
        }
        book.restore()?.exec(&mut tx).await.map_err(Book::map_db_err)?;
        let bindings = BookTag::filter(BookTag::fields().book_id().eq(id.clone()))
            .exec(&mut tx).await.map_err(BookTag::map_db_err)?;
        for mut binding in bindings {
            if binding.status() != BookTagStatus::Active {
                binding.restore()?.exec(&mut tx).await.map_err(BookTag::map_db_err)?;
            }
        }
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn permanently_delete_books(
        &mut self,
        command: BookIdsCommand,
    ) -> Result<(), LibraryApplicationError> {
        let ids = command.ids.iter().map(BookId::parse).collect::<Result<Vec<_>, _>>()?;
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        for id in &ids {
            if Book::get_by_id(&mut tx, id).await.map_result()?.status() == BookStatus::Active {
                return Err(BookApplicationError::NotFound.into());
            }
        }
        for id in ids {
            Book::get_by_id(&mut tx, &id).await.map_result()?.delete()
                .exec(&mut tx).await.map_err(Book::map_db_err)?;
        }
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn get_trash(
        &mut self,
    ) -> Result<Vec<TrashedBookOutput>, LibraryApplicationError> {
        let trashed = Book::all().exec(&mut self.db).await.map_err(Book::map_db_err)?
            .into_iter().filter(|book| book.status() != BookStatus::Active).collect::<Vec<_>>();
        let mut output = Vec::new();
        for book in trashed {
            let deleted_at = match book.status() {
                BookStatus::Active => continue,
                BookStatus::Deleted { deleted_at } => deleted_at,
            };
            let hash = book.hash().to_hex().to_string();
            let file_hash = self.bookfile.parse_existing_hash(&hash)?;
            let file_type = self.bookfile.get_book_type(&file_hash)
                .ok_or(BookFileApplicationError::Unsupported)?.to_string();
            output.push(TrashedBookOutput {
                book: GetBookOutput {
                    id: book.id().to_string(), title: book.title().to_string(), hash,
                    file_type, created_at: book.created_at(), updated_at: book.updated_at(),
                },
                deleted_at,
            });
        }
        Ok(output)
    }

    pub async fn rename_book(
        &mut self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;

        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        if Book::get_by_id(&mut tx, &id).await.map_result()?.status() != BookStatus::Active {
            return Err(BookApplicationError::NotFound.into());
        }
        let mut book = Book::get_by_id(&mut tx, &id).await.map_result()?;
        book.rename(new_title)?
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?;
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn get_books(
        &mut self,
        query: GetBooksQuery,
    ) -> Result<Vec<GetBookOutput>, LibraryApplicationError> {
        let book_ids = query
            .book_ids
            .iter()
            .map(BookId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let mut books_query = Book::all();
        if !book_ids.is_empty() {
            books_query = books_query.filter(
                Book::fields().id().inner().in_list(
                    book_ids
                        .iter()
                        .map(|id| *id.as_ref())
                        .collect::<Vec<uuid::Uuid>>(),
                ),
            );
        }
        let mut books = books_query
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)?
            .into_iter()
            .filter(|book| book.status() == BookStatus::Active)
            .map(|book| {
                let book = book.verify()?;
                Ok(GetBookOutput {
                    id: book.id().to_string(),
                    title: book.title().to_string(),
                    hash: book.hash().to_hex().to_string(),
                    file_type: String::new(),
                    created_at: book.created_at(),
                    updated_at: book.updated_at(),
                })
            })
            .collect::<Result<Vec<_>, LibraryApplicationError>>()?;
        for book in &mut books {
            let hash = self.bookfile.parse_existing_hash(&book.hash)?;
            book.file_type = self
                .bookfile
                .get_book_type(&hash)
                .ok_or(BookFileApplicationError::Unsupported)?
                .to_string();
        }
        Ok(books)
    }

    pub async fn get_cover(
        &mut self,
        command: GetBookCoverCommand,
    ) -> Result<
        (impl Stream<Item = std::io::Result<Bytes>> + 'static, String),
        LibraryApplicationError,
    > {
        let id = BookId::parse(command.id)?;
        if Book::get_by_id(&mut self.db, &id).await.map_result()?.status() != BookStatus::Active {
            return Err(BookApplicationError::NotFound.into());
        }
        let book = Book::get_by_id(&mut self.db, &id).await.map_result()?;
        self.bookfile.download_book_cover_file(*book.hash()).await
    }
}
