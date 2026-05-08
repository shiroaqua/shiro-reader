use axum::{routing::{delete, get, patch, post}, Router};

use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
}
