use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;


use crate::application::library::bookshelf::folder::errors::FolderApplicationError;

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
    BadRequest {
        message: &'static str,
    },
    NotFound {
        message: &'static str,
    },
    Conflict {
        message: &'static str,
    },
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        
        let (status, message) = match self {
            AppError::BadRequest {message } => (StatusCode::BAD_REQUEST, message),
            AppError::NotFound { message } => (StatusCode::NOT_FOUND, message),
            AppError::Conflict {message } => (StatusCode::CONFLICT, message),
            AppError::Internal(error) => {
                error!(error = ?error, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "common.internal_server_error"
                )
            }
        };

        let body = ErrorResponse {
            error: ErrorDetail { message: message.to_owned() },
        };

        (status, Json(body)).into_response()
    }
}

impl From<FolderApplicationError> for AppError {
    fn from(value: FolderApplicationError) -> Self {
        use FolderApplicationError::*;

        match value {
            InvalidFolderId => AppError::BadRequest {
                message: "library.bookshelf.folder.invalid_id",
            },
            InvalidParentId => AppError::BadRequest {
                message: "library.bookshelf.folder.invalid_parent_id"
            },
            FolderIdRequired => AppError::BadRequest {
                message: "library.bookshelf.folder.id_required",
            },
            FolderNameRequired => AppError::BadRequest {
                message: "library.bookshelf.folder.name_required",
            },
            FolderNameInvalidFormat => AppError::BadRequest {
                message: "library.bookshelf.folder.name_invaild_format",
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
            Storage(error) => AppError::Internal(error),
        }
    }
}
