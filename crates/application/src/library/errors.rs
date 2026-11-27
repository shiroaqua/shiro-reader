use shiro_domain::library::{
    book::errors::BookDomainError,
    tag::errors::TagDomainError,
};
use thiserror::Error;

use crate::library::{
    book::{
        errors::BookApplicationError,
        file::errors::BookFileApplicationError,
    },
    ports::{
        BookFileStorageError, BookRepositoryError, TagRepositoryError,
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
            BookDomainError::InvalidSubtitle => Self::InvalidSubtitle,
            BookDomainError::InvalidAuthor => Self::InvalidAuthor,
            BookDomainError::InvalidPublisher => Self::InvalidPublisher,
            BookDomainError::InvalidPublishedDate => Self::InvalidPublishedDate,
            BookDomainError::InvalidLanguage => Self::InvalidLanguage,
            BookDomainError::InvalidDescription => Self::InvalidDescription,
            BookDomainError::InvalidIdentifier => Self::InvalidIdentifier,
            BookDomainError::SubtitleUnchanged => Self::SubtitleUnchanged,
            BookDomainError::AuthorsUnchanged => Self::AuthorsUnchanged,
            BookDomainError::PublisherUnchanged => Self::PublisherUnchanged,
            BookDomainError::PublishedDateUnchanged => Self::PublishedDateUnchanged,
            BookDomainError::LanguageUnchanged => Self::LanguageUnchanged,
            BookDomainError::DescriptionUnchanged => Self::DescriptionUnchanged,
            BookDomainError::IdentifierUnchanged => Self::IdentifierUnchanged,
            BookDomainError::InvalidReadingUnitOffset
            | BookDomainError::InvalidBookProgression
            | BookDomainError::InvalidEpubResourcePath
            | BookDomainError::InvalidEpubFragment
            | BookDomainError::InvalidEpubCfi
            | BookDomainError::InvalidPdfDestination
            | BookDomainError::InvalidNormalizedRect
            | BookDomainError::InvalidPdfSelection
            | BookDomainError::ReadingUnitOutOfBounds { .. }
            | BookDomainError::TooManyReadingUnits
            | BookDomainError::TooManyTocEntries
            | BookDomainError::TocMaxDepthExceeded => Self::Storage(value.into()),
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
