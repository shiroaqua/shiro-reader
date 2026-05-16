use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::folder::dto::*},
    application::library::bookshelf::folder::commands::{
        CreateFolderCommand, DeleteFolderCommand, RenameFolderCommand,
    },
    error::AppError,
    state::AppState,
};

pub async fn create_folder(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
    Json(request): Json<CreateFolderRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateFolderResponse>>), AppError> {
    let output = state
        .folder_service
        .create_folder(CreateFolderCommand {
            bookshelf_id: bookshelf_id,
            parent_id: request.parent_id,
            name: request.name,
        })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(output.into())),
    ))
}

pub async fn rename_folder(
    State(state): State<AppState>,
    Path((bookshelf_id, folder_id)): Path<(String, String)>,
    Json(request): Json<RenameFolderRequest>,
) -> Result<StatusCode, AppError> {
    state
        .folder_service
        .rename_folder(RenameFolderCommand {
            bookshelf_id: bookshelf_id,
            folder_id: folder_id,
            name: request.name,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_folder(
    State(state): State<AppState>,
    Path((bookshelf_id, folder_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    state
        .folder_service
        .delete_folder(DeleteFolderCommand {
            bookshelf_id,
            folder_id,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
