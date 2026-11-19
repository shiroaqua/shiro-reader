use std::sync::Arc;

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;
use toasty::Db;

use crate::{
    application::{
        errors::MapToastyError,
        library::{
            book::errors::BookApplicationError,
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
        },
    },
    domain::{
        EntityCreate, EntityId,
        library::{
            book::{
                entity::Book,
                errors::BookDomainError,
                value_objects::{BookId, BookStatus, BookTitle},
            },
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
    original: &BookTitle,
) -> Result<BookTitle, LibraryApplicationError> {
    let occupied = Book::all()
        .exec(tx)
        .await
        .map_err(Book::map_db_err)?
        .into_iter()
        .filter_map(|book| {
            (book.status() == BookStatus::Active)
                .then(|| -> String { book.into_inner().title.into() })
        })
        .collect::<std::collections::HashSet<_>>();
    let original_title: &String = original.as_ref();
    if !occupied.contains(original_title) {
        return Ok(original.clone());
    }
    for suffix in 1..=1024 {
        let candidate = format!("{original_title} ({suffix})");
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
        let book = book
            .create()
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)?
            .into_inner();

        Ok(CreateBookOutput {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: self
                .bookfile
                .get_book_type(book.hash.as_hash())
                .ok_or(BookFileApplicationError::Unsupported)?
                .to_string(),
            created_at: book.created_at,
            updated_at: book.updated_at,
        })
    }

    pub async fn delete_book(
        &mut self,
        command: DeleteBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        self.delete_books(DeleteBooksCommand {
            ids: vec![command.id],
        })
        .await
    }

    pub async fn delete_books(
        &mut self,
        command: DeleteBooksCommand,
    ) -> Result<(), LibraryApplicationError> {
        let ids = command
            .ids
            .iter()
            .map(|id| BookId::parse(id).map(Into::into))
            .collect::<Result<Vec<uuid::Uuid>, _>>()?;
        if ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let mut books = Book::filter(Book::fields().id().inner().in_list(ids.clone()))
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?
            .into_iter()
            .map(|book| Ok((*book.id().as_ref(), book)))
            .collect::<Result<std::collections::HashMap<_, _>, LibraryApplicationError>>()?;
        for id in &ids {
            if !books.contains_key(id) {
                return Err(BookApplicationError::NotFound.into());
            }
        }
        for id in &ids {
            books
                .get_mut(id)
                .expect("book existence checked")
                .mark_deleted()?
                .exec(&mut tx)
                .await
                .map_err(Book::map_db_err)?;
        }
        let bindings = BookTag::filter(BookTag::fields().book_id().inner().in_list(ids))
            .exec(&mut tx)
            .await
            .map_err(BookTag::map_db_err)?;
        for mut binding in bindings {
            binding
                .mark_deleted()?
                .exec(&mut tx)
                .await
                .map_err(BookTag::map_db_err)?;
        }
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn restore_book(
        &mut self,
        command: BookIdCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let mut book = Book::get_by_id(&mut tx, &id).await.map_err(Book::map_db_err)?;
        if book.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted.into());
        }
        let restored_title = available_restore_title(&mut tx, book.title()).await?;
        book.restore(restored_title)?
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?;
        let bindings = BookTag::filter(BookTag::fields().book_id().eq(id))
            .exec(&mut tx)
            .await
            .map_err(BookTag::map_db_err)?;
        for mut binding in bindings {
            if binding.status() != BookTagStatus::Active {
                binding
                    .restore()?
                    .exec(&mut tx)
                    .await
                    .map_err(BookTag::map_db_err)?;
            }
        }
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn permanently_delete_books(
        &mut self,
        command: BookIdsCommand,
    ) -> Result<(), LibraryApplicationError> {
        let ids = command
            .ids
            .iter()
            .map(|id| BookId::parse(id).map(Into::into))
            .collect::<Result<Vec<uuid::Uuid>, _>>()?;
        if ids.is_empty() {
            return Ok(());
        }
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let books = Book::filter(Book::fields().id().inner().in_list(ids.clone()))
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?;
        if books.len() != ids.len() {
            return Err(BookApplicationError::NotFound.into());
        }
        for book in books {
            if book.status() == BookStatus::Active {
                return Err(BookApplicationError::NotFound.into());
            }
        }
        Book::filter(Book::fields().id().inner().in_list(ids))
            .delete()
            .exec(&mut tx)
            .await
            .map_err(Book::map_db_err)?;
        tx.commit().await.map_err(Book::map_db_err)
    }

    pub async fn get_trash(&mut self) -> Result<Vec<TrashedBookOutput>, LibraryApplicationError> {
        Book::all()
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)?
            .into_iter()
            .filter_map(|book| {
                let BookStatus::Deleted { deleted_at } = book.status() else {
                    return None;
                };
                Some((|| {
                    let book = book.into_inner();
                    let hash = book.hash.as_hash().to_hex().to_string();
                    let file_hash = self.bookfile.parse_existing_hash(&hash)?;
                    let file_type = self
                        .bookfile
                        .get_book_type(&file_hash)
                        .ok_or(BookFileApplicationError::Unsupported)?
                        .to_string();
                    Ok(TrashedBookOutput {
                        book: GetBookOutput {
                            id: book.id.to_string(),
                            title: book.title.into(),
                            hash,
                            file_type,
                            created_at: book.created_at,
                            updated_at: book.updated_at,
                        },
                        deleted_at,
                    })
                })())
            })
            .collect::<Result<Vec<_>, LibraryApplicationError>>()
    }

    pub async fn rename_book(
        &mut self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;
        let mut tx = self.db.transaction().await.map_err(Book::map_db_err)?;
        let mut book = Book::get_by_id(&mut tx, &id).await.map_err(Book::map_db_err)?;
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
        let book_query = if query.book_ids.is_empty() {
            Book::all()
        } else {
            Book::all().filter(
                Book::fields().id().inner().in_list(
                    query
                        .book_ids
                        .iter()
                        .map(|id| BookId::parse(id).map(Into::into))
                        .collect::<Result<Vec<uuid::Uuid>, _>>()?,
                ),
            )
        };
        book_query
            .exec(&mut self.db)
            .await
            .map_err(Book::map_db_err)?
            .into_iter()
            .filter_map(|book| {
                (book.status() == BookStatus::Active).then(|| {
                    let book = book.into_inner();
                    let hash = book.hash.as_hash().to_hex().to_string();
                    let file_hash = self.bookfile.parse_existing_hash(&hash)?;

                    Ok(GetBookOutput {
                        id: book.id.to_string(),
                        title: book.title.into(),
                        hash,
                        file_type: self
                            .bookfile
                            .get_book_type(&file_hash)
                            .ok_or(BookFileApplicationError::Unsupported)?
                            .to_string(),
                        created_at: book.created_at,
                        updated_at: book.updated_at,
                    })
                })
            })
            .collect::<Result<Vec<_>, LibraryApplicationError>>()
    }

    pub async fn get_cover(
        &mut self,
        command: GetBookCoverCommand,
    ) -> Result<
        (impl Stream<Item = std::io::Result<Bytes>> + 'static, String),
        LibraryApplicationError,
    > {
        let id = BookId::parse(command.id)?;
        let book = Book::get_by_id(&mut self.db, &id).await.map_err(Book::map_db_err)?;
        self.bookfile.download_book_cover_file(*book.hash()).await
    }
}