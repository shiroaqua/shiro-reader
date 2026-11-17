use axum::{
    Json,
    extract::multipart::MultipartError,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tracing::error;

use crate::{
    api::v1::library::book::file::errors::BookFileAPIError,
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

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub code: u32,
    pub message: String,
}

#[derive(Debug)]
pub enum AppError {
    BadRequest { message: &'static str },
    Unauthorized { message: &'static str },
    NotFound { message: &'static str },
    Conflict { message: &'static str },
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest { message } => (StatusCode::BAD_REQUEST, message),
            AppError::Unauthorized { message } => (StatusCode::UNAUTHORIZED, message),
            AppError::NotFound { message } => (StatusCode::NOT_FOUND, message),
            AppError::Conflict { message } => (StatusCode::CONFLICT, message),
            AppError::Internal(error) => {
                let is_stream_aborted = error.chain().any(|e| e.is::<axum::Error>());

                let is_multipart_limit = !is_stream_aborted
                    && error
                        .chain()
                        .find_map(|e| e.downcast_ref::<axum::extract::multipart::MultipartError>())
                        .is_some_and(|e| e.status() == StatusCode::PAYLOAD_TOO_LARGE);

                if is_multipart_limit || is_stream_aborted {
                    (StatusCode::PAYLOAD_TOO_LARGE, "common.payload_too_large")
                } else {
                    error!(error = ?error, "internal server error");
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "common.internal_server_error",
                    )
                }
            }
        };

        let body = ErrorResponse {
            error: ErrorDetail {
                code: error_code(message),
                message: message.to_owned(),
            },
        };

        (status, Json(body)).into_response()
    }
}

fn error_code(message: &str) -> u32 {
    match message {
        "identity.authentication.invalid_credentials" => 1001,
        "identity.authentication.missing_bearer_token" => 1002,
        "identity.authentication.invalid_token" => 1003,
        "identity.authentication.user_not_found" => 1004,
        "identity.authentication.invalid_token_type" => 1005,
        "identity.user.invalid_id" => 1101,
        "identity.user.missing_user_name" => 1102,
        "identity.user.invalid_user_name_format" => 1103,
        "identity.user.invalid_password_hash" => 1104,
        "identity.user.not_found" => 1105,
        "identity.user.name_conflict" => 1106,
        "identity.user.name_unchanged" => 1107,
        "identity.user.password_hash_unchanged" => 1108,
        "library.book.invalid_id" => 2001,
        "library.book.missing_title" => 2002,
        "library.book.invalid_title_format" => 2003,
        "library.book.invalid_hash_format" => 2004,
        "library.book.invalid_file" => 2005,
        "library.book.not_found" => 2006,
        "library.book.title_conflict" => 2007,
        "library.book.title_unchanged" => 2008,
        "library.book.already_deleted" => 2009,
        "library.book.not_deleted" => 2010,
        "library.book.version_conflict" => 2011,
        "library.book.file.upload.conflict" => 2101,
        "library.book.file.already_exists" => 2102,
        "library.book.file.invalid_hash_format" => 2103,
        "library.book.file.hash_mismatch" => 2104,
        "library.book.file.not_found" => 2105,
        "library.book.file.unsupported" => 2106,
        "library.book.file.upload.duplicate_hash" => 2110,
        "library.book.file.upload.missing_hash" => 2111,
        "library.book.file.upload.missing_file" => 2112,
        "library.tag.invalid_id" => 2201,
        "library.tag.missing_name" => 2202,
        "library.tag.invalid_name_format" => 2203,
        "library.tag.name_unchanged" => 2204,
        "library.tag.invalid_color" => 2205,
        "library.tag.color_unchanged" => 2206,
        "library.tag.invalid_value_regex" => 2207,
        "library.tag.value_regex_not_allowed" => 2208,
        "library.tag.value_required" => 2209,
        "library.tag.value_not_allowed" => 2210,
        "library.tag.value_regex_mismatch" => 2211,
        "library.tag.already_deleted" => 2212,
        "library.tag.not_deleted" => 2213,
        "library.tag.not_found" => 2214,
        "library.tag.parent_not_found" => 2215,
        "library.tag.invalid_move" => 2216,
        "library.tag.invalid_delete_strategy" => 2217,
        "library.tag.sibling_name_conflict" => 2218,
        "common.payload_too_large" => 9001,
        "common.internal_server_error" => 9002,
        _ => 9999,
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
                CryptoApplicationError::JsonWebToken(error) => {
                    use jsonwebtoken::errors::ErrorKind;

                    // JWT errors are shared by token verification and issuance.
                    // Only invalid client tokens should be reported as unauthorized.

                    match error.kind() {
                        ErrorKind::InvalidToken
                        | ErrorKind::InvalidSignature
                        | ErrorKind::InvalidAlgorithmName
                        | ErrorKind::MissingRequiredClaim(_)
                        | ErrorKind::InvalidClaimFormat(_)
                        | ErrorKind::ExpiredSignature
                        | ErrorKind::InvalidIssuer
                        | ErrorKind::InvalidAudience
                        | ErrorKind::InvalidSubject
                        | ErrorKind::ImmatureSignature
                        | ErrorKind::InvalidAlgorithm
                        | ErrorKind::Base64(_)
                        | ErrorKind::Json(_)
                        | ErrorKind::Utf8(_) => AppError::Unauthorized {
                            message: "identity.authentication.invalid_token",
                        },
                        _ => AppError::Internal(error.into()),
                    }
                }
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
                BookApplicationError::InvalidHashFormat => AppError::BadRequest {
                    message: "library.book.invalid_hash_format",
                },
                BookApplicationError::InvailFile => AppError::BadRequest {
                    message: "library.book.invalid_file",
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
                TagDomainError::InvalidTimestamp => Self::Internal(error.into()),
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
