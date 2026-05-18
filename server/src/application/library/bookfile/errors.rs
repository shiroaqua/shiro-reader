#[derive(Debug, thiserror::Error)]
pub enum BookFileError {
    #[error("duplicate hash field")]
    DuplicateHashField,
    
    #[error("missing hash field")]
    MissingHashField,
    
    #[error("missing file field")]
    MissingFileField,

    #[error("invalid hash format")]
    InvalidHashFormat,

    #[error("bookfile not found")]
    NotFound,

    #[error("bookfile already exists")]
    AlreadyExists,

    #[error("bookfile upload conflict")]
    UploadConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}