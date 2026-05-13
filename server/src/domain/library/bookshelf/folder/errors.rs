#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderDomainError {
    #[error("invalid folder id")]
    InvalidFolderId,

    #[error("invalid parent folder id")]
    InvalidParentFolderId,

    #[error("missing folder id")]
    MissingFolderId,

    #[error("missing folder name")]
    MissingFolderName,

    #[error("invalid folder name format")]
    InvalidFolderNameFormat,
}