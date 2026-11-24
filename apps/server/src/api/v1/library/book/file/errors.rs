#[derive(Debug, thiserror::Error)]
pub enum BookFileAPIError {
    #[error("duplicate hash field")]
    DuplicateHashField,

    #[error("missing hash field")]
    MissingHashField,

    #[error("duplicate size field")]
    DuplicateSizeField,

    #[error("missing size field")]
    MissingSizeField,

    #[error("invalid size field")]
    InvalidSizeField,

    #[error("missing file field")]
    MissingFileField,
}

/// Marker error emitted when the net uploaded book file stream exceeds `max_upload_size`.
#[derive(Debug, thiserror::Error)]
#[error("net book file payload exceeded max upload size")]
pub struct NetUploadSizeExceeded;
