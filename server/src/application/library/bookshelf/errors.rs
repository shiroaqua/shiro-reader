#[derive(Debug, thiserror::Error)]
pub enum BookshelfApplicationError {
    #[error("invalid bookshelf id")]
    InvalidId,

    #[error("missing bookshelf name")]
    MissingName,

    #[error("invalid bookshelf name format")]
    InvalidNameFormat,

    #[error("bookshelf not found")]
    NotFound,

    #[error("bookshelf name conflict")]
    NameConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}