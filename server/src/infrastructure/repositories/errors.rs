#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("book file type is inviled")]
    InvalidBookFileType,

    #[error("book not found")]
    BookNotFound,

    #[error("book title conflict")]
    BookTitleConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
