#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChapterDomainError {
    #[error("chapter ID is invalid")]
    InvalidId,
    #[error("chapter title cannot be empty or padded with whitespace")]
    InvalidTitle,
    #[error("chapter location is invalid")]
    InvalidLocation,
}
