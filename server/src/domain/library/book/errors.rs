#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookDomainError {
    #[error("invalid book id")]
    InvalidBookId,

    #[error("missing book title")]
    MissingBookTitle,

    #[error("invalid book title format")]
    InvalidBookTitleFormat,

    #[error("invalid book hash")]
    InvalidBookHash,
}
