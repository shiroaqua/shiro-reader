use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};

use crate::{
    api::{response::DataResponse, v1::library::bookshelf::dto::*},
    application::library::bookshelf::commands::{
        CreateBookshelfCommand, DeleteBookshelfCommand, GetBookshelfCommand,
        RenameBookshelfCommand,
    },
    error::AppError,
    state::AppState,
};

pub async fn create_bookshelf(
    State(state): State<AppState>,
    Json(request): Json<CreateBookshelfRequest>,
) -> Result<(StatusCode, Json<DataResponse<CreateBookshelfResponse>>), AppError> {
    let output = state
        .bookshelf_service
        .create_bookshelf(CreateBookshelfCommand { name: request.name })
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(DataResponse::new(output.into())),
    ))
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


pub async fn get_bookshelf(
    State(state): State<AppState>,
    Path(bookshelf_id): Path<String>,
) -> Result<(StatusCode, Json<DataResponse<GetBookshelfResponse>>), AppError> {
    let output = state
        .bookshelf_service
        .get_bookshelf(GetBookshelfCommand { id: bookshelf_id })
        .await?;

    Ok((StatusCode::OK, Json(DataResponse::new(output.into()))))
}

pub async fn list_bookshelf(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<DataResponse<ListBookshelfResponse>>), AppError> {
    let output = state.bookshelf_service.get_all_bookshelf().await?;
    Ok((
        StatusCode::OK,
        Json(DataResponse::new(ListBookshelfResponse(
            output.0.into_iter().map(Into::into).collect(),
        ))),
    ))
}
