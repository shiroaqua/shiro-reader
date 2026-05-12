use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::folder::dto::*},
    application::library::bookshelf::folder::commands::{CreateFolderCommand, DeleteFolderCommand},
    error::AppError,
    shared::time,
    state::AppState,
};

pub async fn create_folder(
    State(state): State<AppState>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateFolderResponse>>), AppError> {
    let output = state
        .folder_service
        .create_folder(CreateFolderCommand {
            bookshelf_id: request.bookshelf_id,
            parent_id: request.parent_id,
            name: request.name,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CreateFolderResponse {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        })),
    ))
}

pub async fn delete_folder(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
    Path(folder_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .folder_service
        .delete_folder(DeleteFolderCommand {
            bookshelf_id,
            folder_id,
        })
        .await?;
    todo!();
}
