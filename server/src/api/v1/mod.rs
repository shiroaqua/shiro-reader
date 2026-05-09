use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

pub mod bookshelf;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/bookshelf", post(bookshelf::handlers::create_directory))
        .route(
            "/bookshelf/{directory_id}",
            get(bookshelf::handlers::get_directory_contents)
                .patch(bookshelf::handlers::move_directory)
                .delete(bookshelf::handlers::delete_directory),
        )
}
