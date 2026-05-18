use crate::{
    api::{
        v1::library::book::file::dto::{BookQuery},
    },
    application::library::bookfile::errors::BookFileError,
    error::AppError,
    state::AppState,
};
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_TYPE},
        HeaderValue, Response, StatusCode,
    },
};
use futures_util::TryStreamExt;
use tokio_util::io::{ReaderStream, StreamReader};

pub async fn upload_book(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<StatusCode, AppError> {
    let mut hash: Option<String> = None;
    loop {
        let field = multipart.next_field().await?;

        match field {
            Some(field) => {
                let field_name = field.name().unwrap_or("").to_owned();
                match field_name.as_str() {
                    "hash" => {
                        // 不允许重复的 hash 字段
                        if hash.is_some() {
                            return Err(BookFileError::DuplicateHashField.into());
                        }

                        hash = Some(
                            field
                                .text()
                                .await?,
                        );
                    }
                    "file" => {
                        if field.file_name().is_none() {
                            continue;
                        }
                        if hash.is_none() {
                            return Err(BookFileError::MissingHashField.into());
                        }

                        let stream = field.map_err(std::io::Error::other);
                        let reader = StreamReader::new(stream);

                        state
                            .bookfile_service
                            .upload_book_file(hash.clone().unwrap(), reader)
                            .await?;

                        return Ok(StatusCode::CREATED);
                    }
                    _ => continue,
                }
            }
            None => break,
        }
    }
    Err(BookFileError::MissingFileField.into())
}

pub async fn download_book(
    State(state): State<AppState>,
    Path(book_hash): Path<String>,
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
    Query(query): Query<BookQuery>,
) -> Result<StatusCode, AppError> {
    if state.bookfile_service.contains(&query.hash)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(BookFileError::NotFound.into())
    }
}

fn header_value(value: &str) -> Result<HeaderValue, AppError> {
    HeaderValue::from_str(value).map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}
