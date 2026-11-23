use axum::{Json, extract::State, http::StatusCode};
use shiro_http_contract::{LoginRequest, LogoutRequest, RefreshAccessTokenRequest};

use crate::{
    api::extractors::auth::RoutedState,
    application::identity::{
        authentication::{
            commands::{LoginCommand, LogoutCommand, RefreshAccessTokenCommand},
            dto::{LoginOutput, RefreshAccessTokenOutput},
            errors::AuthenticationApplicationError,
        },
        user::{dto::GetUserOutput, queries::GetUsersQuery},
    },
    error::AppError,
    state::AppState,
};

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<(StatusCode, Json<LoginOutput>), AppError> {
    let output = state
        .auth_service
        .login(LoginCommand {
            username: request.username,
            password: request.password,
        })
        .await?;

    Ok((StatusCode::OK, Json(output)))
}

pub async fn refresh_access_token(
    State(state): State<AppState>,
    Json(request): Json<RefreshAccessTokenRequest>,
) -> Result<(StatusCode, Json<RefreshAccessTokenOutput>), AppError> {
    let output = state
        .auth_service
        .refresh_access_token(RefreshAccessTokenCommand {
            token: request.token,
        })
        .await?;

    Ok((StatusCode::OK, Json(output)))
}

/// Idempotent for a valid refresh token, including an already revoked session.
pub async fn logout(
    State(state): State<AppState>,
    Json(request): Json<LogoutRequest>,
) -> Result<StatusCode, AppError> {
    state
        .auth_service
        .logout(LogoutCommand {
            token: request.token,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Returns the authenticated user's canonical identity without exposing tokens.
pub async fn me(
    State(state): State<AppState>,
    routed: RoutedState,
) -> Result<Json<GetUserOutput>, AppError> {
    let mut users = state
        .user_service
        .get_users(GetUsersQuery {
            ids: vec![routed.user_id],
        })
        .await?;
    let Some(user) = users.pop() else {
        return Err(AuthenticationApplicationError::NotFound.into());
    };
    Ok(Json(user))
}
