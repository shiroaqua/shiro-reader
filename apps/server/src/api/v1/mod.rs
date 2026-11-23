use std::time::Duration;

use axum::{Json, Router, extract::State, routing::get};
use shiro_http_contract::{PROTOCOL_VERSION, SERVICE_NAME, ServerMetaResponse, paths};

use crate::state::AppState;

pub mod auth;
pub mod library;

pub fn standard_router() -> Router<AppState> {
    Router::new()
        .route(paths::META_ROUTE, get(meta))
        .nest(paths::LIBRARY_ROUTE, library::standard_router())
        .nest(paths::AUTH_ROUTE, auth::router())
}

pub fn streaming_router(upload_body_limit: usize, standard_timeout: Duration) -> Router<AppState> {
    Router::new().nest(
        paths::LIBRARY_ROUTE,
        library::streaming_router(upload_body_limit, standard_timeout),
    )
}

async fn meta(State(state): State<AppState>) -> Json<ServerMetaResponse> {
    Json(ServerMetaResponse {
        service: SERVICE_NAME.to_owned(),
        api_version: PROTOCOL_VERSION,
        server_version: env!("CARGO_PKG_VERSION").to_owned(),
        max_upload_bytes: state.max_upload_size as u64,
    })
}
