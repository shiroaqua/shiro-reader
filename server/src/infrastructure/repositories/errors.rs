#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {

    #[error("bookshelf name conflict")]
    BookshelfNameConflict,
    
    #[error("bookshelf not found")]
    BookshelfNotFound,

    #[error("book file type is inviled")]
    InvalidBookFileType,

    #[error("book not found")]
    BookNotFound,

    #[error("book title conflict")]
    BookTitleConflict,
    
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
