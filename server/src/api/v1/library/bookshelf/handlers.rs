use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::dto::*},
    error::AppError,
    state::AppState,
    shared::time,
};


pub async fn create_bookshlef(
    State(state): State<AppState>,
    Json(request): Json<CreateBookshelfRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateBookshelfResponse>>), AppError> {
    todo!()
}


pub async fn delete_bookshlef(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
) -> Result<StatusCode, AppError> {
    todo!()
}