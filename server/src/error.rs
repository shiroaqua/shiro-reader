use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tracing::error;


use crate::application::bookshelf::errors::DirectoryApplicationError;

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

impl From<DirectoryApplicationError> for AppError {
    fn from(value: DirectoryApplicationError) -> Self {
        use DirectoryApplicationError::*;

        match value {
            InvalidDirectoryId => AppError::BadRequest {
                message: "bookshelf.invalid_directory_id",
            },
            InvalidParentId => AppError::BadRequest {
                message: "bookshelf.invalid_parent_id"
            },
            DirectoryIdRequired => AppError::BadRequest {
                message: "bookshelf.directory_id_required",
            },
            DirectoryNameRequired => AppError::BadRequest {
                message: "bookshelf.directory_name_required",
            },
            DirectoryNameReserved => AppError::BadRequest {
                message: "bookshelf.directory_name_reserved",
            },
            DirectoryNameInvalidFormat => AppError::BadRequest {
                message: "bookshelf.directory_name_invaild_format",
            },
            DirectoryNotFound => AppError::NotFound {
                message: "bookshelf.directory_not_found",
            },
            ParentDirectoryNotFound => AppError::NotFound {
                message: "bookshelf.parent_directory_not_found",
            },
            DirectoryNameConflict => AppError::Conflict {
                message: "bookshelf.directory_name_conflict",
            },
            Storage(error) => AppError::Internal(error),
        }
    }
}
