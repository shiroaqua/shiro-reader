use axum::{Router, routing::post};

use crate::state::AppState;

pub mod dto;
pub mod handlers;
pub mod mapper;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login))
        .route("/refresh", post(handlers::refresh_access_token))
}
