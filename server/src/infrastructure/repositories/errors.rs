#[derive(Debug, thiserror::Error)]
pub enum LibraryRepositoryError {
    #[error("book file type is inviled")]
    InvalidBookFileType,

    #[error("book not found")]
    BookNotFound,

    #[error("book title conflict")]
    BookTitleConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum UserRepositoryError {
    #[error("user not found")]
    UserNotFound,

    #[error("user name conflict")]
    UserNameConflict,

    #[error("user version conflict")]
    UserVersionConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
