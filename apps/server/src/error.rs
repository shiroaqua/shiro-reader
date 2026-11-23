use axum::{
    Json,
    extract::multipart::MultipartError,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use shiro_error_catalog::ErrorCode;
use shiro_http_contract::{ErrorDetail, ErrorResponse};
use tracing::error;

use crate::{
    api::v1::library::book::file::errors::{BookFileAPIError, NetUploadSizeExceeded},
    application::{
        identity::{
            authentication::errors::AuthenticationApplicationError,
            crypto::errors::CryptoApplicationError, errors::IdentityApplicationError,
            user::errors::UserApplicationError,
        },
        library::{
            book::{errors::BookApplicationError, file::errors::BookFileApplicationError},
            errors::LibraryApplicationError,
            tag::errors::TagApplicationError,
        },
    },
    domain::library::tag::errors::TagDomainError,
};

#[derive(Debug)]
pub enum AppError {
    BadRequest { message: &'static str },
    Unauthorized { message: &'static str },
    NotFound { message: &'static str },
    Conflict { message: &'static str },
    PayloadTooLarge,
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest { message } => (StatusCode::BAD_REQUEST, message),
            AppError::Unauthorized { message } => (StatusCode::UNAUTHORIZED, message),
            AppError::NotFound { message } => (StatusCode::NOT_FOUND, message),
            AppError::Conflict { message } => (StatusCode::CONFLICT, message),
            AppError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                ErrorCode::CommonPayloadTooLarge.key(),
            ),
            AppError::Internal(error) => {
                let is_net_size_exceeded = error.chain().any(|e| e.is::<NetUploadSizeExceeded>());
                let is_stream_aborted = error.chain().any(|e| e.is::<axum::Error>());

                let is_multipart_limit = !is_stream_aborted
                    && error
                        .chain()
                        .find_map(|e| e.downcast_ref::<MultipartError>())
                        .is_some_and(|e| e.status() == StatusCode::PAYLOAD_TOO_LARGE);

                if is_net_size_exceeded || is_multipart_limit || is_stream_aborted {
                    (
                        StatusCode::PAYLOAD_TOO_LARGE,
                        ErrorCode::CommonPayloadTooLarge.key(),
                    )
                } else {
                    error!(error = ?error, "internal server error");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        ErrorCode::CommonInternalServerError.key(),
                    )
                }
            }
        };

        let code = ErrorCode::from_key(message)
            .unwrap_or(ErrorCode::Unknown)
            .code();
        let body = ErrorResponse {
            error: ErrorDetail {
                code,
                message: message.to_owned(),
            },
        };

        (status, Json(body)).into_response()
    }
}

impl From<AuthenticationApplicationError> for AppError {
    fn from(value: AuthenticationApplicationError) -> Self {
        match value {
            AuthenticationApplicationError::InvalidCredentials => AppError::Unauthorized {
                message: "identity.authentication.invalid_credentials",
            },
            AuthenticationApplicationError::MissingBearerToken => AppError::Unauthorized {
                message: "identity.authentication.missing_bearer_token",
            },
            AuthenticationApplicationError::InvalidToken => AppError::Unauthorized {
                message: "identity.authentication.invalid_token",
            },
            AuthenticationApplicationError::Storage(error) => AppError::Internal(error),
            AuthenticationApplicationError::NotFound => AppError::NotFound {
                message: "identity.authentication.user_not_found",
            },
        }
    }
}

impl From<IdentityApplicationError> for AppError {
    fn from(value: IdentityApplicationError) -> Self {
        match value {
            IdentityApplicationError::Authentication(error) => error.into(),
            IdentityApplicationError::User(error) => match error {
                UserApplicationError::InvalidId => AppError::BadRequest {
                    message: "identity.user.invalid_id",
                },
                UserApplicationError::MissingUserName => AppError::BadRequest {
                    message: "identity.user.missing_user_name",
                },
                UserApplicationError::InvalidUserNameFormat => AppError::BadRequest {
                    message: "identity.user.invalid_user_name_format",
                },
                UserApplicationError::InvalidPasswordHash => AppError::BadRequest {
                    message: "identity.user.invalid_password_hash",
                },
                UserApplicationError::NotFound => AppError::NotFound {
                    message: "identity.user.not_found",
                },
                UserApplicationError::NameConflict => AppError::Conflict {
                    message: "identity.user.name_conflict",
                },
                UserApplicationError::NameUnchanged => AppError::BadRequest {
                    message: "identity.user.name_unchanged",
                },
                UserApplicationError::PasswordHashUnchanged => AppError::BadRequest {
                    message: "identity.user.password_hash_unchanged",
                },
                UserApplicationError::Storage(error) => AppError::Internal(error),
            },
            IdentityApplicationError::Crypto(error) => match error {
                CryptoApplicationError::InvalidTokenType => AppError::Unauthorized {
                    message: "identity.authentication.invalid_token_type",
                },
                CryptoApplicationError::InvalidToken => AppError::Unauthorized {
                    message: "identity.authentication.invalid_token",
                },
                error => AppError::Internal(error.into()),
            },
        }
    }
}

impl From<LibraryApplicationError> for AppError {
    fn from(value: LibraryApplicationError) -> Self {
        match value {
            LibraryApplicationError::Tag(error) => error.into(),
            LibraryApplicationError::Book(b) => match b {
                BookApplicationError::InvalidId => AppError::BadRequest {
                    message: "library.book.invalid_id",
                },
                BookApplicationError::MissingTitle => AppError::BadRequest {
                    message: "library.book.missing_title",
                },
                BookApplicationError::InvalidTitleFormat => AppError::BadRequest {
                    message: "library.book.invalid_title_format",
                },
                BookApplicationError::NotFound => AppError::NotFound {
                    message: "library.book.not_found",
                },
                BookApplicationError::TitleConflict => AppError::Conflict {
                    message: "library.book.title_conflict",
                },
                BookApplicationError::TitleUnchanged => AppError::BadRequest {
                    message: "library.book.title_unchanged",
                },
                BookApplicationError::AlreadyDeleted => AppError::Conflict {
                    message: "library.book.already_deleted",
                },
                BookApplicationError::NotDeleted => AppError::Conflict {
                    message: "library.book.not_deleted",
                },
                BookApplicationError::Storage(error) => AppError::Internal(error),
            },
            LibraryApplicationError::BookFile(f) => match f {
                BookFileApplicationError::UploadConflict => AppError::Conflict {
                    message: "library.book.file.upload.conflict",
                },
                BookFileApplicationError::AlreadyExists => AppError::Conflict {
                    message: "library.book.file.already_exists",
                },
                BookFileApplicationError::InvalidHashFormat => AppError::BadRequest {
                    message: "library.book.file.invalid_hash_format",
                },
                BookFileApplicationError::HashMismatch => AppError::BadRequest {
                    message: "library.book.file.hash_mismatch",
                },
                BookFileApplicationError::NotFound => AppError::NotFound {
                    message: "library.book.file.not_found",
                },
                BookFileApplicationError::Unsupported => AppError::BadRequest {
                    message: "library.book.file.unsupported",
                },
                BookFileApplicationError::Storage(error) => AppError::Internal(error),
            },
        }
    }
}

impl From<TagApplicationError> for AppError {
    fn from(value: TagApplicationError) -> Self {
        match value {
            TagApplicationError::Domain(error) => match error {
                TagDomainError::InvalidId => Self::BadRequest {
                    message: "library.tag.invalid_id",
                },
                TagDomainError::MissingName => Self::BadRequest {
                    message: "library.tag.missing_name",
                },
                TagDomainError::InvalidNameFormat => Self::BadRequest {
                    message: "library.tag.invalid_name_format",
                },
                TagDomainError::NameUnchanged => Self::BadRequest {
                    message: "library.tag.name_unchanged",
                },
                TagDomainError::InvalidColor => Self::BadRequest {
                    message: "library.tag.invalid_color",
                },
                TagDomainError::ColorUnchanged => Self::BadRequest {
                    message: "library.tag.color_unchanged",
                },
                TagDomainError::InvalidValueRegex => Self::BadRequest {
                    message: "library.tag.invalid_value_regex",
                },
                TagDomainError::ValueRegexNotAllowed => Self::BadRequest {
                    message: "library.tag.value_regex_not_allowed",
                },
                TagDomainError::ValueRequired => Self::BadRequest {
                    message: "library.tag.value_required",
                },
                TagDomainError::ValueNotAllowed => Self::BadRequest {
                    message: "library.tag.value_not_allowed",
                },
                TagDomainError::ValueRegexMismatch => Self::BadRequest {
                    message: "library.tag.value_regex_mismatch",
                },
                TagDomainError::AlreadyDeleted => Self::Conflict {
                    message: "library.tag.already_deleted",
                },
                TagDomainError::NotDeleted => Self::Conflict {
                    message: "library.tag.not_deleted",
                },
                TagDomainError::NotFound => Self::NotFound {
                    message: "library.tag.not_found",
                },
                TagDomainError::ParentNotFound => Self::NotFound {
                    message: "library.tag.parent_not_found",
                },
                TagDomainError::InvalidMove => Self::BadRequest {
                    message: "library.tag.invalid_move",
                },
                TagDomainError::InvalidTree => Self::Internal(error.into()),
            },
            TagApplicationError::ParentNotFound => Self::NotFound {
                message: "library.tag.parent_not_found",
            },
            TagApplicationError::SiblingNameConflict => Self::Conflict {
                message: "library.tag.sibling_name_conflict",
            },
            TagApplicationError::Storage(error) => Self::Internal(error),
        }
    }
}

impl From<BookFileAPIError> for AppError {
    fn from(value: BookFileAPIError) -> Self {
        match value {
            BookFileAPIError::DuplicateHashField => AppError::BadRequest {
                message: "library.book.file.upload.duplicate_hash",
            },
            BookFileAPIError::MissingHashField => AppError::BadRequest {
                message: "library.book.file.upload.missing_hash",
            },
            BookFileAPIError::MissingFileField => AppError::BadRequest {
                message: "library.book.file.upload.missing_file",
            },
        }
    }
}

impl From<MultipartError> for AppError {
    fn from(value: MultipartError) -> Self {
        AppError::Internal(anyhow::Error::new(value))
    }
}

#[cfg(test)]
mod tests {
    use axum::{body::to_bytes, http::StatusCode, response::IntoResponse};
    use shiro_error_catalog::ErrorCode;
    use shiro_http_contract::ErrorResponse;

    use super::*;

    async fn assert_app_error(
        err: impl Into<AppError>,
        expected_status: StatusCode,
        expected_code: ErrorCode,
    ) -> ErrorResponse {
        let resp = err.into().into_response();
        assert_eq!(resp.status(), expected_status);
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let parsed: ErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert_ne!(parsed.error.code, ErrorCode::Unknown.code());
        assert_eq!(parsed.error.code, expected_code.code());
        assert_eq!(parsed.error.message, expected_code.key());
        parsed
    }

    #[tokio::test]
    async fn all_application_and_api_errors_map_to_known_http_statuses_and_catalog_codes() {
        let storage_err = || anyhow::anyhow!("internal sqlite detail");

        for (err, status, code) in [
            (
                AuthenticationApplicationError::InvalidCredentials,
                StatusCode::UNAUTHORIZED,
                ErrorCode::AuthenticationInvalidCredentials,
            ),
            (
                AuthenticationApplicationError::MissingBearerToken,
                StatusCode::UNAUTHORIZED,
                ErrorCode::AuthenticationMissingBearerToken,
            ),
            (
                AuthenticationApplicationError::InvalidToken,
                StatusCode::UNAUTHORIZED,
                ErrorCode::AuthenticationInvalidToken,
            ),
            (
                AuthenticationApplicationError::NotFound,
                StatusCode::NOT_FOUND,
                ErrorCode::AuthenticationUserNotFound,
            ),
            (
                AuthenticationApplicationError::Storage(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(err, status, code).await;
        }

        for (err, status, code) in [
            (
                UserApplicationError::InvalidId,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserInvalidId,
            ),
            (
                UserApplicationError::MissingUserName,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserMissingName,
            ),
            (
                UserApplicationError::InvalidUserNameFormat,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserInvalidNameFormat,
            ),
            (
                UserApplicationError::InvalidPasswordHash,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserInvalidPasswordHash,
            ),
            (
                UserApplicationError::NotFound,
                StatusCode::NOT_FOUND,
                ErrorCode::UserNotFound,
            ),
            (
                UserApplicationError::NameConflict,
                StatusCode::CONFLICT,
                ErrorCode::UserNameConflict,
            ),
            (
                UserApplicationError::NameUnchanged,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserNameUnchanged,
            ),
            (
                UserApplicationError::PasswordHashUnchanged,
                StatusCode::BAD_REQUEST,
                ErrorCode::UserPasswordHashUnchanged,
            ),
            (
                UserApplicationError::Storage(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(IdentityApplicationError::User(err), status, code).await;
        }

        for (err, status, code) in [
            (
                CryptoApplicationError::InvalidTokenType,
                StatusCode::UNAUTHORIZED,
                ErrorCode::AuthenticationInvalidTokenType,
            ),
            (
                CryptoApplicationError::InvalidToken,
                StatusCode::UNAUTHORIZED,
                ErrorCode::AuthenticationInvalidToken,
            ),
            (
                CryptoApplicationError::SaltGeneration,
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
            (
                CryptoApplicationError::PasswordHashParse("parse".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
            (
                CryptoApplicationError::PasswordHash("hash".into()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
            (
                CryptoApplicationError::Internal(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(IdentityApplicationError::Crypto(err), status, code).await;
        }

        for (err, status, code) in [
            (
                BookApplicationError::InvalidId,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookInvalidId,
            ),
            (
                BookApplicationError::MissingTitle,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookMissingTitle,
            ),
            (
                BookApplicationError::InvalidTitleFormat,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookInvalidTitleFormat,
            ),
            (
                BookApplicationError::NotFound,
                StatusCode::NOT_FOUND,
                ErrorCode::BookNotFound,
            ),
            (
                BookApplicationError::TitleConflict,
                StatusCode::CONFLICT,
                ErrorCode::BookTitleConflict,
            ),
            (
                BookApplicationError::TitleUnchanged,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookTitleUnchanged,
            ),
            (
                BookApplicationError::AlreadyDeleted,
                StatusCode::CONFLICT,
                ErrorCode::BookAlreadyDeleted,
            ),
            (
                BookApplicationError::NotDeleted,
                StatusCode::CONFLICT,
                ErrorCode::BookNotDeleted,
            ),
            (
                BookApplicationError::Storage(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(LibraryApplicationError::Book(err), status, code).await;
        }

        for (err, status, code) in [
            (
                BookFileApplicationError::UploadConflict,
                StatusCode::CONFLICT,
                ErrorCode::BookFileUploadConflict,
            ),
            (
                BookFileApplicationError::AlreadyExists,
                StatusCode::CONFLICT,
                ErrorCode::BookFileAlreadyExists,
            ),
            (
                BookFileApplicationError::InvalidHashFormat,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileInvalidHashFormat,
            ),
            (
                BookFileApplicationError::HashMismatch,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileHashMismatch,
            ),
            (
                BookFileApplicationError::NotFound,
                StatusCode::NOT_FOUND,
                ErrorCode::BookFileNotFound,
            ),
            (
                BookFileApplicationError::Unsupported,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileUnsupported,
            ),
            (
                BookFileApplicationError::Storage(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(LibraryApplicationError::BookFile(err), status, code).await;
        }

        for (err, status, code) in [
            (
                TagApplicationError::Domain(TagDomainError::InvalidId),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagInvalidId,
            ),
            (
                TagApplicationError::Domain(TagDomainError::MissingName),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagMissingName,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidNameFormat),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagInvalidNameFormat,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NameUnchanged),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagNameUnchanged,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidColor),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagInvalidColor,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ColorUnchanged),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagColorUnchanged,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidValueRegex),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagInvalidValueRegex,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRegexNotAllowed),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagValueRegexNotAllowed,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRequired),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagValueRequired,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueNotAllowed),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagValueNotAllowed,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ValueRegexMismatch),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagValueRegexMismatch,
            ),
            (
                TagApplicationError::Domain(TagDomainError::AlreadyDeleted),
                StatusCode::CONFLICT,
                ErrorCode::TagAlreadyDeleted,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NotDeleted),
                StatusCode::CONFLICT,
                ErrorCode::TagNotDeleted,
            ),
            (
                TagApplicationError::Domain(TagDomainError::NotFound),
                StatusCode::NOT_FOUND,
                ErrorCode::TagNotFound,
            ),
            (
                TagApplicationError::Domain(TagDomainError::ParentNotFound),
                StatusCode::NOT_FOUND,
                ErrorCode::TagParentNotFound,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidMove),
                StatusCode::BAD_REQUEST,
                ErrorCode::TagInvalidMove,
            ),
            (
                TagApplicationError::Domain(TagDomainError::InvalidTree),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
            (
                TagApplicationError::ParentNotFound,
                StatusCode::NOT_FOUND,
                ErrorCode::TagParentNotFound,
            ),
            (
                TagApplicationError::SiblingNameConflict,
                StatusCode::CONFLICT,
                ErrorCode::TagSiblingNameConflict,
            ),
            (
                TagApplicationError::Storage(storage_err()),
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::CommonInternalServerError,
            ),
        ] {
            assert_app_error(LibraryApplicationError::Tag(err), status, code).await;
        }

        for (err, status, code) in [
            (
                BookFileAPIError::DuplicateHashField,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileUploadDuplicateHash,
            ),
            (
                BookFileAPIError::MissingHashField,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileUploadMissingHash,
            ),
            (
                BookFileAPIError::MissingFileField,
                StatusCode::BAD_REQUEST,
                ErrorCode::BookFileUploadMissingFile,
            ),
        ] {
            assert_app_error(err, status, code).await;
        }

        assert_app_error(
            AppError::PayloadTooLarge,
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::CommonPayloadTooLarge,
        )
        .await;
    }

    #[tokio::test]
    async fn internal_errors_redact_details_and_recognize_upload_size_markers() {
        let resp = AppError::Internal(anyhow::anyhow!("sensitive db secret")).into_response();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let bytes = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
        let text = std::str::from_utf8(&bytes).unwrap();
        assert!(!text.contains("sensitive db secret"));
        let parsed: ErrorResponse = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            parsed.error.code,
            ErrorCode::CommonInternalServerError.code()
        );
        assert_eq!(
            parsed.error.message,
            ErrorCode::CommonInternalServerError.key()
        );

        assert_app_error(
            AppError::Internal(anyhow::Error::new(NetUploadSizeExceeded)),
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::CommonPayloadTooLarge,
        )
        .await;
        assert_app_error(
            AppError::Internal(anyhow::Error::new(axum::Error::new(std::io::Error::other(
                "aborted",
            )))),
            StatusCode::PAYLOAD_TOO_LARGE,
            ErrorCode::CommonPayloadTooLarge,
        )
        .await;
    }
}
