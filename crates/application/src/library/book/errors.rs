#[derive(Debug, thiserror::Error)]
pub enum BookApplicationError {
    #[error("invalid book id")]
    InvalidId,

    #[error("missing book title")]
    MissingTitle,

    #[error("invalid book title format")]
    InvalidTitleFormat,

    #[error("book not found")]
    NotFound,

    #[error("book title conflict")]
    TitleConflict,

    #[error("unchanged book title")]
    TitleUnchanged,

    #[error("book is already deleted")]
    AlreadyDeleted,

    #[error("book is not deleted")]
    NotDeleted,

    #[error("invalid book subtitle")]
    InvalidSubtitle,

    #[error("invalid book author")]
    InvalidAuthor,

    #[error("invalid book publisher")]
    InvalidPublisher,

    #[error("invalid book published date")]
    InvalidPublishedDate,

    #[error("invalid book language")]
    InvalidLanguage,

    #[error("invalid book description")]
    InvalidDescription,

    #[error("invalid book identifier")]
    InvalidIdentifier,

    #[error("unchanged book subtitle")]
    SubtitleUnchanged,

    #[error("unchanged book authors")]
    AuthorsUnchanged,

    #[error("unchanged book publisher")]
    PublisherUnchanged,

    #[error("unchanged book published date")]
    PublishedDateUnchanged,

    #[error("unchanged book language")]
    LanguageUnchanged,

    #[error("unchanged book description")]
    DescriptionUnchanged,

    #[error("unchanged book identifier")]
    IdentifierUnchanged,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}
