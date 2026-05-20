#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookDomainError {
    #[error("invalid book id")]
    InvalidId,

    #[error("missing book title")]
    MissingTitle,

    #[error("invalid book title format")]
    InvalidTitleFormat,

    #[error("invalid book hash")]
    InvalidHash,
}
