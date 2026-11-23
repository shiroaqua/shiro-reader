use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{
        HeaderValue, Response, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
};
use futures_util::TryStreamExt;
use shiro_http_contract::BookFileQuery;
use tokio_util::io::{ReaderStream, StreamReader};

use crate::{
    api::{
        extractors::auth::RoutedState,
        v1::library::book::file::errors::{BookFileAPIError, NetUploadSizeExceeded},
    },
    error::AppError,
    state::AppState,
};

pub async fn upload_book(
    State(state): State<AppState>,
    _: RoutedState,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    let mut hash: Option<String> = None;
    let max_net_bytes = state.max_upload_size as u64;

    loop {
        let field = multipart.next_field().await?;

        match field {
            Some(field) => match field.name() {
                Some("hash") => {
                    // 不允许重复的 hash 字段
                    if hash.is_some() {
                        return Err(BookFileAPIError::DuplicateHashField.into());
                    }

                    hash = Some(field.text().await?);
                }
                Some("file") => {
                    if field.file_name().is_none() {
                        continue;
                    }
                    let Some(hash) = hash.take() else {
                        return Err(BookFileAPIError::MissingHashField.into());
                    };

                    let mut received: u64 = 0;
                    let size_exceeded = Arc::new(AtomicBool::new(false));
                    let exceeded_flag = Arc::clone(&size_exceeded);
                    let stream = field.map_err(std::io::Error::other).and_then(move |chunk| {
                        received = received.saturating_add(chunk.len() as u64);
                        if received > max_net_bytes {
                            exceeded_flag.store(true, Ordering::Relaxed);
                            futures_util::future::err(std::io::Error::other(NetUploadSizeExceeded))
                        } else {
                            futures_util::future::ok(chunk)
                        }
                    });
                    let reader = StreamReader::new(stream);

                    let result = state.bookfile_service.upload_book_file(hash, reader).await;
                    if size_exceeded.load(Ordering::Relaxed) {
                        return Err(AppError::PayloadTooLarge);
                    }
                    result?;

                    return Ok(StatusCode::CREATED);
                }
                _ => continue,
            },
            None => break,
        }
    }
    Err(BookFileAPIError::MissingFileField.into())
}

pub async fn download_book(
    State(state): State<AppState>,
    Path(book_hash): Path<String>,
    _: RoutedState,
) -> Result<Response<Body>, AppError> {
    let file = state
        .bookfile_service
        .download_book_file(&book_hash)
        .await?;
    let body = Body::from_stream(ReaderStream::new(file));

    let filename = format!("{}.book", &book_hash);
    let content_disposition = format!("attachment; filename=\"{}\"", filename);
    let cd_header_value = HeaderValue::from_str(&content_disposition)
        .unwrap_or_else(|_| HeaderValue::from_static("attachment"));

    Response::builder()
        .status(StatusCode::OK)
        .header(
            CONTENT_TYPE,
            HeaderValue::from_static("application/octet-stream"),
        )
        .header(CONTENT_DISPOSITION, cd_header_value)
        .header("x-book-hash", header_value(&book_hash)?)
        .body(body)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub async fn get_book(
    State(state): State<AppState>,
    Query(query): Query<BookFileQuery>,
    _: RoutedState,
) -> Result<StatusCode, AppError> {
    let _ = state.bookfile_service.parse_existing_hash(&query.hash)?;
    Ok(StatusCode::NO_CONTENT)
}

fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}
