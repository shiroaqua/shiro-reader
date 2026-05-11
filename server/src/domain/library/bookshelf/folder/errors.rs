#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FolderDomainError {
    #[error("invalid folder id")]
    InvalidFolderId,

    #[error("invalid parent id")]
    InvalidParentId,

    #[error("folder id is required")]
    FolderIdRequired,

    #[error("folder name is required")]
    FolderNameRequired,

    #[error("folder Name is invalid format")]
    FolderNameInvalidFormat,
    
}
