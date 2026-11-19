use std::io::ErrorKind;

use thiserror::Error;

use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::book::file::errors::BookFileApplicationError;
use crate::application::library::tag::errors::TagApplicationError;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::tag::errors::TagDomainError;
use crate::infrastructure::repositories::book_repository::BookRepositoryError;
use crate::infrastructure::repositories::tag_repository::TagRepositoryError;
use crate::infrastructure::storage::book_file_storage::BookFileStorageError;

#[derive(Debug, Error)]
pub enum LibraryApplicationError {
    #[error("Book error: {0}")]
    Book(#[from] BookApplicationError),

    #[error("Book file error: {0}")]
    BookFile(#[from] BookFileApplicationError),

    #[error("Tag error: {0}")]
    Tag(#[from] TagApplicationError),
}

impl From<BookDomainError> for BookApplicationError {
    fn from(value: BookDomainError) -> Self {
        match value {
            BookDomainError::InvalidId => Self::InvalidId,
            BookDomainError::MissingTitle => Self::MissingTitle,
            BookDomainError::InvalidTitleFormat => Self::InvalidTitleFormat,
            BookDomainError::InvalidHash => Self::InvalidHashFormat,
            BookDomainError::TitleUnchanged => Self::TitleUnchanged,
            BookDomainError::AlreadyDeleted => Self::AlreadyDeleted,
            BookDomainError::NotDeleted => Self::NotDeleted,
            BookDomainError::InvalidTimestamp => Self::Storage(anyhow::Error::new(value)),
        }
    }
}

impl From<BookFileStorageError> for LibraryApplicationError {
    fn from(value: BookFileStorageError) -> Self {
        match value.0.kind() {
            ErrorKind::NotFound => BookFileApplicationError::NotFound,
            _ => BookFileApplicationError::Storage(value.0.into()),
        }
        .into()
    }
}

impl From<BookDomainError> for LibraryApplicationError {
    fn from(err: BookDomainError) -> Self {
        LibraryApplicationError::Book(err.into())
    }
}

impl From<BookRepositoryError> for BookApplicationError {
    fn from(value: BookRepositoryError) -> Self {
        match value {
            BookRepositoryError::NotFound => Self::NotFound,
            BookRepositoryError::TitleConflict => Self::TitleConflict,
            BookRepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}

impl From<BookRepositoryError> for LibraryApplicationError {
    fn from(error: BookRepositoryError) -> Self {
        Self::Book(error.into())
    }
}

impl From<TagDomainError> for LibraryApplicationError {
    fn from(error: TagDomainError) -> Self {
        Self::Tag(error.into())
    }
}

impl From<TagRepositoryError> for TagApplicationError {
    fn from(value: TagRepositoryError) -> Self {
        match value {
            TagRepositoryError::NotFound => Self::Domain(TagDomainError::NotFound),
            TagRepositoryError::ParentNotFound => Self::ParentNotFound,
            TagRepositoryError::SiblingNameConflict => Self::SiblingNameConflict,
            TagRepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}

impl From<TagRepositoryError> for LibraryApplicationError {
    fn from(error: TagRepositoryError) -> Self {
        Self::Tag(error.into())
    }
}
