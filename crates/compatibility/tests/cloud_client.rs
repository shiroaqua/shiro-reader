use std::{
    fs,
    io::Cursor,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Json, Router,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Redirect,
    routing::get,
};
use futures_util::TryStreamExt;
use shiro_compatibility::{
    CloudClient, CloudProbeOutcome, CredentialStore, DEFAULT_CLOUD_MAX_UPLOAD_BYTES,
    DeleteTagStrategyDto, EncryptedFileCredentialStore, SessionVerifyOutcome,
    normalize_cloud_origin,
};
use shiro_error_catalog::ErrorCode;
use shiro_http_contract::{ServerMetaResponse, paths};
use shiro_reader_server::{
    app::build_router,
    application::identity::user::commands::UserRegisterCommand,
    config::{Config, CryptoConfig, LogFormat, ServerConfig as AppServerConfig, TelemetryConfig},
    state::AppState,
};
use tokio::{
    net::TcpListener,
    sync::{OnceCell, oneshot},
};

struct RunningHttpServer {
    addr: SocketAddr,
    origin: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl RunningHttpServer {
    async fn start(router: Router) -> Self {
        Self::start_on_addr("127.0.0.1:0".parse().unwrap(), router).await
    }

    async fn start_on_addr(bind_addr: SocketAddr, router: Router) -> Self {
        let listener = TcpListener::bind(bind_addr)
            .await
            .expect("bind tcp listener");
        let addr = listener.local_addr().expect("local addr");
        let origin = format!("http://127.0.0.1:{}", addr.port());

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });

        Self {
            addr,
            origin,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    async fn stop(mut self) -> SocketAddr {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
        self.addr
    }
}

impl Drop for RunningHttpServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

static SHARED_LIBS_DIR: OnceCell<PathBuf> = OnceCell::const_new();

fn shared_root_directory() -> PathBuf {
    if let Ok(value) = std::env::var("ROOT_DIRECTORY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let configured = PathBuf::from(trimmed);
            if configured.is_absolute() {
                return configured;
            }
            let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")));
            return workspace_root.join(configured);
        }
    }

    std::env::temp_dir().join("ShiroReader")
}

#[cfg(unix)]
fn link_libs_dir(shared_libs: &Path, target_libs: &Path) -> std::io::Result<()> {
    if shared_libs == target_libs {
        return Ok(());
    }
    std::os::unix::fs::symlink(shared_libs, target_libs)
}

#[cfg(not(unix))]
fn link_libs_dir(shared_libs: &Path, target_libs: &Path) -> std::io::Result<()> {
    if shared_libs == target_libs {
        return Ok(());
    }
    fs::create_dir_all(target_libs)?;
    for entry in fs::read_dir(shared_libs)? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            let from = entry.path();
            let to = target_libs.join(entry.file_name());
            if fs::hard_link(&from, &to).is_err() {
                fs::copy(&from, &to)?;
            }
        }
    }
    Ok(())
}

async fn build_real_v1_server_state(root: &Path) -> AppState {
    let shared_libs = SHARED_LIBS_DIR
        .get_or_init(|| async {
            let shared_root = shared_root_directory();
            shiro_reader_server::infrastructure::dependencies::ensure_runtime_dependencies(
                &shared_root,
            )
            .await
            .expect("ensure test runtime dependencies");
            let libs =
                shiro_reader_server::infrastructure::dependencies::libs_dir(&shared_root);
            std::path::absolute(&libs).unwrap_or(libs)
        })
        .await;

    let target_libs = shiro_reader_server::infrastructure::dependencies::libs_dir(root);
    if fs::symlink_metadata(&target_libs).is_err() {
        fs::create_dir_all(root).unwrap();
        link_libs_dir(shared_libs, &target_libs).expect("prepare test libs directory");
    }

    let config = Config {
        root_directory: root.to_path_buf(),
        server: AppServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            max_upload_size: 16 * 1024 * 1024,
            upload_base_timeout_secs: 30,
            upload_min_bytes_per_sec: 64 * 1024,
        },
        telemetry: TelemetryConfig {
            log_format: LogFormat::Pretty,
            log_level: shiro_reader_server::config::default_log_level().to_owned(),
        },
        crypto: CryptoConfig {
            argon2_memory_cost: 1024,
            argon2_time_cost: 1,
            argon2_parallelism: 1,
            argon2_output_length: 32,
            jwt_secret: "test-cloud-client-jwt-secret-key-0123456789".into(),
            jwt_issuer: "shiro-test".into(),
            jwt_audience: "shiro-client".into(),
            jwt_access_ttl_secs: 3600,
            jwt_refresh_ttl_secs: 86400,
            jwt_validation_leeway_secs: 0,
        },
    };
    AppState::build(&config)
        .await
        .expect("build server AppState")
}

