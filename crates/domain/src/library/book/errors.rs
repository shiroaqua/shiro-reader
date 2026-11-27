#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum BookDomainError {
    #[error("invalid book id")]
    InvalidId,

    #[error("missing book title")]
    MissingTitle,

    #[error("invalid book title format")]
    InvalidTitleFormat,

    #[error("unchanged book title")]
    TitleUnchanged,

    #[error("book is already deleted")]
    AlreadyDeleted,

    #[error("book is not deleted")]
    NotDeleted,

    #[error("reading unit offset must be finite and within [0.0, 1.0]")]
    InvalidReadingUnitOffset,

    #[error("book progression must be finite and within [0.0, 1.0]")]
    InvalidBookProgression,

    #[error("invalid epub resource path")]
    InvalidEpubResourcePath,

    #[error("invalid epub fragment")]
    InvalidEpubFragment,

    #[error("invalid epub cfi")]
    InvalidEpubCfi,

    #[error("invalid pdf destination coordinate")]
    InvalidPdfDestination,

    #[error("normalized rectangle coordinates must be finite within [0.0, 1.0] with positive dimensions")]
    InvalidNormalizedRect,

    #[error("invalid pdf selection target")]
    InvalidPdfSelection,

    #[error("reading unit index {index} out of bounds (unit count: {unit_count})")]
    ReadingUnitOutOfBounds { index: u32, unit_count: usize },

    #[error("too many reading units")]
    TooManyReadingUnits,

    #[error("too many toc entries")]
    TooManyTocEntries,

    #[error("maximum toc depth exceeded")]
    TocMaxDepthExceeded,

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
}
