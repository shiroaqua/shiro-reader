use std::time::Duration;

use axum::Router;
use shiro_http_contract::{API_PREFIX, paths};

use crate::state::AppState;

pub mod extractors;
pub mod v1;

/// Standard metadata/JSON endpoints protected by the request timeout layer.
pub fn standard_router() -> Router<AppState> {
    Router::new()
        .route(paths::HEALTHZ, axum::routing::get(healthz))
        .nest(API_PREFIX, v1::standard_router())
}

/// Streaming file/cover transfer endpoints exempt from the short 30s total-request timeout
/// so slow-but-active large transfers (up to 512 MiB) are not aborted prematurely.
pub fn streaming_router(upload_body_limit: usize, standard_timeout: Duration) -> Router<AppState> {
    Router::new().nest(
        API_PREFIX,
        v1::streaming_router(upload_body_limit, standard_timeout),
    )
}

async fn healthz() -> &'static str {
    "ok"
}
