use axum::{
    Json,
    body::Body,
    extract::Path,
    http::{
        HeaderMap, Response, StatusCode,
        header::{
            ACCEPT_RANGES, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE, ETAG,
            IF_RANGE, RANGE,
        },
    },
};
use axum_extra::extract::Query;
use shiro_domain::library::book::value_objects::BookFileType;
use shiro_error_catalog::ErrorCode;
use shiro_http_contract::{
    BookIdsRequest, BooksQuery, ChangeBookAuthorsRequest, ChangeBookDescriptionRequest,
    ChangeBookIdentifierRequest, ChangeBookLanguageRequest, ChangeBookPublishedDateRequest,
    ChangeBookPublisherRequest, ChangeBookSubtitleRequest, CreateBookRequest, RenameBookRequest,
};
use tokio_util::io::ReaderStream;

use crate::{
    api::extractors::auth::RoutedState,
    application::library::book::{
        commands::{
            ChangeBookAuthorsCommand, ChangeBookDescriptionCommand, ChangeBookIdentifierCommand,
            ChangeBookLanguageCommand, ChangeBookPublishedDateCommand, ChangeBookPublisherCommand,
            ChangeBookSubtitleCommand, ClearBookAuthorsCommand, ClearBookDescriptionCommand,
            ClearBookIdentifierCommand, ClearBookLanguageCommand, ClearBookPublishedDateCommand,
            ClearBookPublisherCommand, ClearBookSubtitleCommand, CreateBookCommand,
            DeleteBookCommand, DeleteBooksCommand, GetBookCoverCommand, GetBookFileMetaCommand,
            OpenBookFileRangeCommand, PermanentlyDeleteBooksCommand, RenameBookCommand,
            RestoreBookIdCommand,
        },
        dto::{
            BookFileDescriptor, BookMetadataOutput, BookNavigationOutput, CreateBookOutput,
            GetBookOutput, TrashedBookOutput,
        },
        queries::{GetBookMetadataQuery, GetBookNavigationQuery, GetBooksQuery},
    },
    error::AppError,
};

use super::range::{RangeParseError, if_range_matches_strong_etag, parse_single_byte_range};

pub async fn create_book(
    state: RoutedState,
    Json(request): Json<CreateBookRequest>,
) -> Result<(StatusCode, Json<CreateBookOutput>), AppError> {
    let output = state
        .book_service()
        .create_book(CreateBookCommand {
            title: request.title,
            hash: request.hash,
        })
        .await?;

    Ok((StatusCode::CREATED, Json(output)))
}

