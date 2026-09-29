use crate::{
    api::{response::DataResponse, v1::library::bookshelf::dto::*},
    application::library::bookshelf::{
        commands::{CreateBookshelfCommand, DeleteBookshelfCommand, RenameBookshelfCommand},
        queries::GetBookshelvesQuery,
    },
    error::AppError,
    state::AppState,
};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use axum_extra::extract::Query;

pub async fn create_bookshelf(
    State(state): State<AppState>,
    Json(request): Json<CreateBookshelfRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateBookshelfResponse>>), AppError> {
    let output = state
        .bookshelf_service
        .create_bookshelf(CreateBookshelfCommand { name: request.name })
        .await?;

    Ok((StatusCode::CREATED, Json(DataResponse::new(output.into()))))
}

pub async fn rename_bookshelf(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
    Json(request): Json<RenameBookshelfRequest>,
) -> Result<StatusCode, AppError> {
    state
        .bookshelf_service
        .rename_bookshelf(RenameBookshelfCommand {
            id: bookshelf_id,
            name: request.name,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_bookshelf(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .bookshelf_service
        .delete_bookshelf(DeleteBookshelfCommand { id: bookshelf_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_bookshelves(
    State(state): State<AppState>,
    Query(query): Query<BookshelvesQuery>,
) -> Result<Response, AppError> {
    let response_data: Vec<GetBookshelfResponse> = state
        .bookshelf_service
        .get_bookshelves(GetBookshelvesQuery { ids: query.id })
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok((StatusCode::OK, Json(DataResponse::new(response_data))).into_response())
}
