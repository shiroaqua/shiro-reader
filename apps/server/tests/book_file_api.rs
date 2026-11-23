mod support;
use HttpError::*;
use SampleFile::*;
use support::*;

// 验证 PDF 和 EPUB 文件可上传、查询和原样下载，且重复上传被拒绝。
#[tokio::test]
async fn upload_lookup_download_and_deduplication() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for file in [Pdf, Epub] {
        app.upload(&reader, file).await;
        app.find_file(&reader, &file.hash()).await.assert_empty(204);
        app.download(&reader, &file.hash()).await.assert_file(&file);
        app.upload_again(&reader, file)
            .await
            .assert_error(DuplicateFile);
    }
}

// 验证上传时拒绝缺失哈希、缺失文件、重复哈希字段和非法哈希。
#[tokio::test]
async fn upload_rejects_missing_and_duplicate_fields() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload_parts(Some(&reader), &[], Some(b"file"))
        .await
        .assert_error(MissingHash);
    app.upload_parts(Some(&reader), &[UNKNOWN_HASH], None)
        .await
        .assert_error(MissingFile);
    app.upload_parts(Some(&reader), &[UNKNOWN_HASH, UNKNOWN_HASH], Some(b"file"))
        .await
        .assert_error(DuplicateHash);
    app.upload_parts(Some(&reader), &[INVALID_HASH], Some(b"file"))
        .await
        .assert_error(InvalidHash);
}

// 验证哈希不匹配的上传失败后不留下可查询文件，且可重新上传正确内容。
#[tokio::test]
async fn failed_upload_allows_valid_retry() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload_parts(Some(&reader), &[&Epub.hash()], Some(b"wrong bytes"))
        .await
        .assert_error(HashMismatch);
    app.find_file(&reader, &Epub.hash())
        .await
        .assert_error(FileNotFound);
    app.upload(&reader, Epub).await;
    app.download(&reader, &Epub.hash()).await.assert_file(&Epub);
}

// 验证上传不支持的文件格式时返回对应错误。
#[tokio::test]
async fn unsupported_file_is_rejected() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let file = UnsupportedSample::new();
    app.upload_parts(Some(&reader), &[&file.hash], Some(&file.bytes))
        .await
        .assert_error(UnsupportedFile);
}

// 验证查询和下载文件时拒绝非法哈希或不存在的文件。
#[tokio::test]
async fn lookup_and_download_reject_invalid_and_missing_hashes() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (hash, error) in [(INVALID_HASH, InvalidHash), (UNKNOWN_HASH, FileNotFound)] {
        app.find_file(&reader, hash).await.assert_error(error);
        app.download(&reader, hash).await.assert_error(error);
    }
}

// 重启扫描时必须同时校验文件大小和内容哈希，损坏的文件不可查询。
#[tokio::test]
async fn restart_rejects_book_files_with_mismatched_size_or_hash() {
    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    drop(app);

    let path = directory
        .path()
        .join("books")
        .join(format!("{}.book", Epub.hash()));
    let original = std::fs::read(&path).unwrap();
    assert_eq!(original[0], 1);
    assert_eq!(
        u64::from_be_bytes(original[34..42].try_into().unwrap()),
        Epub.bytes().len() as u64
    );

    for corrupt_byte in [41, 42] {
        let mut stored = original.clone();
        stored[corrupt_byte] ^= 1;
        std::fs::write(&path, stored).unwrap();

        let app = TestApp::at(directory.path()).await;
        app.find_file(&reader, &Epub.hash())
            .await
            .assert_error(FileNotFound);
        app.download(&reader, &Epub.hash())
            .await
            .assert_error(FileNotFound);
    }
}

// 验证 file 先于 hash 出现、file 缺少 filename、以及探针请求缺少 hash 查询参数均被拒绝。
#[tokio::test]
async fn upload_rejects_misordered_or_missing_filename_and_probe_requires_hash_query() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let epub_bytes = Epub.bytes();
    let epub_hash = Epub.hash();

    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("file", Some("book.book"), &epub_bytes),
            ("hash", None, epub_hash.as_bytes()),
        ],
    )
    .await
    .assert_error(MissingHash);

    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("file", None, &epub_bytes),
        ],
    )
    .await
    .assert_error(MissingFile);

    app.find_file_raw(&reader, "").await.assert_status(400);
}
