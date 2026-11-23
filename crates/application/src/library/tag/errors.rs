use shiro_domain::library::tag::errors::TagDomainError;

#[derive(Debug, thiserror::Error)]
pub enum TagApplicationError {
    #[error(transparent)]
    Domain(#[from] TagDomainError),

    #[error("parent tag not found")]
    ParentNotFound,

    #[error("a sibling tag with this name already exists")]
    SiblingNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
