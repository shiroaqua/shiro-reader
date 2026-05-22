#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {

    #[error("bookshelf name conflict")]
    BookshelfNameConflict,
    
    #[error("bookshelf not found")]
    BookshelfNotFound,

    #[error("folder not found")]
    FolderNotFound,

    #[error("parent folder not found")]
    ParentFolderNotFound,

    #[error("folder name conflict")]
    FolderNameConflict,

    #[error("book not found")]
    BookNotFound,

    #[error("book title conflict")]
    BookTitleConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
