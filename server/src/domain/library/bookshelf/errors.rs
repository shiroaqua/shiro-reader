#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookshelfDomainError {
    #[error("invalid bookshelf id")]
    InvalidBookshelfId,

    #[error("bookshelf name is required")]
    BookshelfNameRequired,

    #[error("bookshelf Name is invalid format")]
    BookshelfNameInvalidFormat,
    
}
