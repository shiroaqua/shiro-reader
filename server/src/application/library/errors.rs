use std::io::ErrorKind;

use crate::application::errors::MapToastyError;
use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::book::file::errors::BookFileApplicationError;
use crate::application::library::tag::errors::TagApplicationError;
use crate::domain::library::book::entity::Book;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::tag::entity::{BookTag, TagDefinition};
use crate::domain::library::tag::errors::TagDomainError;
use crate::infrastructure::db::sqlite::sqlite_error;
use crate::infrastructure::storage::book_file_storage::BookFileStorageError;
use thiserror::Error;

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

impl From<TagDomainError> for LibraryApplicationError {
    fn from(error: TagDomainError) -> Self {
        Self::Tag(error.into())
    }
}

impl MapToastyError<LibraryApplicationError> for Book {
    fn map_db_err(error: toasty::Error) -> LibraryApplicationError {
        if error.is_record_not_found() {
            return BookApplicationError::NotFound.into();
        }
        if let Some(rusqlite::Error::SqliteFailure(code, message)) = sqlite_error(&error) {
            match (code.extended_code, message.as_deref()) {
                (rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE, Some(msg))
                    if msg.contains("books.title") =>
                {
                    return BookApplicationError::TitleConflict.into();
                }
                _ => {}
            }
        }
        BookApplicationError::Storage(error.into()).into()
    }
}

impl MapToastyError<LibraryApplicationError> for TagDefinition {
    fn map_db_err(error: toasty::Error) -> LibraryApplicationError {
        if error.is_record_not_found() {
            return TagDomainError::NotFound.into();
        }
        if let Some(rusqlite::Error::SqliteFailure(code, message)) = sqlite_error(&error) {
            match (code.extended_code, message.as_deref()) {
                (rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE, Some(msg))
                    if msg.contains("tag_definitions.name") =>
                {
                    return TagApplicationError::SiblingNameConflict.into();
                }
                (rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY, _) => {
                    return TagApplicationError::ParentNotFound.into();
                }
                _ => {}
            }
        }
        TagApplicationError::Storage(error.into()).into()
    }
}

impl MapToastyError<LibraryApplicationError> for BookTag {
    fn map_db_err(error: toasty::Error) -> LibraryApplicationError {
        if error.is_record_not_found() {
            return TagDomainError::NotFound.into();
        }
        TagApplicationError::Storage(error.into()).into()
    }
}
