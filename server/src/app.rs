use std::time::Duration;

use axum::{Router, http::StatusCode};
use tower_http::{
    cors::CorsLayer,
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{api, state::AppState};

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .merge(api::router())
        .layer(TimeoutLayer::with_status_code(StatusCode::REQUEST_TIMEOUT,Duration::from_secs(30)))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
