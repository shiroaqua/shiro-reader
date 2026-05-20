#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookshelfDomainError {
    #[error("invalid bookshelf id")]
    InvalidId,

    #[error("missing bookshelf name")]
    MissingName,

    #[error("invalid bookshelf name format")]
    InvalidNameFormat,
}