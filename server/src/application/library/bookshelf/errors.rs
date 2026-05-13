#[derive(Debug, thiserror::Error)]
pub enum BookshelfApplicationError {
    #[error("invalid bookshelf id")]
    InvalidBookshelfId,

    #[error("missing bookshelf name")]
    MissingBookshelfName,

    #[error("invalid bookshelf name format")]
    InvalidBookshelfNameFormat,
}