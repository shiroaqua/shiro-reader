use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use derive_new::new;
use shiro_domain::library::{
    book::{
        entity::Book,
        errors::BookDomainError,
        value_objects::{BookId, BookStatus, BookTitle},
    },
    tag::value_objects::BookTagStatus,
};

use crate::library::{
    book::{
        commands::{
            CreateBookCommand, DeleteBookCommand, DeleteBooksCommand, GetBookCoverCommand,
            GetBookFileMetaCommand,
            OpenBookFileRangeCommand, PermanentlyDeleteBooksCommand, RenameBookCommand,
            RestoreBookIdCommand,
        },
        dto::{
            BookFileDescriptor, BookNavigationOutput, CreateBookOutput, GetBookOutput,
            TrashedBookOutput,
        },
        errors::BookApplicationError,
        file::{errors::BookFileApplicationError, service::BookFileService},
        queries::{GetBookNavigationQuery, GetBooksQuery},
    },
    errors::LibraryApplicationError,
    ports::{BookFileStore, BookRepository, BookTransaction, CoverByteStream, OpenedBookFile},
};

#[derive(new)]
pub struct BookService<R, F> {
    repository: R,
    bookfile: Arc<BookFileService<F>>,
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

impl<R, F> BookService<R, F>
where
    R: BookRepository,
    F: BookFileStore,
{
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
        let book = Book::new(BookId::new(), title, hash)?;
        let book = self.repository.create(book).await?;

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
            let change = books
                .get_mut(id)
                .expect("book existence checked")
                .mark_deleted()?;
            tx.apply_book_change(change).await?;
        }
        for mut binding in tx.list_tag_bindings(&ids).await? {
            let change = binding.mark_deleted()?;
            tx.apply_tag_binding_change(change).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn restore_book(
        &mut self,
        command: RestoreBookIdCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut book = tx.find_by_id(&id).await?;
        if book.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted.into());
        }
        let occupied = tx.list_active_titles().await?;
        let restored_title = available_restore_title(&occupied, book.title())?;
        let change = book.restore(restored_title)?;
        tx.apply_book_change(change).await?;
        for mut binding in tx.list_tag_bindings(std::slice::from_ref(&id)).await? {
            if binding.status() != BookTagStatus::Active {
                let change = binding.restore()?;
                tx.apply_tag_binding_change(change).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn permanently_delete_books(
        &mut self,
        command: PermanentlyDeleteBooksCommand,
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

    pub async fn list_trashed_book(&mut self) -> Result<Vec<TrashedBookOutput>, LibraryApplicationError> {
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
                    deleted_at: deleted_at.to_string(),
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
        let change = book.rename(new_title)?;
        tx.apply_book_change(change).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn list_books(
        &mut self,
        query: GetBooksQuery,
    ) -> Result<Vec<GetBookOutput>, LibraryApplicationError> {
        let ids = query
            .ids
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
    ) -> Result<(CoverByteStream, String), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        self.bookfile.download_book_cover_file(*book.hash()).await
    }

    pub async fn get_book_file_meta(
        &mut self,
        command: GetBookFileMetaCommand,
    ) -> Result<BookFileDescriptor, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        if book.status() != BookStatus::Active {
            return Err(BookApplicationError::AlreadyDeleted.into());
        }
        let hash = *book.hash();
        let meta = self
            .bookfile
            .get_book_meta(&hash)
            .ok_or(BookFileApplicationError::NotFound)?;
        Ok(BookFileDescriptor {
            hash,
            file_type: meta.file_type,
            size: meta.size,
        })
    }

    pub async fn get_book_navigation(
        &mut self,
        query: GetBookNavigationQuery,
    ) -> Result<BookNavigationOutput, LibraryApplicationError> {
        let id = BookId::parse(query.id)?;
        let book = self.repository.find_by_id(&id).await?;
        if book.status() != BookStatus::Active {
            return Err(BookApplicationError::AlreadyDeleted.into());
        }
        let hash = *book.hash();
        if self.bookfile.get_book_meta(&hash).is_none() {
            return Err(BookFileApplicationError::NotFound.into());
        }
        let navigation = self.bookfile.extract_book_navigation(&hash).await;
        Ok(navigation.into())
    }

    pub async fn open_book_file_range(
        &mut self,
        command: OpenBookFileRangeCommand,
    ) -> Result<(BookFileDescriptor, OpenedBookFile<F::Reader>), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self.repository.find_by_id(&id).await?;
        if book.status() != BookStatus::Active {
            return Err(BookApplicationError::AlreadyDeleted.into());
        }
        let hash = *book.hash();
        let meta = self
            .bookfile
            .get_book_meta(&hash)
            .ok_or(BookFileApplicationError::NotFound)?;
        let opened = self
            .bookfile
            .open_book_file_range(&hash, command.offset, command.length)
            .await?;
        Ok((
            BookFileDescriptor {
                hash,
                file_type: meta.file_type,
                size: meta.size,
            },
            opened,
        ))
    }
}
