use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::bookshelf::dto::*},
    application::bookshelf::commands::CreateDirectoryCommand,
    error::AppError,
    state::AppState,
    shared::time,
};

pub async fn create_directory(
    State(state): State<AppState>,
    Json(request): Json<CreateDirectoryRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateDirectoryResponse>>), AppError> {
    let output = state
        .directory_service
        .create_directory(CreateDirectoryCommand {
            parent_id: request.parent_id,
            name: request.name,
        })
        .await?;
    
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CreateDirectoryResponse {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        })),
    ))
}




pub async fn move_directory(
    State(state): State<AppState>,
    Path(directory_id): Path<String>,
    Json(request): Json<MoveDirectoryRequest>,
) -> Result<Json<DataResponse<MoveDirectoryResponse>>, AppError> {
    todo!();
}

pub async fn delete_directory(
    State(state): State<AppState>,
    Path(directory_id): Path<String>,
    Query(query): Query<DeleteDirectoryQuery>,
) -> Result<StatusCode, AppError> {
    todo!();
}

pub async fn get_directory_contents(
    State(state): State<AppState>,
    Path(directory_id): Path<String>,
    Query(query): Query<DirectoryContentsQuery>,
) -> Result<Json<DataResponse<BookshelfNode>>, AppError> {
    todo!();
}
