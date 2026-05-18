use crate::state::AppState;
use axum::{
    routing::{delete, get, post},
    Router,
};

pub mod book;
pub mod bookshelf;

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/books/files",
            get(book::file::handlers::get_book).post(book::file::handlers::upload_book),
        )
        .route(
            "/books/files/{book_hash}",
            get(book::file::handlers::download_book),
        )
        .route(
            "/bookshelves",
            get(bookshelf::handlers::get_bookshelves).post(bookshelf::handlers::create_bookshelf),
        )
        .route(
            "/bookshelves/{bookshelf_id}",
            delete(bookshelf::handlers::delete_bookshelf)
                .patch(bookshelf::handlers::rename_bookshelf),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders",
            post(bookshelf::folder::handlers::create_folder),
        )
        .route(
            "/bookshelves/{bookshelf_id}/folders/{folder_id}",
            delete(bookshelf::folder::handlers::delete_folder)
                .patch(bookshelf::folder::handlers::rename_folder),
        )
}
