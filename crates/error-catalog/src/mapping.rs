use shiro_application::library::{
    book::{errors::BookApplicationError, file::errors::BookFileApplicationError},
    errors::LibraryApplicationError,
    tag::errors::TagApplicationError,
};
use shiro_domain::library::tag::errors::TagDomainError;

use crate::ErrorCode;

impl From<&LibraryApplicationError> for ErrorCode {
    fn from(value: &LibraryApplicationError) -> Self {
        match value {
            LibraryApplicationError::Book(err) => Self::from(err),
            LibraryApplicationError::BookFile(err) => Self::from(err),
            LibraryApplicationError::Tag(err) => Self::from(err),
        }
    }
}

impl From<&BookApplicationError> for ErrorCode {
    fn from(value: &BookApplicationError) -> Self {
        match value {
            BookApplicationError::InvalidId => Self::BookInvalidId,
            BookApplicationError::MissingTitle => Self::BookMissingTitle,
            BookApplicationError::InvalidTitleFormat => Self::BookInvalidTitleFormat,
            BookApplicationError::NotFound => Self::BookNotFound,
            BookApplicationError::TitleConflict => Self::BookTitleConflict,
            BookApplicationError::TitleUnchanged => Self::BookTitleUnchanged,
            BookApplicationError::AlreadyDeleted => Self::BookAlreadyDeleted,
            BookApplicationError::NotDeleted => Self::BookNotDeleted,
            BookApplicationError::Storage(_) => Self::CommonInternalServerError,
        }
    }
}

impl From<&BookFileApplicationError> for ErrorCode {
    fn from(value: &BookFileApplicationError) -> Self {
        match value {
            BookFileApplicationError::InvalidHashFormat => Self::BookFileInvalidHashFormat,
            BookFileApplicationError::HashMismatch => Self::BookFileHashMismatch,
            BookFileApplicationError::NotFound => Self::BookFileNotFound,
            BookFileApplicationError::AlreadyExists => Self::BookFileAlreadyExists,
            BookFileApplicationError::UploadConflict => Self::BookFileUploadConflict,
            BookFileApplicationError::UploadTimeout => Self::BookFileUploadTimeout,
            BookFileApplicationError::SizeMismatch => Self::BookFileSizeMismatch,
            BookFileApplicationError::RangeNotSatisfiable { .. } => Self::BookFileRangeNotSatisfiable,
            BookFileApplicationError::Unsupported => Self::BookFileUnsupported,
            BookFileApplicationError::Storage(_) => Self::CommonInternalServerError,
        }
    }
}

impl From<&TagApplicationError> for ErrorCode {
    fn from(value: &TagApplicationError) -> Self {
        match value {
            TagApplicationError::Domain(err) => match err {
                TagDomainError::InvalidId => Self::TagInvalidId,
                TagDomainError::MissingName => Self::TagMissingName,
                TagDomainError::InvalidNameFormat => Self::TagInvalidNameFormat,
                TagDomainError::NameUnchanged => Self::TagNameUnchanged,
                TagDomainError::InvalidColor => Self::TagInvalidColor,
                TagDomainError::ColorUnchanged => Self::TagColorUnchanged,
                TagDomainError::InvalidValueRegex => Self::TagInvalidValueRegex,
                TagDomainError::ValueRegexNotAllowed => Self::TagValueRegexNotAllowed,
                TagDomainError::ValueRequired => Self::TagValueRequired,
                TagDomainError::ValueNotAllowed => Self::TagValueNotAllowed,
                TagDomainError::ValueRegexMismatch => Self::TagValueRegexMismatch,
                TagDomainError::AlreadyDeleted => Self::TagAlreadyDeleted,
                TagDomainError::NotDeleted => Self::TagNotDeleted,
                TagDomainError::NotFound => Self::TagNotFound,
                TagDomainError::ParentNotFound => Self::TagParentNotFound,
                TagDomainError::InvalidMove => Self::TagInvalidMove,
                TagDomainError::InvalidTree => Self::CommonInternalServerError,
            },
            TagApplicationError::ParentNotFound => Self::TagParentNotFound,
            TagApplicationError::SiblingNameConflict => Self::TagSiblingNameConflict,
            TagApplicationError::Storage(_) => Self::CommonInternalServerError,
        }
    }
}

#[cfg(test)]
mod tests {
    use shiro_application::library::{
        book::{errors::BookApplicationError, file::errors::BookFileApplicationError},
        errors::LibraryApplicationError,
        tag::errors::TagApplicationError,
    };
    use shiro_domain::library::tag::errors::TagDomainError;

