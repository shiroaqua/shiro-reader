#[derive(Debug, thiserror::Error)]
pub enum BookFileAPIError {
    #[error("duplicate hash field")]
    DuplicateHashField,
    
    #[error("missing hash field")]
    MissingHashField,
    
    #[error("missing file field")]
    MissingFileField,
}
