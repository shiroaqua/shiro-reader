use std::io::{self, ErrorKind};

use thiserror::Error;

use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::book::file::errors::BookFileApplicationError;
use crate::application::library::tag::errors::TagApplicationError;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::tag::errors::TagDomainError;
use crate::infrastructure::repositories::errors::{LibraryRepositoryError, TagRepositoryError};

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

impl From<LibraryRepositoryError> for LibraryApplicationError {
    fn from(value: LibraryRepositoryError) -> Self {
        match value { // InvalidBookFileType好像是泛型转换出了问题，因此不该报告给用户具体错误
            LibraryRepositoryError::BookNotFound => Self::Book(BookApplicationError::NotFound),
            LibraryRepositoryError::BookVersionConflict => Self::Book(BookApplicationError::VersionConflict),
            LibraryRepositoryError::BookTitleConflict => Self::Book(BookApplicationError::TitleConflict),
            LibraryRepositoryError::InvalidBookFileType => Self::Book(BookApplicationError::Storage(anyhow::Error::new(value))), 
            LibraryRepositoryError::Storage(error) => Self::Book(BookApplicationError::Storage(error)),
        }
    }
}

impl From<TagDomainError> for LibraryApplicationError {
    fn from(error: TagDomainError) -> Self {
        Self::Tag(error.into())
    }
}

impl From<TagRepositoryError> for LibraryApplicationError {
    fn from(error: TagRepositoryError) -> Self {
        match error {
            TagRepositoryError::NotFound => TagDomainError::NotFound.into(),
            TagRepositoryError::ParentNotFound => TagApplicationError::ParentNotFound.into(),
            TagRepositoryError::BookNotFound => BookApplicationError::NotFound.into(),
            TagRepositoryError::VersionConflict => TagApplicationError::VersionConflict.into(),
            TagRepositoryError::Storage(error) => TagApplicationError::Storage(error).into(),
        }
    }
}
