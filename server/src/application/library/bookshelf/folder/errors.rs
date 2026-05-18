#[derive(Debug, thiserror::Error)]
pub enum FolderApplicationError {
    #[error("invalid folder id")]
    InvalidId,

    #[error("invalid parent folder id")]
    InvalidParentId,

    #[error("missing folder id")]
    MissingFolderId,

    #[error("missing folder name")]
    MissingName,

    #[error("invalid folder name format")]
    InvalidNameFormat,

    #[error("folder not found")]
    NotFound,

    #[error("parent folder not found")]
    ParentNotFound,

    #[error("folder name conflict")]
    NameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}