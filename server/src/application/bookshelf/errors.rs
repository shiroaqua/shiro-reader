#[derive(Debug, thiserror::Error)]
pub enum DirectoryApplicationError {
    #[error("invalid directory id")]
    InvalidDirectoryId,

    #[error("invalid parent id")]
    InvalidParentId,

    #[error("directory id is required")]
    DirectoryIdRequired,

    #[error("directory name is required")]
    DirectoryNameRequired,

    #[error("directory name is reserved")]
    DirectoryNameReserved,

    #[error("directory name format is invaild")]
    DirectoryNameInvalidFormat,

    #[error("directory not found")]
    DirectoryNotFound,

    #[error("parent directory not found")]
    ParentDirectoryNotFound,

    #[error("directory name conflict")]
    DirectoryNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
