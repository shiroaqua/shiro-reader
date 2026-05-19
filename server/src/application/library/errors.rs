use thiserror::Error;

use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::bookshelf::folder::errors::FolderApplicationError;
use crate::application::library::bookshelf::errors::BookshelfApplicationError;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::bookshelf::errors::BookshelfDomainError;
use crate::domain::library::bookshelf::folder::errors::FolderDomainError;

#[derive(Debug, Error)]
pub enum LibraryApplicationError {
    #[error("Book error: {0}")]
    Book(#[from] BookApplicationError),

    #[error("Folder error: {0}")]
    Folder(#[from] FolderApplicationError),
    
    #[error("Bookshelf error: {0}")]
    Bookshelf(#[from] BookshelfApplicationError),
}

impl From<BookDomainError> for LibraryApplicationError {
    fn from(err: BookDomainError) -> Self {
        LibraryApplicationError::Book(err.into())
    }
}


impl From<BookshelfDomainError> for LibraryApplicationError {
    fn from(err: BookshelfDomainError) -> Self {
        LibraryApplicationError::Bookshelf(err.into())
    }
}

impl From<FolderDomainError> for LibraryApplicationError {
    fn from(err: FolderDomainError) -> Self {
        LibraryApplicationError::Folder(err.into())
    }
}