fn new_client(
    origin: &str,
    client_root: &Path,
    on_invalidated: Option<Arc<dyn Fn() + Send + Sync>>,
) -> (CloudClient, Arc<EncryptedFileCredentialStore>) {
    let cred_store = Arc::new(EncryptedFileCredentialStore::new(
        client_root.join("keys"),
        client_root.join("credentials"),
    ));
    let client = CloudClient::new(
        origin.to_owned(),
        client_root.join("temp"),
        cred_store.clone(),
        on_invalidated,
    )
    .expect("construct CloudClient");
    (client, cred_store)
}

#[test]
fn normalize_cloud_origin_accepts_http_and_https_and_rejects_invalid_values() {
    assert_eq!(
        normalize_cloud_origin("http://127.0.0.1:8080/").unwrap(),
        "http://127.0.0.1:8080"
    );
    assert_eq!(
        normalize_cloud_origin("HTTPS://Books.Example.COM/").unwrap(),
        "https://books.example.com"
    );
    assert_eq!(
        normalize_cloud_origin("HTTP://[::1]:3000").unwrap(),
        "http://[::1]:3000"
    );
    assert!(normalize_cloud_origin("").is_err());
    assert!(normalize_cloud_origin("ftp://books.example.com").is_err());
    assert!(normalize_cloud_origin("http://books.example.com/api").is_err());
    assert!(normalize_cloud_origin("books.example.com").is_err());
    assert!(normalize_cloud_origin("http://[not-ipv6]").is_err());
    assert!(normalize_cloud_origin("http://bad%host").is_err());
    assert!(normalize_cloud_origin("https://user@books.example.com").is_err());
    assert!(normalize_cloud_origin("http://books.example.com:0").is_err());
}

#[tokio::test]
async fn protocol_probe_rejects_missing_meta_and_incompatible_servers() {
    // Simulated server without /api/v1/meta
    let missing_meta_router =
        Router::new().route("/api/v1/auth/login", get(|| async { StatusCode::OK }));
    let missing_meta_server = RunningHttpServer::start(missing_meta_router).await;

    // Simulated incompatible service / protocol version server
    let bad_meta_router = Router::new().route(
        paths::META,
        get(|| async {
            Json(ServerMetaResponse {
                service: "other-service".to_owned(),
                api_version: 99,
                server_version: "9.9.9".to_owned(),
                max_upload_bytes: 1024,
            })
        }),
    );
    let bad_server = RunningHttpServer::start(bad_meta_router).await;

    let client_root = tempfile::tempdir().unwrap();
    for srv in [&missing_meta_server, &bad_server] {
        let (client, _) = new_client(&srv.origin, client_root.path(), None);
        assert!(
            matches!(
                client.probe().await,
                CloudProbeOutcome::ProtocolIncompatible { .. }
            ),
            "expected ProtocolIncompatible for {}",
            srv.origin
        );
    }
}

