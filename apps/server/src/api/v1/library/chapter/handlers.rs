use axum::{Json, extract::Path, http::StatusCode};
use shiro_application::library::book::{
    commands::{CreateChapterCommand, DeleteChapterCommand}, dto::ChapterOutput, queries::ListChaptersQuery,
};
use shiro_http_contract::CreateChapterRequest;

use crate::{api::extractors::auth::RoutedState, error::AppError};

pub async fn add_chapter(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<CreateChapterRequest>,
) -> Result<(StatusCode, Json<ChapterOutput>), AppError> {
    let output = state
        .book_service()
        .create_chapter(CreateChapterCommand {
            book_id,
            position: request.position,
            title: request.title,
            location: request.location,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(output)))
}

pub async fn delete_chapter(
    state: RoutedState,
    Path((book_id, chapter_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .delete_chapter(DeleteChapterCommand {
            book_id,
            chapter_id,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_chapters(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<ChapterOutput>>, AppError> {
    let chapters = state
        .book_service()
        .list_chapters(ListChaptersQuery { book_id })
        .await?;
    Ok(Json(chapters))
}
