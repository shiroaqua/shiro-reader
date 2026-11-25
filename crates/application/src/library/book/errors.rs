#[derive(Debug, thiserror::Error)]
pub enum BookApplicationError {
    #[error("invalid book id")]
    InvalidId,

    #[error("missing book title")]
    MissingTitle,

    #[error("invalid book title format")]
    InvalidTitleFormat,

    #[error("book not found")]
    NotFound,

    #[error("book title conflict")]
    TitleConflict,

    #[error("unchanged book title")]
    TitleUnchanged,

    #[error("book is already deleted")]
    AlreadyDeleted,

    #[error("book is not deleted")]
    NotDeleted,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
