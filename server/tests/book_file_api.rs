mod support;

use axum::http::{
    header::{CONTENT_DISPOSITION, CONTENT_TYPE},
    StatusCode,
};
use support::{
    assert_book_file_already_exists_error, assert_book_file_hash_mismatch_error,
    assert_book_file_invalid_hash_format_error, assert_book_file_not_found_error,
    assert_book_file_upload_duplicate_hash_error, assert_book_file_upload_missing_file_error,
    assert_book_file_upload_missing_hash_error, body_bytes, hash_for, TestApp,
    HASH_MISMATCH_BOOK_BYTES, INVALID_BOOK_HASH, MISSING_SAMPLE_BOOK_BYTES, SAMPLE_BOOK_BYTES,
};

#[tokio::test]
async fn upload_book_file() {
    let app = TestApp::new().await;
    let bytes = SAMPLE_BOOK_BYTES;
    let hash = hash_for(bytes);

    let response = app.upload_book_file(&hash, bytes, None).await;

    assert_eq!(response.status(), StatusCode::CREATED);
}

#[tokio::test]
async fn get_book_file() {
    let app = TestApp::new().await;
    let bytes = SAMPLE_BOOK_BYTES;
    let hash = hash_for(bytes);
    assert_eq!(
        app.upload_book_file(&hash, bytes, None).await.status(),
        StatusCode::CREATED
    );

    let response = app
        .get(&format!("/api/v1/library/books/files?hash={hash}"))
        .await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn download_book_file() {
    let app = TestApp::new().await;
    let bytes = SAMPLE_BOOK_BYTES;
    let hash = hash_for(bytes);
    assert_eq!(
        app.upload_book_file(&hash, bytes, None).await.status(),
        StatusCode::CREATED
    );

    let response = app
        .get(&format!("/api/v1/library/books/files/{hash}"))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/octet-stream");
    assert_eq!(response.headers()["x-book-hash"], hash);
    assert_eq!(
        response.headers()[CONTENT_DISPOSITION],
        format!("attachment; filename=\"{hash}.book\"")
    );
    assert_eq!(body_bytes(response).await, bytes);
}

#[tokio::test]
async fn upload_book_file_rejects_missing_hash() {
    let app = TestApp::new().await;

    assert_book_file_upload_missing_hash_error(
        app.upload_book_file_without_hash(SAMPLE_BOOK_BYTES).await,
    )
    .await;
}

#[tokio::test]
async fn upload_book_file_rejects_missing_file() {
    let app = TestApp::new().await;
    let hash = hash_for(SAMPLE_BOOK_BYTES);

    assert_book_file_upload_missing_file_error(app.upload_book_file_without_file(&hash).await)
        .await;
}

#[tokio::test]
async fn upload_book_file_rejects_duplicate_hash_field() {
    let app = TestApp::new().await;
    let bytes = SAMPLE_BOOK_BYTES;
    let hash = hash_for(bytes);

    assert_book_file_upload_duplicate_hash_error(
        app.upload_book_file(&hash, bytes, Some(&hash)).await,
    )
    .await;
}

#[tokio::test]
async fn upload_book_file_rejects_hash_mismatch() {
    let app = TestApp::new().await;

    assert_book_file_hash_mismatch_error(
        app.upload_book_file(&hash_for(HASH_MISMATCH_BOOK_BYTES), SAMPLE_BOOK_BYTES, None)
            .await,
    )
    .await;
}

#[tokio::test]
async fn upload_book_file_rejects_existing_file() {
    let app = TestApp::new().await;
    let bytes = SAMPLE_BOOK_BYTES;
    let hash = hash_for(bytes);
    assert_eq!(
        app.upload_book_file(&hash, bytes, None).await.status(),
        StatusCode::CREATED
    );

    assert_book_file_already_exists_error(app.upload_book_file(&hash, bytes, None).await)
        .await;
}

#[tokio::test]
async fn get_book_file_rejects_invalid_hash() {
    let app = TestApp::new().await;

    assert_book_file_invalid_hash_format_error(
        app.get(&format!(
            "/api/v1/library/books/files?hash={INVALID_BOOK_HASH}"
        ))
        .await,
    )
    .await;
}

#[tokio::test]
async fn get_book_file_returns_not_found_for_missing_file() {
    let app = TestApp::new().await;

    assert_book_file_not_found_error(
        app.get(&format!(
            "/api/v1/library/books/files?hash={}",
            hash_for(MISSING_SAMPLE_BOOK_BYTES)
        ))
        .await,
    )
    .await;
}
