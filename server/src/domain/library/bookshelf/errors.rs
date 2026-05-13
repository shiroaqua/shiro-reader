#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookshelfDomainError {
    #[error("invalid bookshelf id")]
    InvalidBookshelfId,

    #[error("missing bookshelf name")]
    MissingBookshelfName,

    #[error("invalid bookshelf name format")]
    InvalidBookshelfNameFormat,
}