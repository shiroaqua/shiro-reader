mod support;

use std::time::Duration;

use axum::{
    body::{Body, Bytes, to_bytes},
    http::{Method, Request, StatusCode},
};
use futures_util::stream;
use serde_json::{Value, json};
use shiro_http_contract::{PROTOCOL_VERSION, SERVICE_NAME, ServerMetaResponse, paths};
use shiro_reader_server::{
    app::build_router_with_timeout,
    application::identity::{
        authentication::commands::LoginCommand,
        user::{commands::UserRegisterCommand, dto::GetUserOutput},
    },
    state::AppState,
};
use tower::ServiceExt;

use support::{HttpError, PASSWORD, SampleFile, TestApp};

#[tokio::test]
async fn v1_meta_probe_and_healthz_report_protocol_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = support::runtime::config(dir.path());
    let expected_max = cfg.server.max_upload_size as u64;
    let app = TestApp::at(dir.path()).await;

    // /api/v1/meta returns canonical protocol metadata
    let meta_resp = app.meta().await;
    meta_resp.assert_status(200);
    let meta: ServerMetaResponse = meta_resp.json_as();
    assert_eq!(meta.service, SERVICE_NAME);
    assert_eq!(meta.api_version, PROTOCOL_VERSION);
    assert_eq!(meta.max_upload_bytes, expected_max);
    assert!(meta.is_compatible());

    // /healthz remains available
    app.healthz().await.assert_status(200);
}

#[tokio::test]
async fn v1_auth_me_returns_authenticated_identity_and_rejects_anonymous_or_revoked() {
    let app = TestApp::new().await;

    // Anonymous request rejected with 401
    app.me_anonymous()
        .await
        .assert_error(HttpError::MissingBearer);

    let session = app.session("CloudReader").await;

    // Authenticated request returns canonical GetUserOutput
    let me_resp = app.me(&session).await;
    me_resp.assert_status(200);
    let me: GetUserOutput = me_resp.json_as();
    assert_eq!(me.id, session.user_id);
    assert_eq!(me.name, "CloudReader");
    assert!(!me.created_at.is_empty());
    assert!(!me.updated_at.is_empty());

    // Revoked session token is rejected with 401
    app.logout(&session).await.assert_empty(204);
    app.me(&session)
        .await
        .assert_error(HttpError::InvalidToken);
}

#[tokio::test]
async fn streaming_upload_is_exempt_from_standard_request_timeout_and_allows_multipart_overhead() {
    let dir = tempfile::tempdir().unwrap();
    support::runtime::prepare_libs(dir.path()).await;

    let sample_bytes = SampleFile::Epub.bytes().to_vec();
    let sample_hash = SampleFile::Epub.hash();

    // Set max_upload_size to EXACTLY the sample's net byte length so multipart boundary overhead
    // pushes the HTTP body above max_upload_size, verifying MULTIPART_BODY_OVERHEAD_BYTES works.
    let mut cfg = support::runtime::config(dir.path());
    cfg.server.max_upload_size = sample_bytes.len();
    let state = AppState::build(&cfg).await.unwrap();

    state
        .user_service
        .register(UserRegisterCommand {
            name: "SlowUploader".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();
    let login = state
        .auth_service
        .login(LoginCommand {
            username: "SlowUploader".to_owned(),
            password: PASSWORD.to_owned(),
        })
        .await
        .unwrap();

    // Configure an aggressive 80ms standard timeout; slow upload will stream over ~200ms.
    let router = build_router_with_timeout(state, Duration::from_millis(80));

    let boundary = "----shiro-slow-upload-boundary";
    let mut full_body = Vec::new();
    full_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{sample_hash}\r\n"
        )
        .as_bytes(),
    );
    full_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n",
            sample_bytes.len()
        )
        .as_bytes(),
    );
    full_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"book.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n"
        )
        .as_bytes(),
    );
    full_body.extend_from_slice(&sample_bytes);
    full_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    assert!(
        full_body.len() > cfg.server.max_upload_size,
        "multipart body must exceed net max_upload_size"
    );

    let chunk_size = (full_body.len() / 4).max(1);
    let chunks: Vec<Bytes> = full_body
        .chunks(chunk_size)
        .map(Bytes::copy_from_slice)
        .collect();
    let slow_stream = stream::unfold(chunks.into_iter(), |mut iter| async move {
        let chunk = iter.next()?;
        tokio::time::sleep(Duration::from_millis(55)).await;
        Some((Ok::<Bytes, std::io::Error>(chunk), iter))
    });

    let upload_resp = router
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
                .body(Body::from_stream(slow_stream))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(upload_resp.status(), StatusCode::CREATED);

    // Now test a net payload exceeding max_upload_size by 1 byte -> must return 413 payload_too_large
    let mut oversized = sample_bytes.clone();
    oversized.push(0x20);
    let oversized_hash = blake3::hash(&oversized).to_hex().to_string();
    let mut oversized_body = Vec::new();
    oversized_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"hash\"\r\n\r\n{oversized_hash}\r\n"
        )
        .as_bytes(),
    );
    oversized_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"size\"\r\n\r\n{}\r\n",
            oversized.len()
        )
        .as_bytes(),
    );
    oversized_body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"too_big.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n"
        )
        .as_bytes(),
    );
    oversized_body.extend_from_slice(&oversized);
    oversized_body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let oversized_resp = router
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(paths::BOOK_FILES)
                .header("authorization", format!("Bearer {}", login.access_token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(oversized_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(oversized_resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let err_bytes = to_bytes(oversized_resp.into_body(), 64 * 1024)
        .await
        .unwrap();
    let err_json: Value = serde_json::from_slice(&err_bytes).unwrap();
    assert_eq!(
        err_json,
        json!({"error": {"code": 9001, "message": "common.payload_too_large"}})
    );
}
