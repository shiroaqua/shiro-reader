use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bytes::Bytes;
use derive_new::new;
use futures_util::Stream;

use crate::{
    application::library::{
        book::{
            commands::{
                BookIdCommand, BookIdsCommand, CreateBookCommand, DeleteBookCommand,
                DeleteBooksCommand, GetBookCoverCommand, RenameBookCommand,
            },
            dto::{CreateBookOutput, GetBookOutput, TrashedBookOutput},
            errors::BookApplicationError,
            file::{errors::BookFileApplicationError, service::BookFileService},
            queries::GetBooksQuery,
        },
        errors::LibraryApplicationError,
    },
    domain::{
        EntityId,
        library::{
            book::{
                entity::Book,
                errors::BookDomainError,
                value_objects::{BookId, BookStatus, BookTitle},
            },
            tag::value_objects::BookTagStatus,
        },
    },
    infrastructure::repositories::book_repository::BookRepository,
};

#[derive(new)]
pub struct BookService {
    repository: BookRepository,
    bookfile: Arc<BookFileService>,
}

fn available_restore_title(
    occupied: &HashSet<String>,
    original: &BookTitle,
) -> Result<BookTitle, LibraryApplicationError> {
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
        let title = BookTitle::parse(command.title)?;
        let hash = self.bookfile.parse_existing_hash(&command.hash)?;
        let file_type = self
            .bookfile
            .get_book_type(&hash)
            .ok_or(BookFileApplicationError::Unsupported)?;
        let book = self
            .repository
            .create(Book::new(BookId::new(), title, hash)?)
            .await?;

        Ok((book, file_type).into())
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
            .map(BookId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            return Ok(());
        }

        let mut tx = self.repository.begin_transaction().await?;
        let mut books = tx
            .find_by_ids(&ids)
            .await?
            .into_iter()
            .map(|book| (book.id().clone(), book))
            .collect::<HashMap<_, _>>();
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
                .await?;
        }
        for mut binding in tx.list_tag_bindings(&ids).await? {
            binding.mark_deleted()?.exec(&mut tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn restore_book(
        &mut self,
        command: BookIdCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut book = tx.find_by_id(&id).await?;
        if book.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted.into());
        }
        let occupied = tx.list_active_titles().await?;
        let restored_title = available_restore_title(&occupied, book.title())?;
        book.restore(restored_title)?.exec(&mut tx).await?;
        for mut binding in tx.list_tag_bindings(std::slice::from_ref(&id)).await? {
            if binding.status() != BookTagStatus::Active {
                binding.restore()?.exec(&mut tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn permanently_delete_books(
        &mut self,
        command: BookIdsCommand,
    ) -> Result<(), LibraryApplicationError> {
        let ids = command
            .ids
            .iter()
            .map(BookId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        if ids.is_empty() {
            return Ok(());
        }

        let mut tx = self.repository.begin_transaction().await?;
        let books = tx.find_by_ids(&ids).await?;
        if books.len() != ids.len() {
            return Err(BookApplicationError::NotFound.into());
        }
        if books.iter().any(|book| book.status() == BookStatus::Active) {
            return Err(BookApplicationError::NotFound.into());
        }
        tx.delete_books(books).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_trash(&mut self) -> Result<Vec<TrashedBookOutput>, LibraryApplicationError> {
        self.repository
            .list_trashed()
            .await?
            .into_iter()
            .map(|book| {
                let BookStatus::Deleted { deleted_at } = book.status() else {
                    return Err(BookApplicationError::NotDeleted.into());
                };
                let file_type = self
                    .bookfile
                    .get_book_type(book.hash())
                    .ok_or(BookFileApplicationError::Unsupported)?;
                Ok(TrashedBookOutput {
                    book: (book, file_type).into(),
                    deleted_at,
                })
            })
            .collect()
    }

    pub async fn rename_book(
        &mut self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut book = tx.find_by_id(&id).await?;
        book.rename(new_title)?.exec(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_books(
        &mut self,
        query: GetBooksQuery,
    ) -> Result<Vec<GetBookOutput>, LibraryApplicationError> {
        let ids = query
            .book_ids
            .iter()
            .map(BookId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        self.repository
            .list_active(&ids)
            .await?
            .into_iter()
            .map(|book| {
                let file_type = self
                    .bookfile
                    .get_book_type(book.hash())
                    .ok_or(BookFileApplicationError::Unsupported)?;
                Ok((book, file_type).into())
            })
            .collect()
    }

    pub async fn get_cover(
        &mut self,
        command: GetBookCoverCommand,
    ) -> Result<(impl Stream<Item = std::io::Result<Bytes>> + 'static, String), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        self.bookfile.download_book_cover_file(*book.hash()).await
    }
}
