use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        book::{
            commands::{
                CreateBookCommand, CreateBookOutput, DeleteBookCommand, GetBookCommand,
                GetBookOutput,
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
    infrastructure::repositories::errors::RepositoryError,
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
        let hash = blake3::Hash::from_hex(command.hash).map_err(|_| BookApplicationError::InvalidHashFormat)?;
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = command.folder_id.map(FolderId::parse_folder_id).transpose()?;
        let now = now_ms();

        let book = Book::new(BookId::new(), title, hash, bookshelf_id, folder_id, now, now);
        let created = self
            .repository
            .create(book)
            .await
            .map_err(BookApplicationError::from)?;

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
        self.repository
            .delete(&id)
            .await
            .map_err(BookApplicationError::from)?;
        Ok(())
    }

    pub async fn get_book(
        &self,
        command: GetBookCommand,
    ) -> Result<GetBookOutput, LibraryApplicationError> {
        let id = BookId::parse(command.id)?;
        let book = self
            .repository
            .find_by_id(&id)
            .await
            .map_err(BookApplicationError::from)?;

        Ok(GetBookOutput {
            id: book.id,
            title: book.title,
            hash: book.hash,
            bookshelf_id: book.bookshelf_id,
            folder_id: book.folder_id,
            created_at: book.created_at,
            updated_at: book.updated_at,
        })
    }
}

impl From<BookDomainError> for BookApplicationError {
    fn from(value: BookDomainError) -> Self {
        match value {
            BookDomainError::InvalidBookId => Self::InvalidId,
            BookDomainError::MissingBookTitle => Self::MissingTitle,
            BookDomainError::InvalidBookTitleFormat => Self::InvalidTitleFormat,
            BookDomainError::InvalidBookHash => Self::InvalidHashFormat,
        }
    }
}

impl From<RepositoryError> for BookApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::BookNotFound => Self::NotFound,
            RepositoryError::BookTitleConflict => Self::TitleConflict,
            RepositoryError::BookLocationNotFound => Self::LocationNotFound,
            RepositoryError::Storage(error) => Self::Storage(error),
            _ => Self::Storage(anyhow::anyhow!("unrelated error")),
        }
    }
}
