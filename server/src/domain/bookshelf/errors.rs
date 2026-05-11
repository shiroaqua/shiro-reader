#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DirectoryDomainError {
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

    #[error("Directory Name is invalid format")]
    DirectoryNameInvalidFormat,
    
}