#[tokio::test]
async fn cross_origin_redirect_never_forwards_authorization_header() {
    let target_received_auth = Arc::new(AtomicBool::new(false));
    let target_received_any = Arc::new(AtomicBool::new(false));

    let auth_flag = Arc::clone(&target_received_auth);
    let any_flag = Arc::clone(&target_received_any);
    let target_router = Router::new().route(
        "/steal",
        get(move |headers: HeaderMap| {
            let auth_flag = Arc::clone(&auth_flag);
            let any_flag = Arc::clone(&any_flag);
            async move {
                any_flag.store(true, Ordering::SeqCst);
                if headers.contains_key("authorization") {
                    auth_flag.store(true, Ordering::SeqCst);
                }
                StatusCode::OK
            }
        }),
    );
    let target_server = RunningHttpServer::start(target_router).await;

    let steal_url = format!("{}/steal", target_server.origin);
    let redirect_router = Router::new()
        .route(
            paths::META,
            get(|| async {
                Json(ServerMetaResponse {
                    service: "shiro-reader".to_owned(),
                    api_version: 1,
                    server_version: "0.0.1".to_owned(),
                    max_upload_bytes: 1024 * 1024,
                })
            }),
        )
        .route(
            paths::BOOKS,
            get(move || {
                let target = steal_url.clone();
                async move { Redirect::temporary(&target) }
            }),
        );
    let source_server = RunningHttpServer::start(redirect_router).await;

    let client_root = tempfile::tempdir().unwrap();
    let (client, cred_store) = new_client(&source_server.origin, client_root.path(), None);
    assert!(matches!(
        client.probe().await,
        CloudProbeOutcome::Compatible(_)
    ));

    // Seed a valid-looking session for source_server
    cred_store
        .save(
            &source_server.origin,
            &shiro_compatibility::CloudCredentials {
                origin: source_server.origin.clone(),
                user_id: "u1".into(),
                username: "Alice".into(),
                access_token: "super-secret-bearer".into(),
                refresh_token: "super-secret-refresh".into(),
            },
        )
        .unwrap();
    client.load_saved_credentials().unwrap();

    let err = client.list_books().await.unwrap_err();
    assert_eq!(err, ErrorCode::CloudCrossOriginRedirect);
    assert!(
        !target_received_auth.load(Ordering::SeqCst),
        "Authorization header MUST NEVER reach cross-origin redirect target"
    );
    assert!(
        !target_received_any.load(Ordering::SeqCst),
        "cross-origin redirect must be blocked before contacting target origin"
    );
}

