use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};

use crate::{
    api::{response::DataResponse, v1::library::book::dto::*},
    application::library::book::commands::{
        CreateBookCommand, DeleteBookCommand, GetBookCommand, RenameBookCommand,
    },
    error::AppError,
    state::AppState,
};

pub async fn create_book(
    State(state): State<AppState>,
    Json(request): Json<CreateBookRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateBookResponse>>), AppError> {
    let output = state
        .book_service
        .create_book(CreateBookCommand {
            title: request.title,
            hash: request.hash,
            bookshelf_id: request.bookshelf_id,
            folder_id: request.folder_id,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(DataResponse::new(output.into()))))
}

pub async fn delete_book(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service
        .delete_book(DeleteBookCommand { id: book_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_book(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    Json(request): Json<RenameBookRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service
        .rename_book(RenameBookCommand {
            id: book_id,
            title: request.title,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_book(
    State(state): State<AppState>,
    Query(query): Query<BooksQuery>,
) -> Result<(StatusCode, Json<DataResponse<GetBookResponse>>), AppError> {
    let output = state
        .book_service
        .get_book(GetBookCommand { id: query.id })
        .await?;

    Ok((StatusCode::OK, Json(DataResponse::new(output.into()))))
}
