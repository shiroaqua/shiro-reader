use thiserror::Error;

use crate::application::library::bookshelf::folder::errors::FolderApplicationError;
use crate::application::library::bookshelf::errors::BookshelfApplicationError;
use crate::domain::library::bookshelf::errors::BookshelfDomainError;
use crate::domain::library::bookshelf::folder::errors::FolderDomainError;

#[derive(Debug, Error)]
pub enum LibraryApplicationError {
    #[error("Folder error: {0}")]
    Folder(#[from] FolderApplicationError),
    
    #[error("Bookshelf error: {0}")]
    Bookshelf(#[from] BookshelfApplicationError),
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