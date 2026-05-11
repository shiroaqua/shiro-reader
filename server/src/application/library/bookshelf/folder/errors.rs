#[derive(Debug, thiserror::Error)]
pub enum FolderApplicationError {
    #[error("invalid folder id")]
    InvalidFolderId,

    #[error("invalid parent folder id")]
    InvalidParentId,

    #[error("folder id is required")]
    FolderIdRequired,

    #[error("folder name is required")]
    FolderNameRequired,

    #[error("folder name format is invaild")]
    FolderNameInvalidFormat,

    #[error("folder not found")]
    FolderNotFound,

    #[error("parent folder not found")]
    ParentFolderNotFound,

    #[error("folder name conflict")]
    FolderNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
