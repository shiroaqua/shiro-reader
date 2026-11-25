use shiro_domain::library::{
    book::{chapter::ChapterDomainError, errors::BookDomainError},
    tag::errors::TagDomainError,
};
use thiserror::Error;

use crate::library::{
    book::{
        errors::BookApplicationError,
        file::errors::BookFileApplicationError,
    },
    ports::{
        BookChapterError, BookFileStorageError, BookRepositoryError, TagRepositoryError,
    },
    tag::errors::TagApplicationError,
};

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
            BookDomainError::TitleUnchanged => Self::TitleUnchanged,
            BookDomainError::AlreadyDeleted => Self::AlreadyDeleted,
            BookDomainError::NotDeleted => Self::NotDeleted,
        }
    }
}

impl From<BookFileStorageError> for LibraryApplicationError {
    fn from(value: BookFileStorageError) -> Self {
        match value {
            BookFileStorageError::NotFound => BookFileApplicationError::NotFound,
            BookFileStorageError::AlreadyExists => BookFileApplicationError::AlreadyExists,
            BookFileStorageError::RangeOutOfBounds { size, .. } => {
                BookFileApplicationError::RangeNotSatisfiable { size }
            }
            BookFileStorageError::HashMismatch => BookFileApplicationError::HashMismatch,
            BookFileStorageError::Unsupported => BookFileApplicationError::Unsupported,
            BookFileStorageError::Io(err) => BookFileApplicationError::Storage(err.into()),
        }
        .into()
    }
}

impl From<BookDomainError> for LibraryApplicationError {
    fn from(err: BookDomainError) -> Self {
        Self::Book(err.into())
    }
}

impl From<ChapterDomainError> for LibraryApplicationError {
    fn from(err: ChapterDomainError) -> Self {
        Self::Book(err.into())
    }
}

impl From<ChapterDomainError> for BookApplicationError {
    fn from(value: ChapterDomainError) -> Self {
        match value {
            ChapterDomainError::InvalidId => Self::InvalidId,
            ChapterDomainError::InvalidTitle => Self::InvalidChapterTitle,
            ChapterDomainError::InvalidLocation => Self::InvalidChapterLocation,
        }
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

impl From<BookChapterError> for BookApplicationError {
    fn from(value: BookChapterError) -> Self {
        match value {
            BookChapterError::BookNotFound | BookChapterError::ChapterNotFound => Self::NotFound,
            BookChapterError::PositionConflict => Self::PositionConflict,
            BookChapterError::Storage(error) => Self::Storage(error),
        }
    }
}

impl From<BookChapterError> for LibraryApplicationError {
    fn from(error: BookChapterError) -> Self {
        Self::Book(error.into())
    }
}
