use crate::{
    application::library::bookshelf::errors::BookshelfApplicationError,
    domain::library::bookshelf::errors::BookshelfDomainError,
};

impl From<BookshelfDomainError> for BookshelfApplicationError {
    fn from(value: BookshelfDomainError) -> Self {
        match value {
            BookshelfDomainError::InvalidBookshelfId => Self::InvalidBookshelfId,
            BookshelfDomainError::BookshelfNameRequired => Self::BookshelfNameInvalidFormat,
            BookshelfDomainError::BookshelfNameInvalidFormat => Self::BookshelfNameInvalidFormat,
        }
    }
}