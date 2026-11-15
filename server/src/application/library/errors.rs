use std::io::{self, ErrorKind};

use thiserror::Error;
use crate::application::errors::MapToastyError;
use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::book::file::errors::BookFileApplicationError;
use crate::application::library::tag::errors::TagApplicationError;
use crate::domain::library::book::entity::Book;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::tag::entity::{BookTag, TagDefinition};
use crate::domain::library::tag::errors::TagDomainError;
use crate::infrastructure::db::sqlite::sqlite_error;

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
            BookDomainError::InvalidTimestamp => Self::Storage(anyhow::Error::new(value)),
        }
    }
}

impl From<io::Error> for LibraryApplicationError {
    // todo: 取消使用io:Error，转而专门设一个error
    fn from(value: io::Error) -> Self {
        match value.kind() {
            ErrorKind::NotFound => BookFileApplicationError::NotFound,
            _ => BookFileApplicationError::Storage(value.into()),
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
        if let Some(rusqlite::Error::SqliteFailure(code, Some(message))) = sqlite_error(&error) {
            if code.extended_code == 2067 && message.contains("books.title") {
                return BookApplicationError::TitleConflict.into();
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
        TagApplicationError::Storage(error.into()).into()
    }
}

impl MapToastyError<LibraryApplicationError> for BookTag {
    fn map_db_err(error: toasty::Error) -> LibraryApplicationError {
        TagApplicationError::Storage(error.into()).into()
    }
}
