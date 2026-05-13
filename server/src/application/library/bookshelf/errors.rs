#[derive(Debug, thiserror::Error)]
pub enum BookshelfApplicationError {
    #[error("invalid bookshelf id")]
    InvalidBookshelfId,

    #[error("missing bookshelf name")]
    MissingBookshelfName,

    #[error("invalid bookshelf name format")]
    InvalidBookshelfNameFormat,

    #[error("bookshelf not found")]
    BookshelfNotFound,

    #[error("bookshelf name conflict")]
    BookshelfNameConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}