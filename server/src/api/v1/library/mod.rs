use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get},
};

pub mod book;

pub fn router() -> Router<AppState> {
    Router::new() 
        .route(
            "/files/book",
            get(book::file::handlers::get_book).post(book::file::handlers::upload_book),
        )
        .route(
            "/files/book/{book_hash}",
            get(book::file::handlers::download_book),
        )
        .route( 
            "/bookshelf/books/{book_id}",
            delete(book::handers::delete_book).patch(book::handers::rename_book),
        )
        .route(
            "/bookshelf/books/{book_id}/cover",
            get(book::handers::download_cover),
        )
        .route(
            "/bookshelf/books",
            get(book::handers::get_books).post(book::handers::create_book),
        )
}
