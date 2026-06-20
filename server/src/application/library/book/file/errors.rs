#[derive(Debug, thiserror::Error)]
pub enum BookFileApplicationError {
    #[error("invalid hash format")]
    InvalidHashFormat,

    #[error("bookfile hash mismatch")]
    HashMismatch,

    #[error("bookfile not found")]
    NotFound,

    #[error("bookfile already exists")]
    AlreadyExists,

    #[error("bookfile upload conflict")]
    UploadConflict,

    #[error("bookfile type unsupported")]
    Unsupported,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
