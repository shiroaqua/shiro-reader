#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("directory not found")]
    DirectoryNotFound,

    #[error("parent directory not found")]
    ParentDirectoryNotFound,

    #[error("directory name conflict")]
    DirectoryNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
