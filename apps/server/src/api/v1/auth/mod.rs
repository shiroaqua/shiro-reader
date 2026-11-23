use axum::{
    Router,
    routing::{get, post},
};
use shiro_http_contract::paths;

use crate::state::AppState;

pub mod handlers;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(paths::AUTH_LOGOUT_ROUTE, post(handlers::logout))
        .route(paths::AUTH_LOGIN_ROUTE, post(handlers::login))
        .route(
            paths::AUTH_REFRESH_ROUTE,
            post(handlers::refresh_access_token),
        )
        .route(paths::AUTH_ME_ROUTE, get(handlers::me))
}
