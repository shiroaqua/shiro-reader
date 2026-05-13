use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;

use crate::application::library::{
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
                error!(error = ?error, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "common.internal_server_error",
                )
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
        use BookshelfApplicationError::*;
        use FolderApplicationError::*;

        match value {
            LibraryApplicationError::Bookshelf(b) => match b {
                InvalidBookshelfId => AppError::BadRequest {
                    message: "library.bookshelf.invalid_id",
                },
                MissingBookshelfName => AppError::BadRequest {
                    message: "library.bookshelf.missing_name",
                },
                InvalidBookshelfNameFormat => AppError::BadRequest {
                    message: "library.bookshelf.invalid_name_format",
                },
                BookshelfNotFound => AppError::NotFound {
                    message: "library.bookshelf.not_found",
                },
                BookshelfNameConflict => AppError::Conflict {
                    message: "library.bookshelf.name_conflict",
                },
                BookshelfApplicationError::Storage(error) => AppError::Internal(error),
            },
            LibraryApplicationError::Folder(f) => match f {
                InvalidFolderId => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_id",
                },
                InvalidParentFolderId => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_parent_id",
                },
                MissingFolderId => AppError::BadRequest {
                    message: "library.bookshelf.folder.missing_id",
                },
                MissingFolderName => AppError::BadRequest {
                    message: "library.bookshelf.folder.missing_name",
                },
                InvalidFolderNameFormat => AppError::BadRequest {
                    message: "library.bookshelf.folder.invalid_name_format",
                },
                FolderNotFound => AppError::NotFound {
                    message: "library.bookshelf.folder.not_found",
                },
                ParentFolderNotFound => AppError::NotFound {
                    message: "library.bookshelf.folder.parent_not_found",
                },
                FolderNameConflict => AppError::Conflict {
                    message: "library.bookshelf.folder.name_conflict",
                },
                FolderApplicationError::Storage(error) => AppError::Internal(error),
            },
        }
    }
}
