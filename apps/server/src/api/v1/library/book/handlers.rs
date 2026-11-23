use axum::{
    Json,
    body::Body,
    extract::Path,
    http::{Response, StatusCode, header::CONTENT_TYPE},
};
use axum_extra::extract::Query;
use shiro_http_contract::{BookIdsRequest, BooksQuery, CreateBookRequest, RenameBookRequest};

use crate::{
    api::extractors::auth::RoutedState,
    application::library::book::{
        commands::{
            CreateBookCommand, DeleteBookCommand, DeleteBooksCommand, GetBookCoverCommand,
            PermanentlyDeleteBooksCommand, RenameBookCommand, RestoreBookIdCommand,
        },
        dto::{CreateBookOutput, GetBookOutput, TrashedBookOutput},
        queries::GetBooksQuery,
    },
    error::AppError,
};

pub async fn create_book(
    state: RoutedState,
    Json(request): Json<CreateBookRequest>,
) -> Result<(StatusCode, Json<CreateBookOutput>), AppError> {
    let output = state
        .book_service()
        .create_book(CreateBookCommand {
            title: request.title,
            hash: request.hash,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(output)))
}

pub async fn delete_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .delete_book(DeleteBookCommand { id: book_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_books(
    state: RoutedState,
    Json(request): Json<BookIdsRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .delete_books(DeleteBooksCommand { ids: request.ids })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_trash(state: RoutedState) -> Result<Json<Vec<TrashedBookOutput>>, AppError> {
    let books = state.book_service().list_trashed_book().await?;
    Ok(Json(books))
}

pub async fn restore_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .restore_book(RestoreBookIdCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .permanently_delete_books(PermanentlyDeleteBooksCommand { ids: vec![book_id] })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_books(
    state: RoutedState,
    Json(request): Json<BookIdsRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .permanently_delete_books(PermanentlyDeleteBooksCommand { ids: request.ids })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_book(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<RenameBookRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
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
    let (stream, content_type) = state
        .book_service()
        .get_cover(GetBookCoverCommand { id: book_id })
        .await?;
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub async fn get_books(
    state: RoutedState,
    Query(query): Query<BooksQuery>,
) -> Result<(StatusCode, Json<Vec<GetBookOutput>>), AppError> {
    let response_data = state
        .book_service()
        .list_books(GetBooksQuery { ids: query.id })
        .await?;
    Ok((StatusCode::OK, Json(response_data)))
}