pub async fn delete_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .delete_book(DeleteBookCommand { id: book_id })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_books(
    state: RoutedState,
    Json(request): Json<BookIdsRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .delete_books(DeleteBooksCommand { ids: request.ids })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn get_trash(state: RoutedState) -> Result<Json<Vec<TrashedBookOutput>>, AppError> {
    let books = state.book_service().list_trashed_book().await?;
    Ok(Json(books))
}

pub async fn restore_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .restore_book(RestoreBookIdCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_book(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .permanently_delete_books(PermanentlyDeleteBooksCommand { ids: vec![book_id] })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn permanently_delete_books(
    state: RoutedState,
    Json(request): Json<BookIdsRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .permanently_delete_books(PermanentlyDeleteBooksCommand { ids: request.ids })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn rename_book(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<RenameBookRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .rename_book(RenameBookCommand {
            id: book_id,
            title: request.title,
        })
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

pub async fn download_cover(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let (stream, content_type) = state
        .book_service()
        .get_cover(GetBookCoverCommand { id: book_id })
        .await?;
    let body = Body::from_stream(stream);

    Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, content_type)
        .body(body)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}

pub async fn get_books(
    state: RoutedState,
    Query(query): Query<BooksQuery>,
) -> Result<(StatusCode, Json<Vec<GetBookOutput>>), AppError> {
    let response_data = state
        .book_service()
        .list_books(GetBooksQuery { ids: query.id })
        .await?;
    Ok((StatusCode::OK, Json(response_data)))
}

pub async fn get_book_navigation(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Json<BookNavigationOutput>, AppError> {
    let output = state
        .book_service()
        .get_book_navigation(GetBookNavigationQuery { id: book_id })
        .await?;
    Ok(Json(output))
}

pub async fn get_book_metadata(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Json<BookMetadataOutput>, AppError> {
    let output = state
        .book_service()
        .get_book_metadata(GetBookMetadataQuery { id: book_id })
        .await?;
    Ok(Json(output))
}

pub async fn change_book_subtitle(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookSubtitleRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_subtitle(ChangeBookSubtitleCommand {
            id: book_id,
            subtitle: request.subtitle,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_subtitle(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_subtitle(ClearBookSubtitleCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_authors(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookAuthorsRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_authors(ChangeBookAuthorsCommand {
            id: book_id,
            authors: request.authors,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_authors(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_authors(ClearBookAuthorsCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_publisher(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookPublisherRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_publisher(ChangeBookPublisherCommand {
            id: book_id,
            publisher: request.publisher,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_publisher(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_publisher(ClearBookPublisherCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_published_date(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookPublishedDateRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_published_date(ChangeBookPublishedDateCommand {
            id: book_id,
            published_date: request.published_date,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_published_date(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_published_date(ClearBookPublishedDateCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_language(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookLanguageRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_language(ChangeBookLanguageCommand {
            id: book_id,
            language: request.language,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_language(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_language(ClearBookLanguageCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_description(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookDescriptionRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_description(ChangeBookDescriptionCommand {
            id: book_id,
            description: request.description,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_description(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_description(ClearBookDescriptionCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn change_book_identifier(
    state: RoutedState,
    Path(book_id): Path<String>,
    Json(request): Json<ChangeBookIdentifierRequest>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .change_book_identifier(ChangeBookIdentifierCommand {
            id: book_id,
            identifier: request.identifier,
        })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn clear_book_identifier(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<StatusCode, AppError> {
    state
        .book_service()
        .clear_book_identifier(ClearBookIdentifierCommand { id: book_id })
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn head_book_file(
    state: RoutedState,
    Path(book_id): Path<String>,
) -> Result<Response<Body>, AppError> {
    let descriptor = state
        .book_service()
        .get_book_file_meta(GetBookFileMetaCommand { id: book_id })
        .await?;
    build_book_file_response(
        StatusCode::OK,
        &descriptor,
        descriptor.size,
        None,
        Body::empty(),
    )
}

pub async fn download_book_file(
    state: RoutedState,
    Path(book_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response<Body>, AppError> {
    let mut range_headers = headers.get_all(RANGE).iter();
    let Some(first_range) = range_headers.next() else {
        let (descriptor, opened) = state
            .book_service()
            .open_book_file_range(OpenBookFileRangeCommand {
                id: book_id,
                offset: 0,
                length: None,
            })
            .await?;
        let body = Body::from_stream(ReaderStream::new(opened.reader));
        return build_book_file_response(
            StatusCode::OK,
            &descriptor,
            opened.length,
            None,
            body,
        );
    };

    let mut book_service = state.book_service();
    let descriptor = book_service
        .get_book_file_meta(GetBookFileMetaCommand {
            id: book_id.clone(),
        })
        .await?;

    let if_range_fallback = {
        let mut if_range_headers = headers.get_all(IF_RANGE).iter();
        match if_range_headers.next() {
            None => false,
            Some(first_if_range) => {
                if if_range_headers.next().is_some() {
                    true
                } else {
                    match first_if_range.to_str() {
                        Ok(raw) => {
                            let strong_etag = format!("\"{}\"", descriptor.hash.to_hex());
                            !if_range_matches_strong_etag(raw, &strong_etag)
                        }
                        Err(_) => true,
                    }
                }
            }
        }
    };

    if if_range_fallback {
        let (descriptor, opened) = book_service
            .open_book_file_range(OpenBookFileRangeCommand {
                id: book_id,
                offset: 0,
                length: None,
            })
            .await?;
        let body = Body::from_stream(ReaderStream::new(opened.reader));
        return build_book_file_response(
            StatusCode::OK,
            &descriptor,
            opened.length,
            None,
            body,
        );
    }

    if range_headers.next().is_some() {
        return Err(AppError::BadRequest {
            message: ErrorCode::BookFileInvalidRange.key(),
        });
    }
    let raw_range = first_range.to_str().map_err(|_| AppError::BadRequest {
        message: ErrorCode::BookFileInvalidRange.key(),
    })?;

    let resolved = match parse_single_byte_range(raw_range, descriptor.size) {
        Ok(range) => range,
        Err(RangeParseError::InvalidSyntax) => {
            return Err(AppError::BadRequest {
                message: ErrorCode::BookFileInvalidRange.key(),
            });
        }
        Err(RangeParseError::NotSatisfiable { total_size }) => {
            return Err(AppError::RangeNotSatisfiable { total_size });
        }
    };

    let (descriptor, opened) = book_service
        .open_book_file_range(OpenBookFileRangeCommand {
            id: book_id,
            offset: resolved.start(),
            length: Some(resolved.len()),
        })
        .await?;
    let content_range = format!(
        "bytes {}-{}/{}",
        resolved.start(),
        resolved.end_inclusive(),
        descriptor.size
    );
    let body = Body::from_stream(ReaderStream::new(opened.reader));
    build_book_file_response(
        StatusCode::PARTIAL_CONTENT,
        &descriptor,
        opened.length,
        Some(content_range),
        body,
    )
}

fn content_type_for_book(file_type: BookFileType) -> &'static str {
    match file_type {
        BookFileType::EPUB => "application/epub+zip",
        BookFileType::PDF => "application/pdf",
    }
}

fn build_book_file_response(
    status: StatusCode,
    descriptor: &BookFileDescriptor,
    content_length: u64,
    content_range: Option<String>,
    body: Body,
) -> Result<Response<Body>, AppError> {
    let hash_hex = descriptor.hash.to_hex().to_string();
    let etag = format!("\"{hash_hex}\"");
    let mut builder = Response::builder()
        .status(status)
        .header(ACCEPT_RANGES, "bytes")
        .header(ETAG, etag)
        .header(CACHE_CONTROL, "private, immutable")
        .header(CONTENT_TYPE, content_type_for_book(descriptor.file_type))
        .header(CONTENT_LENGTH, content_length.to_string())
        .header("x-book-hash", hash_hex)
        .header("x-book-type", descriptor.file_type.to_string());
    if let Some(range) = content_range {
        builder = builder.header(CONTENT_RANGE, range);
    }
    builder
        .body(body)
        .map_err(|error| AppError::Internal(anyhow::anyhow!(error)))
}
