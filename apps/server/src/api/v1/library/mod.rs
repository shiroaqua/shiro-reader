use std::time::Duration;

use axum::{
    Router,
    extract::DefaultBodyLimit,
    handler::Handler,
    http::StatusCode,
    routing::{delete, get, patch, post},
};
use shiro_http_contract::paths;
use tower_http::timeout::TimeoutLayer;

use crate::state::AppState;

pub mod book;
pub mod tag;

pub fn standard_router() -> Router<AppState> {
    Router::new()
        .route(
            paths::BOOK_ROUTE,
            delete(book::handlers::delete_book).patch(book::handlers::rename_book),
        )
        .route(
            paths::BOOK_NAVIGATION_ROUTE,
            get(book::handlers::get_book_navigation),
        )
        .route(
            paths::BOOK_TAGS_ROUTE,
            get(tag::handlers::get_book_tags)
                .post(tag::handlers::bind_tag)
                .delete(tag::handlers::unbind_tag),
        )
        .route(
            paths::BOOKS_ROUTE,
            get(book::handlers::get_books)
                .post(book::handlers::create_book)
                .delete(book::handlers::delete_books),
        )
        .route(
            paths::TRASH_ROUTE,
            get(book::handlers::get_trash).delete(book::handlers::permanently_delete_books),
        )
        .route(
            paths::TRASH_BOOK_ROUTE,
            delete(book::handlers::permanently_delete_book),
        )
        .route(
            paths::TRASH_RESTORE_ROUTE,
            post(book::handlers::restore_book),
        )
        .route(
            paths::TAGS_ROUTE,
            get(tag::handlers::get_tree).post(tag::handlers::create_tag),
        )
        .route(paths::TAG_ROUTE, delete(tag::handlers::delete_tag))
        .route(paths::TAG_NAME_ROUTE, patch(tag::handlers::rename_tag))
        .route(
            paths::TAG_COLOR_ROUTE,
            patch(tag::handlers::change_tag_color),
        )
        .route(paths::TAG_PARENT_ROUTE, patch(tag::handlers::move_tag))
}

pub fn streaming_router(upload_body_limit: usize, standard_timeout: Duration) -> Router<AppState> {
    let probe_get = book::file::handlers::get_book.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        standard_timeout,
    ));
    let head_file = book::handlers::head_book_file.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        standard_timeout,
    ));
    let upload_post =
        book::file::handlers::upload_book.layer(DefaultBodyLimit::max(upload_body_limit));

    Router::new()
        .route(paths::BOOK_FILES_ROUTE, get(probe_get).post(upload_post))
        .route(
            paths::BOOK_FILE_ROUTE,
            get(book::handlers::download_book_file).head(head_file),
        )
        .route(paths::BOOK_COVER_ROUTE, get(book::handlers::download_cover))
}
