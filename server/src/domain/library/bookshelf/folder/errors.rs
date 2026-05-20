#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderDomainError {
    #[error("invalid folder id")]
    InvalidId,

    #[error("invalid parent folder id")]
    InvalidParentId,

    #[error("missing folder id")]
    MissingId,

    #[error("missing folder name")]
    MissingName,

    #[error("invalid folder name format")]
    InvalidNameFormat,
}