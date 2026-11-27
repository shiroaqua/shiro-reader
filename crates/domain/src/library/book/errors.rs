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

    #[error("reading unit index {index} out of bounds (unit count: {unit_count})")]
    ReadingUnitOutOfBounds { index: u32, unit_count: usize },

    #[error("too many reading units")]
    TooManyReadingUnits,

    #[error("too many toc entries")]
    TooManyTocEntries,

    #[error("maximum toc depth exceeded")]
    TocMaxDepthExceeded,
}
