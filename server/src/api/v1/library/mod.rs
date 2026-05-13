use crate::state::AppState;
use axum::{
    routing::{delete, post},
    Router,
};

pub mod bookshelf;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bookshelf", post(bookshelf::handlers::create_bookshelf))
        .route(
            "/bookshelf/{bookshelf_id}",
            delete(bookshelf::handlers::delete_bookshelf),
        )
        .route(
            "/bookshelf/{bookshelf_id}/folder",
            post(bookshelf::folder::handlers::create_folder),
        )
        .route(
            "/bookshelf/{bookshelf_id}/folder/{folder_id}",
            delete(bookshelf::folder::handlers::delete_folder),
        )
}
