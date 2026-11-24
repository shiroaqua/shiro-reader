use std::time::Duration;

use axum::{Router, http::StatusCode};
use tower_http::{timeout::TimeoutLayer, trace::TraceLayer};

use crate::{api, state::AppState};

/// Default request timeout for standard JSON / metadata endpoints.
pub const DEFAULT_STANDARD_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Extra multipart framing budget above the net book file upload size so a file of exact
/// `max_upload_size` bytes (e.g. 512 MiB) is never rejected due to multipart boundaries/headers.
pub const MULTIPART_BODY_OVERHEAD_BYTES: usize = 1024 * 1024;

pub fn build_router(state: AppState) -> Router {
    build_router_with_timeout(state, DEFAULT_STANDARD_REQUEST_TIMEOUT)
}

pub fn build_router_with_timeout(mut state: AppState, standard_timeout: Duration) -> Router {
    state.standard_request_timeout = standard_timeout;
    let standard = api::standard_router().layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        standard_timeout,
    ));
    let streaming = api::streaming_router(
        state
            .max_upload_size
            .saturating_add(MULTIPART_BODY_OVERHEAD_BYTES),
        standard_timeout,
    );

    Router::new()
        .merge(standard)
        .merge(streaming)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
