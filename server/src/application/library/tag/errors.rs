use crate::domain::library::tag::errors::TagDomainError;

#[derive(Debug, thiserror::Error)]
pub enum TagApplicationError {
    #[error(transparent)]
    Domain(#[from] TagDomainError),

    #[error("parent tag not found")]
    ParentNotFound,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
