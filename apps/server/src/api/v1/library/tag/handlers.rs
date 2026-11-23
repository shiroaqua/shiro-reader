use axum::{
    Json,
    extract::{Path, Query, rejection::QueryRejection},
    http::StatusCode,
};
use shiro_http_contract::{
    BindBookTagRequest, ChangeTagColorRequest, DefineTagRequest, DeleteTagQuery, MoveTagRequest,
    RenameTagRequest, UnbindBookTagRequest,
};

use crate::{
    api::{extractors::auth::RoutedState, v1::library::tag::mapper::map_delete_strategy},
    application::library::tag::{
        commands::{
            BindBookTagCommand, ChangeTagColorCommand, CreateTagDefinitionCommand, MoveTagCommand,
            RenameTagCommand, UnbindBookTagCommand, UndefineTagCommand,
        },
        dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
    },
    error::AppError,
};

pub async fn create_tag(
    state: RoutedState,
    Json(request): Json<DefineTagRequest>,
) -> Result<(StatusCode, Json<TagDefinitionOutput>), AppError> {
    let output = state
        .tag_service()
        .define_tag(CreateTagDefinitionCommand {
            name: request.name,
            parent_id: request.parent_id,
            color: request.color,
            allows_value: request.allows_value,
            value_regex: request.value_regex,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(output)))
}

pub async fn change_tag_color(
    state: RoutedState,
    Path(tag_id): Path<String>,
    Json(request): Json<ChangeTagColorRequest>,
) -> Result<StatusCode, AppError> {
    state
        .tag_service()
        .change_tag_color(ChangeTagColorCommand {
            id: tag_id,
            color: request.color,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_tag(
    state: RoutedState,
    Path(tag_id): Path<String>,
    Json(request): Json<RenameTagRequest>,
) -> Result<StatusCode, AppError> {
    state
        .tag_service()
        .rename_tag(RenameTagCommand {
            id: tag_id,
            name: request.name,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn move_tag(
    state: RoutedState,
    Path(tag_id): Path<String>,
    Json(request): Json<MoveTagRequest>,
) -> Result<StatusCode, AppError> {
    state
        .tag_service()
        .move_tag(MoveTagCommand {
            id: tag_id,
            parent_id: request.parent_id,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_book_tags(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<BookTagOutput>>, AppError> {
    let output = state.tag_service().get_book_tags(book_id).await?;
    Ok(Json(output))
}

pub async fn bind_tag(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<BindBookTagRequest>,
) -> Result<StatusCode, AppError> {
    state
        .tag_service()
        .bind_book_tag(BindBookTagCommand {
            book_id,
            tag_id: request.tag_id,
            value: request.value,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn unbind_tag(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<UnbindBookTagRequest>,
) -> Result<StatusCode, AppError> {
    state
        .tag_service()
        .unbind_book_tag(UnbindBookTagCommand {
            book_id,
            tag_id: request.tag_id,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_tree(state: RoutedState) -> Result<Json<Vec<TagTreeNodeOutput>>, AppError> {
    let output = state.tag_service().get_tree().await?;
    Ok(Json(output))
}

pub async fn delete_tag(
    state: RoutedState,
    Path(tag_id): Path<String>,
    query: Result<Query<DeleteTagQuery>, QueryRejection>,
) -> Result<StatusCode, AppError> {
    let Query(query) = query.map_err(|_| AppError::BadRequest {
        message: "library.tag.invalid_delete_strategy",
    })?;
    state
        .tag_service()
        .undefine_tag(UndefineTagCommand {
            id: tag_id,
            strategy: map_delete_strategy(query.strategy),
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
