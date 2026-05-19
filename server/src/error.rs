use axum::{
    extract::multipart::MultipartError,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;

use crate::application::library::{
    book::{errors::BookApplicationError, file::errors::BookFileError},
    bookshelf::{errors::BookshelfApplicationError, folder::errors::FolderApplicationError},
    errors::LibraryApplicationError,
};

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ErrorDetail {
    pub message: String,
}

#[derive(Debug)]
pub enum AppError {
    BadRequest { message: &'static str },
    NotFound { message: &'static str },
    Conflict { message: &'static str },
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::BadRequest { message } => (StatusCode::BAD_REQUEST, message),
            AppError::NotFound { message } => (StatusCode::NOT_FOUND, message),
            AppError::Conflict { message } => (StatusCode::CONFLICT, message),
            AppError::Internal(error) => {
                let is_stream_aborted = error
                    .chain()
                    .any(|e| e.is::<axum::Error>());

                let is_multipart_limit = !is_stream_aborted && error
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
                message: message.to_owned(),
            },
        };

        (status, Json(body)).into_response()
    }
}

impl From<LibraryApplicationError> for AppError {
    fn from(value: LibraryApplicationError) -> Self {
        match value {
            LibraryApplicationError::Bookshelf(b) => match b {
                BookshelfApplicationError::InvalidId => AppError::BadRequest {
                    message: "library.bookshelf.invalid_id",
                },
                BookshelfApplicationError::MissingName => AppError::BadRequest {
                    message: "library.bookshelf.missing_name",
                },
                BookshelfApplicationError::InvalidNameFormat => AppError::BadRequest {
                    message: "library.bookshelf.invalid_name_format",
                },
                BookshelfApplicationError::NotFound => AppError::NotFound {
                    message: "library.bookshelf.not_found",
                },
                BookshelfApplicationError::NameConflict => AppError::Conflict {
                    message: "library.bookshelf.name_conflict",
                },
                BookshelfApplicationError::Storage(error) => AppError::Internal(error),
            },
            LibraryApplicationError::Folder(f) => match f {
                FolderApplicationError::InvalidId => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_id",
                },
                FolderApplicationError::InvalidParentId => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_parent_id",
                },
                FolderApplicationError::MissingFolderId => AppError::BadRequest {
                    message: "library.bookshelf.folder.missing_id",
                },
                FolderApplicationError::MissingName => AppError::BadRequest {
                    message: "library.bookshelf.folder.missing_name",
                },
                FolderApplicationError::InvalidNameFormat => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_name_format",
                },
                FolderApplicationError::NotFound => AppError::NotFound {
                    message: "library.bookshelf.folder.not_found",
                },
                FolderApplicationError::ParentNotFound => AppError::NotFound {
                    message: "library.bookshelf.folder.parent_not_found",
                },
                FolderApplicationError::NameConflict => AppError::Conflict {
                    message: "library.bookshelf.folder.name_conflict",
                },
                FolderApplicationError::Storage(error) => AppError::Internal(error),
            },
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
                BookApplicationError::NotFound => AppError::NotFound {
                    message: "library.book.not_found",
                },
                BookApplicationError::TitleConflict => AppError::Conflict {
                    message: "library.book.title_conflict",
                },
                BookApplicationError::LocationNotFound => AppError::NotFound {
                    message: "library.book.location_not_found",
                },
                BookApplicationError::Storage(error) => AppError::Internal(error),
            },
        }
    }
}

impl From<BookFileError> for AppError {
    fn from(value: BookFileError) -> Self {
        use BookFileError::*;

        match value {
            DuplicateHashField => AppError::BadRequest {
                message: "library.book.file.upload.duplicate_hash",
            },
            MissingHashField => AppError::BadRequest {
                message: "library.book.file.upload.missing_hash",
            },
            MissingFileField => AppError::BadRequest {
                message: "library.book.file.upload.missing_file",
            },
            UploadConflict => AppError::Conflict {
                message: "library.book.file.upload.conflict",
            },
            AlreadyExists => AppError::Conflict {
                message: "library.book.file.already_exists",
            },
            InvalidHashFormat => AppError::BadRequest {
                message: "library.book.file.invalid_hash_format",
            },
            NotFound => AppError::NotFound {
                message: "library.book.file.book_not_found",
            },
            Storage(error) => AppError::Internal(error),
        }
    }
}

impl From<MultipartError> for AppError {
    fn from(value: MultipartError) -> Self {
        AppError::Internal(anyhow::Error::new(value))
    }
}
