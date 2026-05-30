use thiserror::Error;

use crate::application::library::book::errors::BookApplicationError;
use crate::application::library::book::file::errors::BookFileApplicationError;
use crate::application::library::bookshelf::errors::BookshelfApplicationError;
use crate::application::library::bookshelf::folder::errors::FolderApplicationError;
use crate::domain::library::book::errors::BookDomainError;
use crate::domain::library::bookshelf::errors::BookshelfDomainError;
use crate::domain::library::bookshelf::folder::errors::FolderDomainError;
use crate::infrastructure::repositories::errors::RepositoryError;

#[derive(Debug, Error)]
pub enum LibraryApplicationError {
    #[error("Book error: {0}")]
    Book(#[from] BookApplicationError),

    #[error("Book file error: {0}")]
    BookFile(#[from] BookFileApplicationError),

    #[error("Bookshelf error: {0}")]
    Bookshelf(#[from] BookshelfApplicationError),

    #[error("Folder error: {0}")]
    Folder(#[from] FolderApplicationError),

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

impl From<RepositoryError> for LibraryApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::InvalidBookFileType => { // 该错误是泛型转换出了问题，因此不该报告给用户具体错误
                LibraryApplicationError::Book(BookApplicationError::Storage(anyhow::Error::new(value)))
            },
            RepositoryError::BookNotFound => {
                LibraryApplicationError::Book(BookApplicationError::NotFound)
            }
            RepositoryError::BookTitleConflict => {
                LibraryApplicationError::Book(BookApplicationError::TitleConflict)
            }
            RepositoryError::BookshelfNotFound => {
                LibraryApplicationError::Bookshelf(BookshelfApplicationError::NotFound)
            }
            RepositoryError::BookshelfNameConflict => {
                LibraryApplicationError::Bookshelf(BookshelfApplicationError::NameConflict)
            }
            RepositoryError::FolderNotFound => {
                LibraryApplicationError::Folder(FolderApplicationError::NotFound)
            }
            RepositoryError::ParentFolderNotFound => {
                LibraryApplicationError::Folder(FolderApplicationError::ParentNotFound)
            }
            RepositoryError::FolderNameConflict => {
                LibraryApplicationError::Folder(FolderApplicationError::NameConflict)
            }
            RepositoryError::FolderCycled => {
                LibraryApplicationError::Folder(FolderApplicationError::Cycled)
            }
            RepositoryError::Storage(error) => {
                LibraryApplicationError::Book(BookApplicationError::Storage(error))
            }
        }
    }
}
