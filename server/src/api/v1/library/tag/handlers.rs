use axum::{
    Json,
    extract::{Path, Query, rejection::{JsonRejection, QueryRejection}},
    http::StatusCode,
};

use crate::{
    api::extractors::auth::RoutedState,
    application::library::tag::commands::{
        BindBookTagCommand, CreateTagDefinitionCommand, DeleteTagDefinitionCommand, RenameTagCommand,
        UnbindBookTagCommand,
    },
    error::AppError,
};

use super::dto::{
    BookTagResponse, BindBookTagRequest, CreateTagRequest, DeleteTagQuery, RenameTagRequest, TagResponse,
    TagTreeNodeResponse, UnbindBookTagRequest,
};

pub async fn create_tag(
    state: RoutedState,
    request: Result<Json<CreateTagRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<TagResponse>), AppError> {
    let Json(request) = request.map_err(invalid_request)?;
    let output = state.tag_service.define_tag(CreateTagDefinitionCommand {
        name: request.name,
        parent_id: request.parent_id,
        allows_value: request.allows_value,
        value_regex: request.value_regex,
    }).await?;
    Ok((StatusCode::CREATED, Json(output.into())))
}

pub async fn rename_tag(
    state: RoutedState,
    Path(tag_id): Path<String>,
    request: Result<Json<RenameTagRequest>, JsonRejection>,
) -> Result<StatusCode, AppError> {
    let Json(request) = request.map_err(invalid_request)?;
    state.tag_service.rename_tag(RenameTagCommand {
        id: tag_id,
        name: request.name,
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_book_tags(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<BookTagResponse>>, AppError> {
    let output = state.tag_service.get_book_tags(book_id).await?;
    Ok(Json(output.into_iter().map(Into::into).collect()))
}

pub async fn bind_tag(
    state: RoutedState,
    Path(book_id): Path<String>,
    request: Result<Json<BindBookTagRequest>, JsonRejection>,
) -> Result<StatusCode, AppError> {
    let Json(request) = request.map_err(invalid_request)?;
    state.tag_service.bind_book_tag(BindBookTagCommand {
        book_id,
        tag_id: request.tag_id,
        value: request.value,
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unbind_tag(
    state: RoutedState,
    Path(book_id): Path<String>,
    request: Result<Json<UnbindBookTagRequest>, JsonRejection>,
) -> Result<StatusCode, AppError> {
    let Json(request) = request.map_err(invalid_request)?;
    state.tag_service.unbind_book_tag(UnbindBookTagCommand {
        book_id,
        tag_id: request.tag_id,
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_tree(
    state: RoutedState,
) -> Result<Json<Vec<TagTreeNodeResponse>>, AppError> {
    let output = state.tag_service.get_tree().await?;
    Ok(Json(output.into_iter().map(Into::into).collect()))
}

pub async fn delete_tag(
    state: RoutedState,
    Path(tag_id): Path<String>,
    query: Result<Query<DeleteTagQuery>, QueryRejection>,
) -> Result<StatusCode, AppError> {
    let Query(query) = query.map_err(|_| AppError::BadRequest {
        message: "library.tag.invalid_delete_strategy",
    })?;
    state.tag_service.delete_tag_definition(DeleteTagDefinitionCommand {
        id: tag_id,
        strategy: query.strategy.into(),
    }).await?;
    Ok(StatusCode::NO_CONTENT)
}

fn invalid_request(_error: JsonRejection) -> AppError {
    AppError::BadRequest { message: "library.tag.invalid_request" }
}