#[tokio::test]
async fn single_flight_refresh_coalescing_transient_offline_recovery_and_revocation() {
    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();

    let refresh_calls = Arc::new(AtomicUsize::new(0));
    let counter_for_mw = Arc::clone(&refresh_calls);
    let reject_next_upload = Arc::new(AtomicBool::new(false));
    let reject_upload_for_mw = Arc::clone(&reject_next_upload);
    let instrumented_router =
        build_router(state.clone()).layer(middleware::from_fn(move |req: Request, next: Next| {
            let counter = Arc::clone(&counter_for_mw);
            let reject_upload = Arc::clone(&reject_upload_for_mw);
            async move {
                if req.uri().path() == paths::AUTH_REFRESH {
                    counter.fetch_add(1, Ordering::SeqCst);
                    tokio::time::sleep(Duration::from_millis(60)).await;
                }
                if req.method() == axum::http::Method::POST
                    && req.uri().path() == paths::BOOK_FILES
                    && reject_upload.swap(false, Ordering::SeqCst)
                {
                    let _ = axum::body::to_bytes(req.into_body(), usize::MAX).await;
                    return axum::response::IntoResponse::into_response((
                        StatusCode::UNAUTHORIZED,
                        Json(shiro_http_contract::ErrorResponse {
                            error: shiro_http_contract::ErrorDetail {
                                code: ErrorCode::AuthenticationInvalidToken.code(),
                                message: ErrorCode::AuthenticationInvalidToken.key().to_owned(),
                            },
                        }),
                    ));
                }
                next.run(req).await
            }
        }));

    let server = RunningHttpServer::start(instrumented_router).await;

    let client_root = tempfile::tempdir().unwrap();
    let invalidated_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = Arc::clone(&invalidated_flag);
    let (client, cred_store) = new_client(
        &server.origin,
        client_root.path(),
        Some(Arc::new(move || {
            flag_clone.store(true, Ordering::SeqCst);
        })),
    );
    let client = Arc::new(client);

    assert!(matches!(
        client.probe().await,
        CloudProbeOutcome::Compatible(_)
    ));

    // Login via Rust client
    let user = client
        .login("Alice".into(), "password123".into())
        .await
        .expect("cloud login");
    assert_eq!(user.username, "Alice");
    assert!(cred_store.load(&server.origin).unwrap().is_some());

    // Simulate expired/invalid access token and fire 8 concurrent list_books() calls
    client.replace_access_token_for_test("expired-invalid-token".into());
    let mut handles = Vec::new();
    for _ in 0..8 {
        let c = Arc::clone(&client);
        handles.push(tokio::spawn(async move { c.list_books().await }));
    }
    for handle in handles {
        let books = handle
            .await
            .unwrap()
            .expect("coalesced refresh must succeed");
        assert!(books.is_empty());
    }
    assert_eq!(
        refresh_calls.load(Ordering::SeqCst),
        1,
        "8 concurrent 401s must trigger exactly 1 /api/v1/auth/refresh call"
    );

    // Simulate access token expiring right before POST /api/v1/library/files/book:
    // upload_staged_book_file must refresh the token, rewind the staged temp file, and succeed.
    reject_next_upload.store(true, Ordering::SeqCst);
    let epub_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/packaging/fixtures/smoke_cover.epub");
    let epub_bytes = fs::read(&epub_path).expect("read smoke_cover.epub");
    let imported = client
        .import_reader_into_cloud("Refreshed Upload.epub", Cursor::new(epub_bytes), None, None)
        .await
        .expect("upload must succeed after refreshing access token on 401");
    assert_eq!(imported.title, "Refreshed Upload");
    assert_eq!(refresh_calls.load(Ordering::SeqCst), 2);

    // Transient server downtime preserves recoverable encrypted credentials
    let addr = server.stop().await;
    match client.verify_session().await {
        SessionVerifyOutcome::TransientFailure { code, .. } => {
            assert_eq!(code, ErrorCode::CloudNetworkUnreachable);
        }
        other => panic!("expected TransientFailure while offline, got {other:?}"),
    }
    assert!(
        client.has_saved_session(),
        "recoverable session must stay intact across transient outage"
    );
    assert!(
        cred_store
            .load(&format!("http://127.0.0.1:{}", addr.port()))
            .unwrap()
            .is_some()
    );
    assert!(!invalidated_flag.load(Ordering::SeqCst));

    // Bring server back up on same address -> session recovers cleanly!
    let restarted = RunningHttpServer::start_on_addr(addr, build_router(state.clone())).await;
    match client.verify_session().await {
        SessionVerifyOutcome::Valid(recovered) => assert_eq!(recovered.username, "Alice"),
        other => panic!("expected recovered valid session, got {other:?}"),
    }

    // Now revoke the session server-side using the stored refresh token
    let saved_refresh = cred_store
        .load(&restarted.origin)
        .unwrap()
        .unwrap()
        .refresh_token
        .clone();
    state
        .auth_service
        .logout(
            shiro_reader_server::application::identity::authentication::commands::LogoutCommand {
                token: saved_refresh,
            },
        )
        .await
        .unwrap();

    // Client discovers confirmed revocation -> clears credentials and fires callback!
    match client.verify_session().await {
        SessionVerifyOutcome::Invalidated(code) => {
            assert_eq!(code, ErrorCode::AuthenticationInvalidToken);
        }
        other => panic!("expected Invalidated after server logout, got {other:?}"),
    }
    assert!(!client.has_saved_session());
    assert!(cred_store.load(&restarted.origin).unwrap().is_none());
    assert!(invalidated_flag.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cloud_end_to_end_library_crud_streaming_upload_cover_and_user_isolation() {
    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    for user in ["Alice", "Bob"] {
        state
            .user_service
            .register(UserRegisterCommand {
                name: user.into(),
                password: "password123".into(),
            })
            .await
            .unwrap();
    }

    let server = RunningHttpServer::start(build_router(state)).await;

    let alice_dir = tempfile::tempdir().unwrap();
    let bob_dir = tempfile::tempdir().unwrap();
    let (alice, _) = new_client(&server.origin, alice_dir.path(), None);
    let (bob, _) = new_client(&server.origin, bob_dir.path(), None);

    for client in [&alice, &bob] {
        assert!(matches!(
            client.probe().await,
            CloudProbeOutcome::Compatible(_)
        ));
    }

    alice
        .login("Alice".into(), "password123".into())
        .await
        .unwrap();
    bob.login("Bob".into(), "password123".into()).await.unwrap();

    // Import real EPUB fixture with PNG cover into Alice's cloud library
    let epub_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/packaging/fixtures/smoke_cover.epub");
    let epub_bytes = fs::read(&epub_path).expect("read smoke_cover.epub");

    let mut progress_bytes = 0u64;
    let mut progress_cb = |n: u64| progress_bytes = n;
    let book = alice
        .import_reader_into_cloud(
            "Cloud Smoke.epub",
            Cursor::new(epub_bytes.clone()),
            None,
            Some(&mut progress_cb),
        )
        .await
        .expect("import epub into cloud");
    assert_eq!(book.title, "Cloud Smoke");
    assert_eq!(progress_bytes, epub_bytes.len() as u64);

    // Stream cover back from cloud server
    let (mut cover_stream, mime) = alice.get_cover_stream(&book.id).await.expect("cloud cover");
    assert_eq!(mime, "image/png");
    let mut cover_bytes = Vec::new();
    while let Some(chunk) = cover_stream.try_next().await.unwrap() {
        cover_bytes.extend_from_slice(&chunk);
    }
    assert!(cover_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    // Rename, tag, bind, trash, restore, and verify
    alice
        .rename_book(book.id.clone(), "Cloud Smoke Renamed".into())
        .await
        .unwrap();
    let tag = alice
        .define_tag("SciFi".into(), None, "#3366FF".into(), false, None)
        .await
        .unwrap();
    alice
        .bind_book_tag(&book.id, tag.id.clone(), None)
        .await
        .unwrap();
    assert_eq!(alice.get_book_tags(&book.id).await.unwrap().len(), 1);

    alice.delete_books(vec![book.id.clone()]).await.unwrap();
    assert_eq!(alice.list_trash().await.unwrap().len(), 1);
    alice.restore_book(&book.id).await.unwrap();
    assert_eq!(alice.list_books().await.unwrap().len(), 1);

    // Multi-user isolation: Bob sees zero of Alice's books and cannot access Alice's book cover/tags
    assert!(bob.list_books().await.unwrap().is_empty());
    assert_eq!(
        bob.get_cover_stream(&book.id).await.err().unwrap(),
        ErrorCode::BookNotFound
    );
    assert_eq!(
        bob.rename_book(book.id.clone(), "Hijack".into())
            .await
            .unwrap_err(),
        ErrorCode::BookNotFound
    );

    // Bob imports the same EPUB -> file deduplicates cleanly on server and Bob gets an independent book
    assert!(bob.file_exists(&book.hash).await.unwrap());
    let bob_book = bob
        .import_reader_into_cloud("Bob Copy.epub", Cursor::new(epub_bytes), None, None)
        .await
        .expect("bob dedup import");
    assert_ne!(bob_book.id, book.id);
    assert_eq!(bob.list_books().await.unwrap().len(), 1);

    // Cleanup tag and permanently delete Alice's book; logout clears session
    alice
        .undefine_tag(tag.id, DeleteTagStrategyDto::Cascade)
        .await
        .unwrap();
    alice.delete_books(vec![book.id.clone()]).await.unwrap();
    alice.permanently_delete_books(vec![book.id]).await.unwrap();
    assert!(alice.list_trash().await.unwrap().is_empty());

    alice.logout().await.unwrap();
    assert!(!alice.has_saved_session());
    assert_eq!(
        alice.list_books().await.unwrap_err(),
        ErrorCode::CloudNotAuthenticated
    );
}

#[tokio::test]
async fn client_runtime_cloud_lifecycle_mode_isolation_session_push_and_zero_tokens_in_ipc() {
    use shiro_compatibility::{
        BootstrapStateDto, ClientRuntime, ErrorSink, InitialSetupRequest, LaunchMode,
        PlatformCapabilities, StartupPhase, UpdateSettingsRequest,
    };
    use shiro_error_catalog::{AppLocale, ErrorEventPayload};

    struct RecordingSink {
        states: std::sync::Mutex<Vec<BootstrapStateDto>>,
        errors: std::sync::Mutex<Vec<ErrorEventPayload>>,
    }

    impl ErrorSink for RecordingSink {
        fn emit_error(&self, event: &ErrorEventPayload) {
            self.errors.lock().unwrap().push(event.clone());
        }

        fn emit_runtime_state(&self, state: &BootstrapStateDto) {
            self.states.lock().unwrap().push(state.clone());
        }
    }

    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "CloudUser".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();

    let server = RunningHttpServer::start(build_router(state.clone())).await;

    let app_root = tempfile::tempdir().unwrap();
    let config_dir = app_root.path().join("config");
    let default_lib = app_root.path().join("library");
    let sink = Arc::new(RecordingSink {
        states: std::sync::Mutex::new(Vec::new()),
        errors: std::sync::Mutex::new(Vec::new()),
    });

    let epub_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/packaging/fixtures/smoke_cover.epub"),
    )
    .unwrap();

    // 1. Start in Local mode and create Book A
    let local_rt = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(default_lib.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    local_rt
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    let local_book = local_rt
        .import_stream(
            "op-local",
            "Book A.epub",
            Cursor::new(epub_bytes.clone()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(local_book.title, "Book A");

    // Check cloud origin connectivity before switching
    let ok_check = local_rt.check_cloud_origin(server.origin.clone()).await;
    assert!(ok_check.ok);
    assert!(ok_check.message.is_none());
    let bad_url_check = local_rt.check_cloud_origin("http://[not-ipv6]".into()).await;
    assert!(!bad_url_check.ok);
    assert_eq!(
        bad_url_check.message.as_deref(),
        Some(ErrorCode::CloudInvalidOrigin.localize(AppLocale::En))
    );
    let dead_check = local_rt.check_cloud_origin("http://127.0.0.1:1".into()).await;
    assert!(!dead_check.ok);
    assert_eq!(
        dead_check.message.as_deref(),
        Some(ErrorCode::CloudNetworkUnreachable.localize(AppLocale::En))
    );

    // Switch settings to Cloud mode (next launch) -> stays Local until restart
    let pending = local_rt
        .update_settings(UpdateSettingsRequest {
            mode: Some(LaunchMode::Cloud),
            locale: None,
            local_library_root: None,
            cloud_origin: Some(server.origin.clone()),
        })
        .await
        .unwrap();
    assert!(pending.pending_restart);
    assert_eq!(pending.active_mode, Some(LaunchMode::Local));
    local_rt.shutdown().await;
    drop(local_rt);

    // 2. Restart in Cloud mode -> connects directly over HTTP and enters CloudAuthRequired
    let cloud_rt = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(default_lib.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    let boot1 = cloud_rt.bootstrap_state();
    assert_eq!(boot1.phase, StartupPhase::CloudAuthRequired);
    assert_eq!(boot1.active_mode, Some(LaunchMode::Cloud));

    // Login via Rust runtime -> transitions to Ready; verify ZERO token leakage in BootstrapStateDto JSON
    let after_login = cloud_rt
        .cloud_login("CloudUser".into(), "password123".into())
        .await
        .unwrap();
    assert_eq!(after_login.phase, StartupPhase::Ready);
    assert_eq!(
        after_login
            .cloud
            .as_ref()
            .and_then(|c| c.user.as_ref())
            .map(|u| u.username.as_str()),
        Some("CloudUser")
    );
    let ipc_json = serde_json::to_string(&after_login).unwrap();
    assert!(!ipc_json.contains("access_token"));
    assert!(!ipc_json.contains("refresh_token"));
    assert!(!ipc_json.contains("eyJ")); // No JWT segments in IPC DTO

    // Import Book B into Cloud mode and verify Book A is NOT visible in Cloud
    let cloud_book = cloud_rt
        .import_stream(
            "op-cloud",
            "Book B.epub",
            Cursor::new(epub_bytes.clone()),
            None,
            None,
        )
        .await
        .unwrap();
    assert_eq!(cloud_book.title, "Book B");
    let cloud_books = cloud_rt.list_books().await.unwrap();
    assert_eq!(cloud_books.len(), 1);
    assert_eq!(cloud_books[0].title, "Book B");

    // Stream cloud cover through unified ClientRuntime::get_cover_stream
    let (mut cover_stream, mime) = cloud_rt
        .get_cover_stream(cloud_book.id.clone())
        .await
        .unwrap();
    assert_eq!(mime, "image/png");
    let mut cover_bytes = Vec::new();
    while let Some(chunk) = cover_stream.try_next().await.unwrap() {
        cover_bytes.extend_from_slice(&chunk);
    }
    assert!(cover_bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    drop(cloud_rt);

    // 3. Restart in Cloud mode -> auto-recovers authenticated session from encrypted credentials
    let cloud_rt2 = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(default_lib.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    let boot2 = cloud_rt2.bootstrap_state();
    assert_eq!(boot2.phase, StartupPhase::Ready);
    assert_eq!(cloud_rt2.list_books().await.unwrap().len(), 1);

    // Revoke session server-side -> next operation triggers single-flight refresh -> 401 ->
    // runtime pushes CloudAuthRequired state event via ErrorSink!
    let cred_store = EncryptedFileCredentialStore::new(
        config_dir.join("keys"),
        app_root.path().join("cloud-credentials"),
    );
    let saved_refresh = cred_store
        .load(&server.origin)
        .unwrap()
        .unwrap()
        .refresh_token
        .clone();
    state
        .auth_service
        .logout(
            shiro_reader_server::application::identity::authentication::commands::LogoutCommand {
                token: saved_refresh,
            },
        )
        .await
        .unwrap();
    sink.states.lock().unwrap().clear();

    let err = cloud_rt2.list_books().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::AuthenticationInvalidToken.code());
    assert_eq!(
        cloud_rt2.bootstrap_state().phase,
        StartupPhase::CloudAuthRequired
    );
    assert!(
        sink.states
            .lock()
            .unwrap()
            .iter()
            .any(|s| s.phase == StartupPhase::CloudAuthRequired),
        "runtime must push CloudAuthRequired state event on confirmed session revocation"
    );

    // 4. Switch back to Local mode and restart -> local still has ONLY Book A (mode isolation!)
    cloud_rt2
        .update_settings(UpdateSettingsRequest {
            mode: Some(LaunchMode::Local),
            locale: None,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    drop(cloud_rt2);

    let local_rt2 = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(default_lib),
        AppLocale::En,
        None,
        sink,
    )
    .await;
    assert_eq!(local_rt2.bootstrap_state().phase, StartupPhase::Ready);
    let final_local_books = local_rt2.list_books().await.unwrap();
    assert_eq!(final_local_books.len(), 1);
    assert_eq!(final_local_books[0].title, "Book A");
}

#[tokio::test]
async fn persistent_401_after_successful_refresh_clears_session_and_notifies() {
    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();

    // Always reject /api/v1/library/bookshelf/books with 401 even after refresh succeeds.
    let router = build_router(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        async move {
            if req.uri().path() == paths::BOOKS {
                return axum::response::IntoResponse::into_response((
                    StatusCode::UNAUTHORIZED,
                    Json(shiro_http_contract::ErrorResponse {
                        error: shiro_http_contract::ErrorDetail {
                            code: ErrorCode::AuthenticationInvalidToken.code(),
                            message: ErrorCode::AuthenticationInvalidToken.key().to_owned(),
                        },
                    }),
                ));
            }
            next.run(req).await
        }
    }));

    let server = RunningHttpServer::start(router).await;
    let client_root = tempfile::tempdir().unwrap();
    let invalidated = Arc::new(AtomicBool::new(false));
    let invalidated_cb = Arc::clone(&invalidated);
    let (client, cred_store) = new_client(
        &server.origin,
        client_root.path(),
        Some(Arc::new(move || {
            invalidated_cb.store(true, Ordering::SeqCst);
        })),
    );

    client
        .login("Alice".into(), "password123".into())
        .await
        .expect("login");
    assert!(client.has_saved_session());

    let err = client.list_books().await.unwrap_err();
    assert_eq!(err, ErrorCode::AuthenticationInvalidToken);
    assert!(!client.has_saved_session());
    assert!(cred_store.load(&server.origin).unwrap().is_none());
    assert!(invalidated.load(Ordering::SeqCst));
}

#[tokio::test]
async fn cloud_import_respects_server_max_upload_bytes_and_reports_upload_progress() {
    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "Alice".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();
    let server = RunningHttpServer::start(build_router(state)).await;

    let client_root = tempfile::tempdir().unwrap();
    let (client, _) = new_client(&server.origin, client_root.path(), None);
    assert_eq!(client.max_upload_bytes(), DEFAULT_CLOUD_MAX_UPLOAD_BYTES);
    assert_eq!(DEFAULT_CLOUD_MAX_UPLOAD_BYTES, 512 * 1024 * 1024);
    assert!(matches!(
        client.probe().await,
        CloudProbeOutcome::Compatible(_)
    ));
    client
        .login("Alice".into(), "password123".into())
        .await
        .unwrap();

    let epub_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/packaging/fixtures/smoke_cover.epub");
    let epub_bytes = fs::read(&epub_path).expect("read smoke_cover.epub");
    let epub_len = epub_bytes.len() as u64;

    let mut progress_samples = Vec::new();
    let mut progress_cb = |bytes: u64| progress_samples.push(bytes);
    let book = client
        .import_reader_into_cloud(
            "Progress Book.epub",
            Cursor::new(epub_bytes.clone()),
            None,
            Some(&mut progress_cb),
        )
        .await
        .expect("import epub");
    assert_eq!(book.title, "Progress Book");
    assert!(
        progress_samples.len() >= 2,
        "expected progress reports from both staging and HTTP upload, got {progress_samples:?}"
    );
    assert!(
        progress_samples.windows(2).all(|w| w[0] <= w[1]),
        "progress samples must be monotonic: {progress_samples:?}"
    );
    assert!(
        progress_samples[0] <= epub_len / 2,
        "first staging progress should be in first half: {progress_samples:?}"
    );
    assert!(
        progress_samples.iter().any(|&v| v > epub_len / 2),
        "HTTP upload phase should advance progress past 50%: {progress_samples:?}"
    );
    assert_eq!(progress_samples.last().copied(), Some(epub_len));

    // Server advertising a smaller max_upload_bytes must reject oversized uploads before transfer.
    let small_meta_router = Router::new().route(
        paths::META,
        get(|| async {
            Json(ServerMetaResponse {
                service: "shiro-reader".to_owned(),
                api_version: 1,
                server_version: "1.0.0-test".to_owned(),
                max_upload_bytes: 128,
            })
        }),
    );
    let limited_server = RunningHttpServer::start(small_meta_router).await;
    let limited_dir = tempfile::tempdir().unwrap();
    let (limited_client, _) = new_client(&limited_server.origin, limited_dir.path(), None);
    assert!(matches!(
        limited_client.probe().await,
        CloudProbeOutcome::Compatible(meta) if meta.max_upload_bytes == 128
    ));
    assert_eq!(limited_client.max_upload_bytes(), 128);

    let err = limited_client
        .import_reader_into_cloud("Too Big.epub", Cursor::new(epub_bytes), None, None)
        .await
        .unwrap_err();
    assert_eq!(err, ErrorCode::CommonPayloadTooLarge);
}
