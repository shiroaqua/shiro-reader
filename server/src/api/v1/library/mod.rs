use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub mod bookshelf;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/bookshelves",
            get(bookshelf::handlers::list_bookshelf).post(bookshelf::handlers::create_bookshelf),
        )
        .route(
            "/bookshelves/{bookshelf_id}",
            get(bookshelf::handlers::get_bookshelf)
                .delete(bookshelf::handlers::delete_bookshelf)
                .patch(bookshelf::handlers::rename_bookshelf),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders",
            post(bookshelf::folder::handlers::create_folder),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders/{folder_id}",
            delete(bookshelf::folder::handlers::delete_folder)
                .patch(bookshelf::folder::handlers::rename_folder)
        )
}
