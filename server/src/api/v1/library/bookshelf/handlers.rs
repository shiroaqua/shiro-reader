use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::dto::*},
    application::library::bookshelf::commands::{CreateBookshelfCommand, DeleteBookshelfCommand},
    error::AppError,
    state::AppState,
    shared::time,
};

pub async fn create_bookshlef(
    State(state): State<AppState>,
    Json(request): Json<CreateBookshelfRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateBookshelfResponse>>), AppError> {
    let output = state
        .bookshelf_service
        .create_bookshelf(CreateBookshelfCommand { name: request.name })
        .await?;
    
    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(CreateBookshelfResponse {
            id: output.id.to_string(),
            created_at: time::ms_to_datetime(output.created_at),
        })),
    ))
}

pub async fn delete_bookshlef(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .bookshelf_service
        .delete_bookshelf(DeleteBookshelfCommand { id: bookshelf_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}
