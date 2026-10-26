use axum::Router;

use crate::state::AppState;

pub mod auth;
pub mod library;

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/library", library::router())
        .nest("/auth", auth::router())
}
