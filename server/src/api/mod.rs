use axum::Router;

use crate::state::AppState;

pub mod response;
pub mod v1;


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/healthz", axum::routing::get(healthz))
        .nest("/api/v1", v1::router())
}

async fn healthz() -> &'static str {
    "ok"
}
