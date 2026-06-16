use axum::http::{header::{CONTENT_DISPOSITION, CONTENT_TYPE},StatusCode};

use crate::support::{INVALID_BOOK_HASH, JsonHttpResponse, TestApp, UNKNOWN_HASH, body_bytes};

mod support;

#[tokio::test]
async fn upload_book_file() {
    let app = TestApp::new().await;
    assert_eq!(app.upload_sample_book_file(support::SampleFile::PDF).await.0.status_code, StatusCode::CREATED);
    assert_eq!(app.upload_sample_book_file(support::SampleFile::EPUB).await.0.status_code, StatusCode::CREATED);
}

#[tokio::test]
async fn upload_book_file_missing_hash_error() {
    let app = TestApp::new().await;
    app.upload_book_file(b"qwq", "").await.assert_book_file_upload_missing_hash_error();
}

#[tokio::test]
async fn upload_book_file_already_exists_error() {
    let app = TestApp::new().await;
    app.upload_sample_book_file(support::SampleFile::PDF).await;
    app.upload_sample_book_file(support::SampleFile::PDF).await.0.assert_book_file_already_exists_error();
}

#[tokio::test]
async fn download_book_file() {
    let app = TestApp::new().await;
    let hash = app.upload_sample_book_file(support::SampleFile::PDF).await.1.to_hex().to_string();
    let response = app.download_book_file(&hash).await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/octet-stream");
    assert_eq!(response.headers()["x-book-hash"], &hash);
    assert_eq!(response.headers()[CONTENT_DISPOSITION], format!("attachment; filename=\"{hash}.book\""));

    let bytes = body_bytes(response).await;
    assert_eq!(hash, blake3::hash(&bytes).to_hex().to_string());
}

#[tokio::test]
async fn get_book_file() {
    let app = TestApp::new().await;
    let hash = app.upload_sample_book_file(support::SampleFile::PDF).await.1.to_hex();
    assert_eq!(app.get_book_file(&hash).await.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn get_book_file_not_found_error() {
    let app = TestApp::new().await;
    JsonHttpResponse::from(app.get_book_file(UNKNOWN_HASH).await).await.assert_book_file_not_found_error();
}

#[tokio::test]
async fn get_book_file_invalid_hash_format_error() {
    let app = TestApp::new().await;
    app.upload_book_file(b"qwq", INVALID_BOOK_HASH).await.assert_book_file_invalid_hash_format_error();
}
