use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use shiro_domain::library::{
    book::{
        entity::{Book, BookMetadata, BookMetadataChange},
        errors::BookDomainError,
        value_objects::{
            BookAuthor, BookDescription, BookHash, BookId, BookIdentifier, BookLanguage,
            BookPublishedDate, BookPublisher, BookStatus, BookSubtitle, BookTitle,
        },
    },
    tag::value_objects::BookTagStatus,
};

use crate::library::{
    book::{
        commands::{
            ChangeBookAuthorsCommand, ChangeBookDescriptionCommand, ChangeBookIdentifierCommand,
            ChangeBookLanguageCommand, ChangeBookPublishedDateCommand, ChangeBookPublisherCommand,
            ChangeBookSubtitleCommand, ClearBookAuthorsCommand, ClearBookDescriptionCommand,
            ClearBookIdentifierCommand, ClearBookLanguageCommand, ClearBookPublishedDateCommand,
            ClearBookPublisherCommand, ClearBookSubtitleCommand, CreateBookCommand,
            DeleteBookCommand, DeleteBooksCommand, GetBookCoverCommand, GetBookFileMetaCommand,
            OpenBookFileRangeCommand, PermanentlyDeleteBooksCommand, RenameBookCommand,
            RestoreBookIdCommand,
        },
        dto::{
            BookFileDescriptor, BookMetadataOutput, BookNavigationOutput, CreateBookOutput,
            GetBookOutput, TrashedBookOutput,
        },
        errors::BookApplicationError,
        file::{errors::BookFileApplicationError, service::BookFileService},
        queries::{GetBookMetadataQuery, GetBookNavigationQuery, GetBooksQuery},
    },
    errors::LibraryApplicationError,
    ports::{BookFileStore, BookRepository, BookTransaction, CoverByteStream, OpenedBookFile},
};

pub struct BookService<R, F> {
    repository: R,
    bookfile: Arc<BookFileService<F>>,
}

impl<R, F> BookService<R, F> {
    pub fn new(repository: R, bookfile: Arc<BookFileService<F>>) -> Self {
        Self {
            repository,
            bookfile,
        }
    }
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
        let extracted = self.bookfile.extract_book_metadata(&hash).await;
        let book = Book::new(BookId::new(), title, BookHash::new(hash));
        let metadata = BookMetadata::new(
            book.id().clone(),
            extracted.subtitle,
            extracted.authors,
            extracted.publisher,
            extracted.published_date,
            extracted.language,
            extracted.description,
            extracted.identifier,
        );
        let mut tx = self.repository.begin_transaction().await?;
        let book = tx.create_book(book).await?;
        let _ = tx.create_metadata(metadata).await?;
        tx.commit().await?;

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
        for mut metadata in tx.find_metadata_by_book_ids(&ids).await? {
            let change = metadata.mark_deleted()?;
            tx.apply_metadata_change(change).await?;
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
        let mut metadata = tx.find_metadata_by_book_id(&id).await?;
        let change = metadata.restore()?;
        tx.apply_metadata_change(change).await?;
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

    pub async fn get_book_metadata(
        &mut self,
        query: GetBookMetadataQuery,
    ) -> Result<BookMetadataOutput, LibraryApplicationError> {
        let id = BookId::parse(query.id)?;
        let book = self.repository.find_by_id(&id).await?;
        if book.status() != BookStatus::Active {
            return Err(BookApplicationError::AlreadyDeleted.into());
        }
        let file_meta = self
            .bookfile
            .get_book_meta(book.hash())
            .ok_or(BookFileApplicationError::NotFound)?;
        let metadata = self.repository.find_metadata_by_book_id(&id).await?;
        Ok((book, metadata, file_meta).into())
    }

    async fn mutate_book_metadata(
        &mut self,
        id: BookId,
        mutate: impl for<'a> FnOnce(
            &'a mut BookMetadata,
        ) -> Result<BookMetadataChange<'a>, BookDomainError>,
    ) -> Result<(), LibraryApplicationError> {
        let mut tx = self.repository.begin_transaction().await?;
        let mut metadata = tx.find_metadata_by_book_id(&id).await?;
        let change = mutate(&mut metadata)?;
        tx.apply_metadata_change(change).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn change_book_subtitle(
        &mut self,
        command: ChangeBookSubtitleCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let subtitle = BookSubtitle::parse(command.subtitle)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_subtitle(subtitle))
            .await
    }

    pub async fn clear_book_subtitle(
        &mut self,
        command: ClearBookSubtitleCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_subtitle)
            .await
    }

    pub async fn change_book_authors(
        &mut self,
        command: ChangeBookAuthorsCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let authors = BookAuthor::parse_list(command.authors)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_authors(authors))
            .await
    }

    pub async fn clear_book_authors(
        &mut self,
        command: ClearBookAuthorsCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_authors)
            .await
    }

    pub async fn change_book_publisher(
        &mut self,
        command: ChangeBookPublisherCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let publisher = BookPublisher::parse(command.publisher)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_publisher(publisher))
            .await
    }

    pub async fn clear_book_publisher(
        &mut self,
        command: ClearBookPublisherCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_publisher)
            .await
    }

    pub async fn change_book_published_date(
        &mut self,
        command: ChangeBookPublishedDateCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let published_date = BookPublishedDate::parse(command.published_date)?;
        self.mutate_book_metadata(id, |metadata| {
            metadata.change_published_date(published_date)
        })
        .await
    }

    pub async fn clear_book_published_date(
        &mut self,
        command: ClearBookPublishedDateCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_published_date)
            .await
    }

    pub async fn change_book_language(
        &mut self,
        command: ChangeBookLanguageCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let language = BookLanguage::parse(command.language)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_language(language))
            .await
    }

    pub async fn clear_book_language(
        &mut self,
        command: ClearBookLanguageCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_language)
            .await
    }

    pub async fn change_book_description(
        &mut self,
        command: ChangeBookDescriptionCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let description = BookDescription::parse(command.description)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_description(description))
            .await
    }

    pub async fn clear_book_description(
        &mut self,
        command: ClearBookDescriptionCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_description)
            .await
    }

    pub async fn change_book_identifier(
        &mut self,
        command: ChangeBookIdentifierCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let identifier = BookIdentifier::parse(command.identifier)?;
        self.mutate_book_metadata(id, |metadata| metadata.change_identifier(identifier))
            .await
    }

    pub async fn clear_book_identifier(
        &mut self,
        command: ClearBookIdentifierCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.mutate_book_metadata(id, BookMetadata::clear_identifier)
            .await
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
