use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::folder::dto::*},
    application::library::bookshelf::folder::commands::{
        CreateFolderCommand, DeleteFolderCommand, GetFoldersCommand, MoveFolderCommand,
        RenameFolderCommand,
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

    Ok((StatusCode::CREATED, Json(DataResponse::new(output.into()))))
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

pub async fn update_folder(
    State(state): State<AppState>,
    Path((bookshelf_id, folder_id)): Path<(String, String)>,
    Json(request): Json<UpdateFolderRequest>,
) -> Result<StatusCode, AppError> {
    if (request.name.is_none() && request.parent_id.is_none()) || (request.name.is_some() && request.parent_id.is_some()) {
        return Ok(StatusCode::BAD_REQUEST);
    }

    if let Some(name) = request.name {
        state
            .folder_service
            .rename_folder(RenameFolderCommand {
                bookshelf_id,
                folder_id,
                name,
            })
            .await?;
    }
    else if let Some(parent_id) = request.parent_id {
        state
            .folder_service
            .move_folder(MoveFolderCommand {
                bookshelf_id: bookshelf_id,
                folder_id: folder_id,
                parent_id: parent_id,
            })
            .await?;
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_folders(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
    Query(query): Query<GetFoldersQuery>,
) -> Result<(StatusCode, Json<DataResponse<GetFoldersResponse>>), AppError> {
    let output = state
        .folder_service
        .get_folders(GetFoldersCommand {
            bookshelf_id: bookshelf_id,
            id: query.id,
            recursive: query.recursive.is_some_and(|x| x),
        })
        .await?;

    Ok((StatusCode::OK, Json(DataResponse::new(output.into()))))
}
