use axum::{Json, extract::State, http::StatusCode};

use crate::{
    api::v1::auth::dto::{
        LoginRequest, LoginResponse, RefreshAccessTokenRequest, RefreshAccessTokenResponse,
    },
    application::identity::authentication::commands::{LoginCommand, RefreshAccessTokenCommand},
    error::AppError,
    state::AppState,
};

pub async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<(StatusCode, Json<LoginResponse>), AppError> {
    let output = state
        .auth_service
        .login(LoginCommand {
            username: request.username,
            password: request.password,
        })
        .await?;

    Ok((StatusCode::OK, Json(output.into())))
}

pub async fn refresh_access_token(
    State(state): State<AppState>,
    Json(request): Json<RefreshAccessTokenRequest>,
) -> Result<(StatusCode, Json<RefreshAccessTokenResponse>), AppError> {
    let output = state
        .auth_service
        .refresh_access_token(RefreshAccessTokenCommand {
            token: request.token,
        })
        .await?;

    Ok((StatusCode::OK, Json(output.into())))
}
