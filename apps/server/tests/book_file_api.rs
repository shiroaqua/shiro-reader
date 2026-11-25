mod support;
use HttpError::*;
use SampleFile::*;
use support::*;

// 验证 PDF 和 EPUB 文件可上传、查询、创建书籍后经 HEAD/GET 原样下载，且重复上传被拒绝。
#[tokio::test]
async fn upload_lookup_download_and_deduplication() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (idx, file) in [Pdf, Epub].into_iter().enumerate() {
        app.upload(&reader, file).await;
        app.find_file(&reader, &file.hash()).await.assert_empty(204);
        let book = app.add_book(&reader, &format!("Book {idx}"), file).await;
        app.head_book_file(&reader, &book.data.id)
            .await
            .assert_empty(200);
        app.download(&reader, &book.data.id).await.assert_file(&file);
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
    let book = app.add_book(&reader, "Retried", Epub).await;
    app.download(&reader, &book.data.id).await.assert_file(&Epub);
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

// 验证查询和下载文件时拒绝非法参数或不存在的资源。
#[tokio::test]
async fn lookup_and_download_reject_invalid_and_missing_ids() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    for (hash, error) in [(INVALID_HASH, InvalidHash), (UNKNOWN_HASH, FileNotFound)] {
        app.find_file(&reader, hash).await.assert_error(error);
    }
    for (book_id, error) in [(INVALID_ID, InvalidBookId), (UNKNOWN_ID, BookNotFound)] {
        app.download(&reader, book_id).await.assert_error(error);
        app.head_book_file(&reader, book_id)
            .await
            .assert_status(if book_id == INVALID_ID { 400 } else { 404 });
    }
}

// 重启扫描时必须同时校验文件大小和内容哈希，损坏的文件不可查询或下载。
#[tokio::test]
async fn restart_rejects_book_files_with_mismatched_size_or_hash() {
    let directory = tempfile::tempdir().unwrap();
    let app = TestApp::at(directory.path()).await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "RestartCheck", Epub).await.data;
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
        app.download(&reader, &book.id)
            .await
            .assert_error(FileNotFound);
        app.head_book_file(&reader, &book.id)
            .await
            .assert_status(404);
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

// 验证缺失、重复、非法、不匹配和超限 size 字段均按契约返回 400 / 413。
#[tokio::test]
async fn upload_rejects_missing_duplicate_invalid_mismatched_and_oversized_size() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    let epub_bytes = Epub.bytes();
    let epub_hash = Epub.hash();
    let size_str = epub_bytes.len().to_string();

    // Missing size before file
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(MissingSize);

    // Size appearing after file
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
            ("size", None, size_str.as_bytes()),
        ],
    )
    .await
    .assert_error(MissingSize);

    // Duplicate size
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("size", None, size_str.as_bytes()),
            ("size", None, size_str.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(DuplicateSize);

    // Invalid size strings
    for bad in ["", "-1", "abc", "+10", " 10"] {
        app.upload_raw_multipart(
            Some(&reader),
            &[
                ("hash", None, epub_hash.as_bytes()),
                ("size", None, bad.as_bytes()),
                ("file", Some("book.epub"), &epub_bytes),
            ],
        )
        .await
        .assert_error(InvalidSize);
    }

    // Declared size larger than actual bytes -> 400 SizeMismatch
    let too_large_declared = (epub_bytes.len() + 10).to_string();
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("size", None, too_large_declared.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(SizeMismatch);

    // Declared size smaller than actual bytes -> 400 SizeMismatch
    let too_small_declared = (epub_bytes.len() - 1).to_string();
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("size", None, too_small_declared.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(SizeMismatch);

    // Oversized text fields are rejected without buffering large payloads in memory.
    let oversized_hash = vec![b'a'; 1024];
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, &oversized_hash),
            ("size", None, size_str.as_bytes()),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(InvalidHash);

    let oversized_size = vec![b'1'; 1024];
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("size", None, &oversized_size),
            ("file", Some("book.epub"), &epub_bytes),
        ],
    )
    .await
    .assert_error(InvalidSize);

    // Declared size exceeding server max_upload_size (8 MiB in test config) -> 413 PayloadTooLarge immediately
    let huge_declared = (9 * 1024 * 1024usize).to_string();
    app.upload_raw_multipart(
        Some(&reader),
        &[
            ("hash", None, epub_hash.as_bytes()),
            ("size", None, huge_declared.as_bytes()),
            ("file", Some("book.epub"), b"tiny"),
        ],
    )
    .await
    .assert_error(PayloadTooLarge);
}

