use axum::{
    Json,
    body::Body,
    extract::Path,
    http::{HeaderValue, Response, StatusCode, header::CONTENT_TYPE},
};
use axum_extra::extract::Query;

use crate::{
    api::{extractors::auth::RoutedState, v1::library::book::dto::*}, application::library::book::{
        commands::{CreateBookCommand, DeleteBookCommand, GetBookCoverCommand, RenameBookCommand},
        queries::GetBooksQuery,
    }, error::AppError,
};

pub async fn create_book(
    state: RoutedState,
    Json(request): Json<CreateBookRequest>,
) -> Result<(StatusCode, Json<CreateBookResponse>), AppError> {
    let output = state
        .book_service
        .create_book(CreateBookCommand {
            title: request.title,
            hash: request.hash,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(output.into())))
}

pub async fn delete_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service
        .delete_book(DeleteBookCommand { id: book_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_book(
    state: RoutedState,
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

pub async fn download_cover(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let stream = state
        .book_service
        .get_cover(GetBookCoverCommand { id: book_id })
        .await?;
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, HeaderValue::from_static("image/png"))
        .body(body)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub async fn get_books(
    state: RoutedState,
    Query(query): Query<BooksQuery>,
) -> Result<(StatusCode, Json<Vec<GetBookResponse>>), AppError> {
    let response_data: Vec<GetBookResponse> = state
        .book_service
        .get_books(GetBooksQuery {
            book_ids: query.id,
        })
        .await?
        .into_iter()
        .map(Into::into)
        .collect();
    Ok((StatusCode::OK, Json(response_data)))
}
