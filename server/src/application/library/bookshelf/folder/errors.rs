#[derive(Debug, thiserror::Error)]
pub enum FolderApplicationError {
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

    #[error("folder not found")]
    FolderNotFound,

    #[error("parent folder not found")]
    ParentFolderNotFound,

    #[error("folder name conflict")]
    FolderNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}