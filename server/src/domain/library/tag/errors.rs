#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TagDomainError {
    #[error("invalid tag id")]
    InvalidId,

    #[error("missing tag name")]
    MissingName,

    #[error("tag name must contain only Unicode letters and numbers")]
    InvalidNameFormat,

    #[error("unchanged tag name")]
    NameUnchanged,

    #[error("invalid tag value regex")]
    InvalidValueRegex,

    #[error("a tag without values cannot have a value regex")]
    ValueRegexNotAllowed,

    #[error("a nonempty tag value is required")]
    ValueRequired,

    #[error("this tag does not allow a value")]
    ValueNotAllowed,

    #[error("tag value does not match the regex")]
    ValueRegexMismatch,

    #[error("tag not found")]
    NotFound,

    #[error("parent tag not found")]
    ParentNotFound,

    #[error("tag cannot be moved under itself or one of its descendants")]
    InvalidMove,

    #[error("tag tree contains duplicate ids, missing parents or a cycle")]
    InvalidTree,

    #[error("invalid tag timestamp")]
    InvalidTimestamp,
}
