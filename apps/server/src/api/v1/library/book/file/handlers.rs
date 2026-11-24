use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use axum::{
    body::Body,
    extract::{
        Multipart, Path, Query, State,
        multipart::{Field, MultipartError},
    },
    http::{
        HeaderValue, Response, StatusCode,
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    },
};
use futures_util::TryStreamExt;
use shiro_application::library::{
    book::file::errors::BookFileApplicationError, errors::LibraryApplicationError,
};
use shiro_error_catalog::ErrorCode;
use shiro_http_contract::{
    BOOK_FILE_FIELD_FILE, BOOK_FILE_FIELD_HASH, BOOK_FILE_FIELD_SIZE, BookFileQuery,
};
use tokio_util::io::{ReaderStream, StreamReader};

use crate::{
    api::{
        extractors::auth::RoutedState,
        v1::library::book::file::errors::{BookFileAPIError, NetUploadSizeExceeded},
    },
    error::AppError,
    state::AppState,
};

const MAX_MULTIPART_TEXT_FIELD_BYTES: usize = 128;

enum BoundedFieldError {
    Timeout,
    Multipart(MultipartError),
    TooLong,
    InvalidUtf8,
}

async fn read_bounded_text_field(
    mut field: Field<'_>,
    deadline: tokio::time::Instant,
    max_bytes: usize,
) -> Result<String, BoundedFieldError> {
    let mut buf = Vec::new();
    while let Some(chunk) = tokio::time::timeout_at(deadline, field.chunk())
        .await
        .map_err(|_| BoundedFieldError::Timeout)?
        .map_err(BoundedFieldError::Multipart)?
    {
        if buf.len().saturating_add(chunk.len()) > max_bytes {
            return Err(BoundedFieldError::TooLong);
        }
        buf.extend_from_slice(&chunk);
    }
    String::from_utf8(buf).map_err(|_| BoundedFieldError::InvalidUtf8)
}

fn parse_declared_size(raw: &str) -> Result<u64, BookFileAPIError> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BookFileAPIError::InvalidSizeField);
    }
    raw.parse::<u64>()
        .map_err(|_| BookFileAPIError::InvalidSizeField)
}

pub async fn upload_book(
    State(state): State<AppState>,
    _: RoutedState,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    let max_net_bytes = state.max_upload_size as u64;
    let header_deadline = tokio::time::Instant::now() + state.standard_request_timeout;
    let mut hash: Option<String> = None;
    let mut declared_size: Option<u64> = None;

    loop {
        let next = tokio::time::timeout_at(header_deadline, multipart.next_field())
            .await
            .map_err(|_| AppError::RequestTimeout {
                message: ErrorCode::BookFileUploadTimeout.key(),
            })??;

        let Some(field) = next else {
            return Err(BookFileAPIError::MissingFileField.into());
        };

        match field.name() {
            Some(BOOK_FILE_FIELD_HASH) => {
                if hash.is_some() {
                    return Err(BookFileAPIError::DuplicateHashField.into());
                }
                let text = read_bounded_text_field(
                    field,
                    header_deadline,
                    MAX_MULTIPART_TEXT_FIELD_BYTES,
                )
                .await
                .map_err(|err| match err {
                    BoundedFieldError::Timeout => AppError::RequestTimeout {
                        message: ErrorCode::BookFileUploadTimeout.key(),
                    },
                    BoundedFieldError::Multipart(err) => err.into(),
                    BoundedFieldError::TooLong | BoundedFieldError::InvalidUtf8 => {
                        LibraryApplicationError::BookFile(
                            BookFileApplicationError::InvalidHashFormat,
                        )
                        .into()
                    }
                })?;
                hash = Some(text);
            }
            Some(BOOK_FILE_FIELD_SIZE) => {
                if declared_size.is_some() {
                    return Err(BookFileAPIError::DuplicateSizeField.into());
                }
                let raw = read_bounded_text_field(
                    field,
                    header_deadline,
                    MAX_MULTIPART_TEXT_FIELD_BYTES,
                )
                .await
                .map_err(|err| match err {
                    BoundedFieldError::Timeout => AppError::RequestTimeout {
                        message: ErrorCode::BookFileUploadTimeout.key(),
                    },
                    BoundedFieldError::Multipart(err) => err.into(),
                    BoundedFieldError::TooLong | BoundedFieldError::InvalidUtf8 => {
                        BookFileAPIError::InvalidSizeField.into()
                    }
                })?;
                declared_size = Some(parse_declared_size(&raw)?);
            }
            Some(BOOK_FILE_FIELD_FILE) => {
                if field.file_name().is_none() {
                    continue;
                }
                let Some(hash) = hash.take() else {
                    return Err(BookFileAPIError::MissingHashField.into());
                };
                let Some(declared_size) = declared_size.take() else {
                    return Err(BookFileAPIError::MissingSizeField.into());
                };
                if declared_size > max_net_bytes {
                    return Err(AppError::PayloadTooLarge);
                }

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

                let result = state
                    .bookfile_service
                    .upload_book_file(hash, declared_size, reader)
                    .await;
                if size_exceeded.load(Ordering::Relaxed) {
                    return Err(AppError::PayloadTooLarge);
                }
                result?;

                return Ok(StatusCode::CREATED);
            }
            _ => continue,
        }
    }
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
    let _ = state.bookfile_service.probe_book_file(&query.hash)?;
    Ok(StatusCode::NO_CONTENT)
}

fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}
