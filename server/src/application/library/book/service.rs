use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        book::{
            commands::{
                CreateBookCommand, CreateBookOutput, DeleteBookCommand, GetBookCommand,
                GetBookOutput, RenameBookCommand,
            },
            errors::BookApplicationError,
            ports::BookRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::{
        book::{
            entity::Book,
            errors::BookDomainError,
            value_objects::{BookId, BookTitle},
        },
        bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId},
    },
    shared::time::now_ms,
};

#[derive(new)]
pub struct BookService {
    repository: Arc<dyn BookRepository>,
}

impl BookService {
    pub async fn create_book(
        &self,
        command: CreateBookCommand,
    ) -> Result<CreateBookOutput, LibraryApplicationError> {
        let title = BookTitle::parse(command.title)?;
        let hash = blake3::Hash::from_hex(command.hash)
            .map_err(|_| BookApplicationError::InvalidHashFormat)?;
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = command
            .folder_id
            .map(FolderId::parse_folder_id)
            .transpose()?;
        let now = now_ms();

        let book = Book::new(
            BookId::new(),
            title,
            hash,
            bookshelf_id,
            folder_id,
            now,
            now,
        );
        let created = self.repository.create(book).await?;

        Ok(CreateBookOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }

    pub async fn delete_book(
        &self,
        command: DeleteBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        self.repository.delete(&id).await?;
        Ok(())
    }

    pub async fn rename_book(
        &self,
        command: RenameBookCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let new_title = BookTitle::parse(command.title)?;
        self.repository.rename(&id, &new_title).await?;

        Ok(())
    }

    pub async fn get_book(
        &self,
        command: GetBookCommand,
    ) -> Result<GetBookOutput, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        Ok(self.repository.find_by_id(&id).await?.into())
    }
}

impl From<BookDomainError> for BookApplicationError {
    fn from(value: BookDomainError) -> Self {
        match value {
            BookDomainError::InvalidId => Self::InvalidId,
            BookDomainError::MissingTitle => Self::MissingTitle,
            BookDomainError::InvalidTitleFormat => Self::InvalidTitleFormat,
            BookDomainError::InvalidHash => Self::InvalidHashFormat,
        }
    }
}
