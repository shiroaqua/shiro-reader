mod support;
use support::*;
use HttpError::*;
use SampleFile::*;

#[tokio::test]
async fn upload_lookup_download_and_deduplication() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for file in [Pdf, Epub] {
        app.upload(&reader, file).await;
        app.find_file(&reader, &file.hash()).await.assert_empty(204);
        app.download(&reader, &file.hash()).await.assert_file(&file);
        app.upload_again(&reader, file).await.assert_error(DuplicateFile);
    }
}

#[tokio::test]
async fn upload_rejects_missing_and_duplicate_fields() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload_parts(Some(&reader), &[], Some(b"file")).await.assert_error(MissingHash);
    app.upload_parts(Some(&reader), &[UNKNOWN_HASH], None).await.assert_error(MissingFile);
    app.upload_parts(Some(&reader), &[UNKNOWN_HASH, UNKNOWN_HASH], Some(b"file")).await.assert_error(DuplicateHash);
    app.upload_parts(Some(&reader), &[INVALID_HASH], Some(b"file")).await.assert_error(InvalidHash);
}

#[tokio::test]
async fn failed_upload_allows_valid_retry() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload_parts(Some(&reader), &[&Epub.hash()], Some(b"wrong bytes")).await.assert_error(HashMismatch);
    app.find_file(&reader, &Epub.hash()).await.assert_error(FileNotFound);
    app.upload(&reader, Epub).await;
    app.download(&reader, &Epub.hash()).await.assert_file(&Epub);
}

#[tokio::test]
async fn unsupported_file_is_rejected() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let file = UnsupportedSample::new();
    app.upload_parts(Some(&reader), &[&file.hash], Some(&file.bytes)).await.assert_error(UnsupportedFile);
}

#[tokio::test]
async fn lookup_and_download_reject_invalid_and_missing_hashes() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (hash, error) in [(INVALID_HASH, InvalidHash), (UNKNOWN_HASH, FileNotFound)] {
        app.find_file(&reader, hash).await.assert_error(error);
        app.download(&reader, hash).await.assert_error(error);
    }
}
