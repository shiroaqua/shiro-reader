use crate::state::AppState;
use axum::{
    routing::{delete, post},
    Router,
};

pub mod bookshelf;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bookshelves", post(bookshelf::handlers::create_bookshelf))
        .route(
            "/bookshelves/{bookshelf_id}",
            delete(bookshelf::handlers::delete_bookshelf),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders",
            post(bookshelf::folder::handlers::create_folder),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders/{folder_id}",
            delete(bookshelf::folder::handlers::delete_folder),
        )
}
