use crate::state::AppState;
use axum::{
    Router,
    routing::{delete, get, patch},
};

pub mod book;
pub mod tag;

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
            "/bookshelf/books/{book_id}/tags",
            get(tag::handlers::get_book_tags)
                .post(tag::handlers::bind_tag)
                .delete(tag::handlers::unbind_tag),
        )
        .route(
            "/bookshelf/books",
            get(book::handers::get_books).post(book::handers::create_book),
        )
        .route(
            "/tags",
            get(tag::handlers::get_tree).post(tag::handlers::create_tag),
        )
        .route("/tags/{tag_id}", delete(tag::handlers::delete_tag))
        .route("/tags/{tag_id}/name", patch(tag::handlers::rename_tag))
        .route(
            "/tags/{tag_id}/color",
            patch(tag::handlers::change_tag_color),
        )
        .route("/tags/{tag_id}/parent", patch(tag::handlers::move_tag))
}
