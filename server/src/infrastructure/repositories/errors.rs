#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("folder not found")]
    FolderNotFound,

    #[error("parent folder not found")]
    ParentFolderNotFound,

    #[error("folder name conflict")]
    FolderNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
