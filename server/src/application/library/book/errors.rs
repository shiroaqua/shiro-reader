#[derive(Debug, thiserror::Error)]
pub enum BookApplicationError {
    #[error("invalid book id")]
    InvalidId,

    #[error("missing book title")]
    MissingTitle,

    #[error("invalid book title format")]
    InvalidTitleFormat,

    #[error("invalid book hash format")]
    InvalidHashFormat,

    #[error("book not found")]
    NotFound,

    #[error("book title conflict")]
    TitleConflict,

    #[error("book location not found")]
    LocationNotFound,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