// 验证多用户并发上传同一 Hash 时，探针 GET 与二次上传 POST 均返回 409，完成后探针返回 204。
#[tokio::test]
async fn concurrent_same_hash_probe_and_upload_return_409_until_completion() {
    use axum::{
        body::{Body, Bytes, to_bytes},
        http::{Method, Request, StatusCode},
    };
    use futures_util::stream;
    use serde_json::{Value, json};
    use shiro_application::identity::{
        authentication::commands::LoginCommand, user::commands::UserRegisterCommand,
    };
    use shiro_http_contract::paths;
    use shiro_reader_server::{app::build_router, state::AppState};
    use std::time::Duration;
    use tokio::sync::oneshot;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().unwrap();
    support::runtime::prepare_libs(dir.path()).await;
    let cfg = support::runtime::config(dir.path());
    let state = AppState::build(&cfg).await.unwrap();

    for name in ["Alice", "Bob"] {
        state
            .user_service
            .register(UserRegisterCommand {
                name: name.to_owned(),
                password: PASSWORD.to_owned(),
            })
            .await
            .unwrap();
    }
    let alice = state
        .auth_service
        .login(LoginCommand {
            username: "Alice".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();
    let bob = state
        .auth_service
        .login(LoginCommand {
            username: "Bob".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();

    let router = build_router(state);
    let sample_bytes = Epub.bytes();
    let sample_hash = Epub.hash();
    let boundary = "----shiro-concurrent-boundary";

    let mut head = Vec::new();
    head.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{sample_hash}\r\n"
        )
        .as_bytes(),
    );
    head.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n",
            sample_bytes.len()
        )
        .as_bytes(),
    );
    head.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"book.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n"
        )
        .as_bytes(),
    );
    head.extend_from_slice(&sample_bytes[..16]);

    let mut tail = Vec::new();
    tail.extend_from_slice(&sample_bytes[16..]);
    tail.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let (release_tx, release_rx) = oneshot::channel::<()>();
    let paused_stream = stream::unfold(
        (Some(Bytes::from(head)), Some(release_rx), Some(Bytes::from(tail))),
        |(head, rx, tail)| async move {
            if let Some(first) = head {
                return Some((Ok::<Bytes, std::io::Error>(first), (None, rx, tail)));
            }
            if let Some(rx) = rx {
                let _ = rx.await;
                if let Some(rest) = tail {
                    return Some((Ok::<Bytes, std::io::Error>(rest), (None, None, None)));
                }
            }
            None
        },
    );

    let router_alice = router.clone();
    let alice_token = alice.access_token.clone();
    let alice_task = tokio::spawn(async move {
        router_alice
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri(paths::BOOK_FILES)
                    .header("authorization", format!("Bearer {alice_token}"))
                    .header(
                        "content-type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .body(Body::from_stream(paused_stream))
                    .unwrap(),
            )
            .await
            .unwrap()
    });

    // Wait until Alice holds the in-flight upload lease (Bob's GET probe flips from 404 to 409).
    let probe_uri = format!("{}?hash={sample_hash}", paths::BOOK_FILES);
    let mut saw_conflict = false;
    for _ in 0..50 {
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri(&probe_uri)
                    .header("authorization", format!("Bearer {}", bob.access_token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        if resp.status() == StatusCode::CONFLICT {
            let body = to_bytes(resp.into_body(), 64 * 1024).await.unwrap();
            let parsed: Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(
                parsed,
                json!({"error": {"code": 2101, "message": "library.book.file.upload.conflict"}})
            );
            saw_conflict = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(saw_conflict, "expected Bob's probe to see 409 UploadConflict");

    // Bob's concurrent POST upload of the same hash must also return 409 UploadConflict.
    let mut bob_body = Vec::new();
    bob_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{sample_hash}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"book.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n",
            sample_bytes.len()
        )
        .as_bytes(),
    );
    bob_body.extend_from_slice(&sample_bytes);
    bob_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let bob_post = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(paths::BOOK_FILES)
                .header("authorization", format!("Bearer {}", bob.access_token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(bob_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bob_post.status(), StatusCode::CONFLICT);
    let bob_post_bytes = to_bytes(bob_post.into_body(), 64 * 1024).await.unwrap();
    let bob_post_json: Value = serde_json::from_slice(&bob_post_bytes).unwrap();
    assert_eq!(
        bob_post_json,
        json!({"error": {"code": 2101, "message": "library.book.file.upload.conflict"}})
    );

    // Release Alice -> completes with 201, and Bob's probe now succeeds with 204.
    release_tx.send(()).unwrap();
    let alice_resp = alice_task.await.unwrap();
    assert_eq!(alice_resp.status(), StatusCode::CREATED);

    let probe_after = router
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(&probe_uri)
                .header("authorization", format!("Bearer {}", bob.access_token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(probe_after.status(), StatusCode::NO_CONTENT);
}

// 验证慢速流上传和慢速 multipart 头部均返回 408 超时并释放冲突锁。
#[tokio::test]
async fn slow_stream_and_slow_header_return_408_and_unlock() {
    use axum::{
        body::{Body, Bytes, to_bytes},
        http::{Method, Request, StatusCode},
    };
    use futures_util::stream;
    use serde_json::{Value, json};
    use shiro_application::identity::{
        authentication::commands::LoginCommand, user::commands::UserRegisterCommand,
    };
    use shiro_http_contract::paths;
    use shiro_reader_server::{app::build_router_with_timeout, state::AppState};
    use std::time::Duration;
    use tower::ServiceExt;

    let dir = tempfile::tempdir().unwrap();
    support::runtime::prepare_libs(dir.path()).await;
    let mut cfg = support::runtime::config(dir.path());
    cfg.server.upload_base_timeout_secs = 0;
    cfg.server.upload_min_bytes_per_sec = 500_000;
    let state = AppState::build(&cfg).await.unwrap();

    state
        .user_service
        .register(UserRegisterCommand {
            name: "SlowUser".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();
    let login = state
        .auth_service
        .login(LoginCommand {
            username: "SlowUser".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();

    let router = build_router_with_timeout(state, Duration::from_millis(80));
    let sample_bytes = Epub.bytes();
    let sample_hash = Epub.hash();
    let boundary = "----shiro-timeout-boundary";

    // Case 1: Slow pre-file multipart header -> 408 RequestTimeout
    let stalled_header_stream = stream::unfold(true, move |first| async move {
        if first {
            Some((
                Ok::<Bytes, std::io::Error>(Bytes::from(format!(
                    "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n"
                ))),
                false,
            ))
        } else {
            futures_util::future::pending::<()>().await;
            None
        }
    });
    let header_timeout_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(paths::BOOK_FILES)
                .header("authorization", format!("Bearer {}", login.access_token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from_stream(stalled_header_stream))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(header_timeout_resp.status(), StatusCode::REQUEST_TIMEOUT);
    let header_json: Value = serde_json::from_slice(
        &to_bytes(header_timeout_resp.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        header_json,
        json!({"error": {"code": 2107, "message": "library.book.file.upload.timeout"}})
    );

    // Case 2: Slow file body stream -> 408 RequestTimeout and unlocks lease for retry
    let mut head = Vec::new();
    head.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{sample_hash}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"book.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n",
            sample_bytes.len()
        )
        .as_bytes(),
    );
    head.extend_from_slice(&sample_bytes[..16]);

    let stalled_body_stream = stream::unfold(Some(Bytes::from(head)), |state| async move {
        if let Some(chunk) = state {
            Some((Ok::<Bytes, std::io::Error>(chunk), None))
        } else {
            futures_util::future::pending::<()>().await;
            None
        }
    });
    let body_timeout_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(paths::BOOK_FILES)
                .header("authorization", format!("Bearer {}", login.access_token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from_stream(stalled_body_stream))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(body_timeout_resp.status(), StatusCode::REQUEST_TIMEOUT);
    let body_json: Value = serde_json::from_slice(
        &to_bytes(body_timeout_resp.into_body(), 64 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        body_json,
        json!({"error": {"code": 2107, "message": "library.book.file.upload.timeout"}})
    );

    // Retry upload of the exact same file now succeeds (lease was released).
    let mut retry_body = Vec::new();
    retry_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{sample_hash}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"book.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n",
            sample_bytes.len()
        )
        .as_bytes(),
    );
    retry_body.extend_from_slice(&sample_bytes);
    retry_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let retry_resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(paths::BOOK_FILES)
                .header("authorization", format!("Bearer {}", login.access_token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(retry_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(retry_resp.status(), StatusCode::CREATED);
}

// 验证 /bookshelf/books/{book_id}/file 支持 HEAD、Range、If-Range、416/400 错误及回收站拦截，并确认旧 hash 下载端点已下线。
#[tokio::test]
async fn book_file_supports_head_range_if_range_errors_trash_guard_and_drops_legacy_endpoint() {
    let app = TestApp::new().await;
    let reader = app.session("Reader").await;
    app.upload(&reader, Epub).await;
    let book = app.add_book(&reader, "RangeEpub", Epub).await;
    let full_bytes = Epub.bytes();
    let total = full_bytes.len();
    let hash = Epub.hash();
    let strong_etag = format!("\"{hash}\"");

    // 1. Legacy endpoint GET /api/v1/library/files/book/{hash} is no longer registered.
    let legacy = app
        .raw_get(&reader, &format!("/api/v1/library/files/book/{hash}"))
        .await;
    assert!(
        legacy.header("x-book-hash").is_none(),
        "legacy hash download endpoint must be removed"
    );
    let legacy_probe = app
        .raw_get(
            &reader,
            &format!("{}?hash={hash}", shiro_http_contract::paths::BOOK_FILES),
        )
        .await;
    legacy_probe.assert_empty(204);
    assert!(
        !matches!(
            app.raw_get(&reader, &format!("/api/v1/library/files/book/{hash}"))
                .await
                .header("content-type"),
            Some("application/octet-stream") | Some("application/epub+zip")
        )
    );

    // 2. HEAD returns 200 with exact metadata and empty body.
    let head_resp = app.head_book_file(&reader, &book.data.id).await;
    head_resp.assert_empty(200);
    assert_eq!(head_resp.header("accept-ranges"), Some("bytes"));
    assert_eq!(head_resp.header("etag"), Some(strong_etag.as_str()));
    assert_eq!(
        head_resp.header("cache-control"),
        Some("private, immutable")
    );
    assert_eq!(
        head_resp.header("content-type"),
        Some("application/epub+zip")
    );
    assert_eq!(
        head_resp.header("content-length"),
        Some(total.to_string().as_str())
    );
    assert_eq!(head_resp.header("x-book-hash"), Some(hash.as_str()));
    assert_eq!(head_resp.header("x-book-type"), Some("EPUB"));

    // 3. Range 206 slices (closed, open-ended, suffix, clamped above size, clamped above u64::MAX).
    for (range_hdr, start, end_inclusive) in [
        ("bytes=0-99", 0usize, 99usize),
        ("bytes=100-", 100usize, total - 1),
        ("bytes=-64", total - 64, total - 1),
        ("bytes=0-99999999", 0, total - 1),
        ("bytes=0-999999999999999999999999", 0, total - 1),
    ] {
        let resp = app
            .download_with_headers(&reader, &book.data.id, &[("range", range_hdr)])
            .await;
        resp.assert_status(206);
        let expected_len = end_inclusive - start + 1;
        assert_eq!(resp.header("accept-ranges"), Some("bytes"));
        assert_eq!(resp.header("etag"), Some(strong_etag.as_str()));
        assert_eq!(resp.header("x-book-hash"), Some(hash.as_str()));
        assert_eq!(resp.header("x-book-type"), Some("EPUB"));
        assert_eq!(
            resp.header("content-range"),
            Some(format!("bytes {start}-{end_inclusive}/{total}").as_str())
        );
        assert_eq!(
            resp.header("content-length"),
            Some(expected_len.to_string().as_str())
        );
        assert_eq!(resp.bytes(), &full_bytes[start..=end_inclusive]);
    }

    // 4. If-Range strong ETag match returns 206; weak ETag, HTTP-date, or mismatch falls back to 200.
    let matched_if_range = app
        .download_with_headers(
            &reader,
            &book.data.id,
            &[("range", "bytes=0-99"), ("if-range", &strong_etag)],
        )
        .await;
    matched_if_range.assert_status(206);
    assert_eq!(matched_if_range.bytes(), &full_bytes[..100]);

    let weak_etag = format!("W/{strong_etag}");
    for if_range in [
        weak_etag.as_str(),
        "Wed, 21 Oct 2015 07:28:00 GMT",
        "\"0000000000000000000000000000000000000000000000000000000000000000\"",
    ] {
        let fallback = app
            .download_with_headers(
                &reader,
                &book.data.id,
                &[("range", "bytes=0-99"), ("if-range", if_range)],
            )
            .await;
        fallback.assert_file(&Epub);
    }

    // 5. 416 Range Not Satisfiable includes Content-Range: bytes */<total>.
    let total_str = total.to_string();
    let oob_closed = format!("bytes={total}-{}", total + 10);
    let oob_open = format!("bytes={total}-");
    for range_hdr in [
        oob_closed.as_str(),
        oob_open.as_str(),
        "bytes=999999999999999999999999-",
        "bytes=-0",
    ] {
        let resp = app
            .download_with_headers(&reader, &book.data.id, &[("range", range_hdr)])
            .await;
        resp.assert_error(RangeNotSatisfiable);
        assert_eq!(
            resp.header("content-range"),
            Some(format!("bytes */{total_str}").as_str())
        );
    }

    // 6. 400 InvalidRange on malformed/multi-range/reversed ranges.
    for bad_range in [
        "bytes=50-10",
        "bytes=-",
        "bytes=0-1, 2-3",
        "0-10",
        "bytes=0-abc",
        "bytes=9999999999999999999999999-9999999999999999999999998",
    ] {
        app.download_with_headers(&reader, &book.data.id, &[("range", bad_range)])
            .await
            .assert_error(InvalidRange);
    }

    // 7. Trashed books reject HEAD and GET (with or without Range) with 409 BookAlreadyDeleted.
    book.delete().await.assert_empty(204);
    app.head_book_file(&reader, &book.data.id)
        .await
        .assert_status(409);
    app.download(&reader, &book.data.id)
        .await
        .assert_error(BookAlreadyDeleted);
    app.download_with_headers(&reader, &book.data.id, &[("range", "bytes=0-9")])
        .await
        .assert_error(BookAlreadyDeleted);
}