    use crate::ErrorCode;

    #[test]
    fn maps_library_application_errors_to_expected_catalog_codes() {
        let storage_err = || std::io::Error::other("disk failure").into();

        let book_cases = [
            (BookApplicationError::InvalidId, ErrorCode::BookInvalidId),
            (BookApplicationError::MissingTitle, ErrorCode::BookMissingTitle),
            (
                BookApplicationError::InvalidTitleFormat,
                ErrorCode::BookInvalidTitleFormat,
            ),
            (BookApplicationError::NotFound, ErrorCode::BookNotFound),
            (
                BookApplicationError::TitleConflict,
                ErrorCode::BookTitleConflict,
            ),
            (
                BookApplicationError::TitleUnchanged,
                ErrorCode::BookTitleUnchanged,
            ),
            (
                BookApplicationError::AlreadyDeleted,
                ErrorCode::BookAlreadyDeleted,
            ),
            (BookApplicationError::NotDeleted, ErrorCode::BookNotDeleted),
            (
                BookApplicationError::Storage(storage_err()),
                ErrorCode::CommonInternalServerError,
            ),
        ];
        for (err, expected) in book_cases {
            assert_eq!(ErrorCode::from(&LibraryApplicationError::Book(err)), expected);
        }

        let book_file_cases = [
            (
                BookFileApplicationError::InvalidHashFormat,
                ErrorCode::BookFileInvalidHashFormat,
            ),
            (
                BookFileApplicationError::HashMismatch,
                ErrorCode::BookFileHashMismatch,
            ),
            (BookFileApplicationError::NotFound, ErrorCode::BookFileNotFound),
            (
                BookFileApplicationError::AlreadyExists,
                ErrorCode::BookFileAlreadyExists,
            ),
            (
                BookFileApplicationError::UploadConflict,
                ErrorCode::BookFileUploadConflict,
            ),
            (
                BookFileApplicationError::UploadTimeout,
                ErrorCode::BookFileUploadTimeout,
            ),
            (
                BookFileApplicationError::SizeMismatch,
                ErrorCode::BookFileSizeMismatch,
            ),
            (
                BookFileApplicationError::RangeNotSatisfiable { size: 1024 },
                ErrorCode::BookFileRangeNotSatisfiable,
            ),
            (
                BookFileApplicationError::Unsupported,
                ErrorCode::BookFileUnsupported,
            ),
            (
                BookFileApplicationError::Storage(storage_err()),
                ErrorCode::CommonInternalServerError,
            ),
        ];
        for (err, expected) in book_file_cases {
            assert_eq!(
                ErrorCode::from(&LibraryApplicationError::BookFile(err)),
                expected
            );
        }

        let tag_cases = [
            (
                TagApplicationError::Domain(TagDomainError::InvalidId),
                ErrorCode::TagInvalidId,
            ),
            (
                TagApplicationError::Domain(TagDomainError::MissingName),
                ErrorCode::TagMissingName,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidNameFormat),
                ErrorCode::TagInvalidNameFormat,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NameUnchanged),
                ErrorCode::TagNameUnchanged,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidColor),
                ErrorCode::TagInvalidColor,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ColorUnchanged),
                ErrorCode::TagColorUnchanged,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidValueRegex),
                ErrorCode::TagInvalidValueRegex,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRegexNotAllowed),
                ErrorCode::TagValueRegexNotAllowed,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRequired),
                ErrorCode::TagValueRequired,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueNotAllowed),
                ErrorCode::TagValueNotAllowed,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRegexMismatch),
                ErrorCode::TagValueRegexMismatch,
            ),
            (
                TagApplicationError::Domain(TagDomainError::AlreadyDeleted),
                ErrorCode::TagAlreadyDeleted,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NotDeleted),
                ErrorCode::TagNotDeleted,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NotFound),
                ErrorCode::TagNotFound,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ParentNotFound),
                ErrorCode::TagParentNotFound,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidMove),
                ErrorCode::TagInvalidMove,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidTree),
                ErrorCode::CommonInternalServerError,
            ),
            (
                TagApplicationError::ParentNotFound,
                ErrorCode::TagParentNotFound,
            ),
            (
                TagApplicationError::SiblingNameConflict,
                ErrorCode::TagSiblingNameConflict,
            ),
            (
                TagApplicationError::Storage(storage_err()),
                ErrorCode::CommonInternalServerError,
            ),
        ];
        for (err, expected) in tag_cases {
            assert_eq!(ErrorCode::from(&LibraryApplicationError::Tag(err)), expected);
        }

    }
}
