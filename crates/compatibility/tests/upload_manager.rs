use std::{
    collections::HashMap,
    fs,
    future::Future,
    io::Cursor,
    path::{Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use axum::{
    Router,
    extract::Request,
    http::{Method, StatusCode},
    middleware::{self, Next},
    response::IntoResponse,
};
use shiro_compatibility::{
    AppTheme, BootstrapStateDto, BoxedUploadReader, ClientRuntime, EnqueueUploadItem, ErrorSink,
    InitialSetupRequest, LaunchMode, PlatformCapabilities, StartupPhase, UpdateSettingsRequest,
    UploadActiveStage, UploadPanelSnapshotDto, UploadSessionStatsDto, UploadSourceLocator,
    UploadSourceOpener, UploadTaskStatusDto,
};
use shiro_error_catalog::{AppLocale, ErrorCode, ErrorEventPayload};
use shiro_http_contract::paths;
use shiro_reader_server::{
    app::build_router,
    application::identity::user::commands::UserRegisterCommand,
    config::{Config, CryptoConfig, LogFormat, ServerConfig as AppServerConfig, TelemetryConfig},
    state::AppState,
};
use tokio::{
    io::AsyncWriteExt,
    net::TcpListener,
    sync::{Barrier, Notify, OnceCell, oneshot},
    time::Instant,
};

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn desktop_caps(default_root: PathBuf) -> PlatformCapabilities {
    PlatformCapabilities {
        can_pick_library_dir: true,
        can_configure_log_level: true,
        can_import_books: true,
        retain_completed_uploads: false,
        default_library_root: default_root,
        cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
        font_url_base: "shiro-font://localhost/font/".to_owned(),
        can_enumerate_system_fonts: true,
        epub_cache_max_bytes: 64 * 1024 * 1024,
    }
}

fn android_caps(private_root: PathBuf) -> PlatformCapabilities {
    PlatformCapabilities {
        can_pick_library_dir: false,
        can_configure_log_level: false,
        can_import_books: true,
        retain_completed_uploads: true,
        default_library_root: private_root,
        cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
        font_url_base: "http://shiro-font.localhost/font/".to_owned(),
        can_enumerate_system_fonts: false,
        epub_cache_max_bytes: 24 * 1024 * 1024,
    }
}

fn unsupported_platform_caps(private_root: PathBuf) -> PlatformCapabilities {
    PlatformCapabilities {
        can_pick_library_dir: false,
        can_configure_log_level: false,
        can_import_books: false,
        retain_completed_uploads: false,
        default_library_root: private_root,
        cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
        font_url_base: "shiro-font://localhost/font/".to_owned(),
        can_enumerate_system_fonts: true,
        epub_cache_max_bytes: 64 * 1024 * 1024,
    }
}

fn sample_epub_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("apps/server/tests/samples/sample_document.epub"))
        .expect("sample_document.epub")
}

fn sample_pdf_bytes() -> Vec<u8> {
    fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf")
}

#[derive(Default)]
struct RecordingSink {
    errors: Mutex<Vec<ErrorEventPayload>>,
    upload_snapshots: Mutex<Vec<UploadPanelSnapshotDto>>,
    runtime_states: Mutex<Vec<BootstrapStateDto>>,
    notify: Notify,
}

impl RecordingSink {
    fn errors(&self) -> Vec<ErrorEventPayload> {
        self.errors.lock().unwrap().clone()
    }

    fn upload_snapshots(&self) -> Vec<UploadPanelSnapshotDto> {
        self.upload_snapshots.lock().unwrap().clone()
    }

    fn clear_errors(&self) {
        self.errors.lock().unwrap().clear();
    }

    async fn wait_for_upload<F>(&self, timeout: Duration, mut predicate: F) -> UploadPanelSnapshotDto
    where
        F: FnMut(&UploadPanelSnapshotDto) -> bool,
    {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(found) = self
                .upload_snapshots
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|snap| predicate(snap))
                .cloned()
            {
                return found;
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let latest = self.upload_snapshots.lock().unwrap().last().cloned();
                panic!("timed out waiting for upload snapshot condition; latest={latest:?}");
            }
            let _ = tokio::time::timeout(remaining, self.notify.notified()).await;
        }
    }
}

impl ErrorSink for RecordingSink {
    fn emit_error(&self, event: &ErrorEventPayload) {
        self.errors.lock().unwrap().push(event.clone());
        self.notify.notify_waiters();
    }

    fn emit_runtime_state(&self, state: &BootstrapStateDto) {
        self.runtime_states.lock().unwrap().push(state.clone());
        self.notify.notify_waiters();
    }

    fn emit_upload_state(&self, snapshot: &UploadPanelSnapshotDto) {
        self.upload_snapshots.lock().unwrap().push(snapshot.clone());
        self.notify.notify_waiters();
    }
}

fn assert_stats_invariant(snap: &UploadPanelSnapshotDto) {
    assert_eq!(
        snap.stats.total,
        snap.stats.remaining + snap.stats.succeeded + snap.stats.failed,
        "stats invariant violated: {snap:?}"
    );
    assert!(
        snap.stats.active <= snap.stats.remaining,
        "active > remaining: {snap:?}"
    );
}

struct BarrierOpener {
    barrier: Arc<Barrier>,
    gated_uris: Vec<String>,
    payloads: HashMap<String, Vec<u8>>,
    release_notify: Option<Arc<Notify>>,
    opened_count: Arc<AtomicUsize>,
}

impl UploadSourceOpener for BarrierOpener {
    fn open_source(
        &self,
        locator: &UploadSourceLocator,
    ) -> Pin<Box<dyn Future<Output = Result<(BoxedUploadReader, Option<u64>), ErrorCode>> + Send>>
    {
        let uri = match locator {
            UploadSourceLocator::Uri(u) => u.clone(),
            UploadSourceLocator::Path(p) => p.display().to_string(),
        };
        let is_gated = self.gated_uris.iter().any(|u| u == &uri);
        let barrier = Arc::clone(&self.barrier);
        let release = self.release_notify.clone();
        let bytes = self.payloads.get(&uri).cloned();
        let opened_count = Arc::clone(&self.opened_count);

        Box::pin(async move {
            opened_count.fetch_add(1, Ordering::SeqCst);
            if is_gated {
                barrier.wait().await;
                if let Some(notify) = release {
                    notify.notified().await;
                }
            }
            let data = bytes.ok_or(ErrorCode::ImportReadFailed)?;
            let len = data.len() as u64;
            Ok((Box::pin(Cursor::new(data)) as BoxedUploadReader, Some(len)))
        })
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
            return workspace_root().join(configured);
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
            let libs = shiro_reader_server::infrastructure::dependencies::libs_dir(&shared_root);
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
            jwt_secret: "test-upload-manager-jwt-secret-key-0123456789".into(),
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

struct RunningServer {
    origin: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
}

impl RunningServer {
    async fn start(router: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.await;
                })
                .await;
        });
        Self {
            origin: format!("http://127.0.0.1:{}", addr.port()),
            shutdown_tx: Some(shutdown_tx),
        }
    }
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[tokio::test]
async fn parallel_multi_thread_execution_and_lock_free_append_while_running() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    let epub = sample_epub_bytes();
    let pdf = sample_pdf_bytes();
    let mut payloads = HashMap::new();
    payloads.insert("mem://book1".to_owned(), epub.clone());
    payloads.insert("mem://book2".to_owned(), pdf.clone());
    payloads.insert("mem://book3".to_owned(), epub.clone());

    let release = Arc::new(Notify::new());
    let opener = Arc::new(BarrierOpener {
        // Barrier of 2 proves both workers execute concurrently (serial execution would deadlock!).
        barrier: Arc::new(Barrier::new(2)),
        gated_uris: vec!["mem://book1".to_owned(), "mem://book2".to_owned()],
        payloads,
        release_notify: Some(Arc::clone(&release)),
        opened_count: Arc::new(AtomicUsize::new(0)),
    });

    let runtime = ClientRuntime::initialize_with_source_opener(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        sink.clone(),
        opener.clone(),
    )
    .await;

    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();

    // Enqueue 2 books that synchronize at a 2-party Barrier before pausing on `release`.
    let snap1 = runtime
        .upload_enqueue(vec![
            EnqueueUploadItem {
                file_name: "Book One.epub".to_owned(),
                total_bytes: Some(epub.len() as u64),
                locator: UploadSourceLocator::Uri("mem://book1".to_owned()),
            },
            EnqueueUploadItem {
                file_name: "Book Two.pdf".to_owned(),
                total_bytes: Some(pdf.len() as u64),
                locator: UploadSourceLocator::Uri("mem://book2".to_owned()),
            },
        ])
        .await
        .unwrap();
    assert!(snap1.visible);
    assert_eq!(snap1.stats.total, 2);
    assert_eq!(snap1.stats.active, 2);
    assert_stats_invariant(&snap1);

    // Wait until both workers have passed the Barrier and entered open_source concurrently.
    tokio::time::timeout(Duration::from_secs(3), async {
        while opener.opened_count.load(Ordering::SeqCst) < 2 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("both workers reached opener concurrently");

    // While book1 & book2 are active, appending book3 must succeed immediately without blocking!
    let t0 = Instant::now();
    let snap2 = runtime
        .upload_enqueue(vec![EnqueueUploadItem {
            file_name: "Book Three.epub".to_owned(),
            total_bytes: Some(epub.len() as u64),
            locator: UploadSourceLocator::Uri("mem://book3".to_owned()),
        }])
        .await
        .unwrap();
    assert!(
        t0.elapsed() < Duration::from_millis(500),
        "enqueue while active must be non-blocking"
    );
    assert_eq!(snap2.stats.total, 3);
    assert_stats_invariant(&snap2);

    // Release the paused workers and wait for all 3 to succeed and auto-dismiss after 1s.
    release.notify_waiters();
    let done = sink
        .wait_for_upload(Duration::from_secs(5), |s| s.completed_counter == 3)
        .await;
    assert_stats_invariant(&done);
    assert_eq!(done.stats.succeeded, 3);

    // After ~1000ms auto-dismiss, panel hides and stats reset while completed_counter stays 3!
    let hidden = sink
        .wait_for_upload(Duration::from_secs(3), |s| !s.visible && s.completed_counter == 3)
        .await;
    assert_eq!(hidden.completed_counter, 3);
    assert_eq!(hidden.stats.total, 0);
    assert_eq!(hidden.stats.succeeded, 0);
    assert!(hidden.tasks.is_empty());

    let books = runtime.list_books().await.unwrap();
    assert_eq!(books.len(), 3);
    // Verify revision monotonicity across every emitted snapshot.
    let snaps = sink.upload_snapshots();
    for pair in snaps.windows(2) {
        assert!(pair[1].revision > pair[0].revision);
        assert_stats_invariant(&pair[1]);
    }
    assert!(sink.errors().is_empty());
}

#[tokio::test]
async fn dynamic_concurrency_scaling_and_settings_persistence_survives_theme_update() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    let epub = sample_epub_bytes();
    let mut payloads = HashMap::new();
    for i in 1..=3 {
        payloads.insert(format!("mem://scale{i}"), epub.clone());
    }
    let opener = Arc::new(BarrierOpener {
        // Barrier of 3 can ONLY finish once concurrency scales up from 1 -> 3!
        barrier: Arc::new(Barrier::new(3)),
        gated_uris: vec![
            "mem://scale1".to_owned(),
            "mem://scale2".to_owned(),
            "mem://scale3".to_owned(),
        ],
        payloads,
        release_notify: None,
        opened_count: Arc::new(AtomicUsize::new(0)),
    });

    let runtime = ClientRuntime::initialize_with_source_opener(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        sink.clone(),
        opener.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();

    // Scale down to 1 before enqueuing 3 tasks.
    let s1 = runtime.set_upload_concurrency(1).await.unwrap();
    assert_eq!(s1.concurrency, 1);

    let enqueued = runtime
        .upload_enqueue(
            (1..=3)
                .map(|i| EnqueueUploadItem {
                    file_name: format!("Scale {i}.epub"),
                    total_bytes: Some(epub.len() as u64),
                    locator: UploadSourceLocator::Uri(format!("mem://scale{i}")),
                })
                .collect(),
        )
        .await
        .unwrap();
    assert_eq!(enqueued.stats.active, 1);
    assert_eq!(enqueued.stats.remaining, 3);

    // Dynamically scale up to 3 while running -> remaining 2 start immediately and pass Barrier(3)!
    let s3 = runtime.set_upload_concurrency(3).await.unwrap();
    assert_eq!(s3.concurrency, 3);
    assert_eq!(s3.stats.active, 3);

    sink.wait_for_upload(Duration::from_secs(5), |s| s.completed_counter == 3)
        .await;

    // Changing theme must NOT overwrite upload_concurrency in settings.json!
    runtime.set_theme(AppTheme::Green).unwrap();
    runtime.shutdown().await;

    // Reload runtime and verify upload_concurrency == 3 persisted cleanly.
    let reloaded = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
    )
    .await;
    assert_eq!(reloaded.upload_snapshot().concurrency, 3);
}

#[tokio::test]
async fn cloud_single_flight_coalesces_duplicate_file_uploads_and_supports_leader_cancel_takeover()
{
    let tmp = tempfile::tempdir().unwrap();
    let server_root = tmp.path().join("server");
    let config_dir = tmp.path().join("client_cfg");
    let lib_root = tmp.path().join("client_lib");

    let state = build_real_v1_server_state(&server_root).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "UploadUser".to_owned(),
            password: "password123".to_owned(),
        })
        .await
        .unwrap();

    let post_count = Arc::new(AtomicUsize::new(0));
    let pause_first_post = Arc::new(AtomicBool::new(true));
    let first_post_entered = Arc::new(Notify::new());
    let first_post_release = Arc::new(Notify::new());

    let post_count_mw = Arc::clone(&post_count);
    let pause_mw = Arc::clone(&pause_first_post);
    let entered_mw = Arc::clone(&first_post_entered);
    let release_mw = Arc::clone(&first_post_release);

    let router = build_router(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        let post_count = Arc::clone(&post_count_mw);
        let pause = Arc::clone(&pause_mw);
        let entered = Arc::clone(&entered_mw);
        let release = Arc::clone(&release_mw);
        async move {
            let is_book_post =
                req.method() == Method::POST && req.uri().path() == paths::BOOK_FILES;
            if is_book_post {
                post_count.fetch_add(1, Ordering::SeqCst);
                if pause.swap(false, Ordering::SeqCst) {
                    entered.notify_waiters();
                    release.notified().await;
                }
            }
            next.run(req).await
        }
    }));

    let server = RunningServer::start(router).await;
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Cloud,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: Some(server.origin.clone()),
        })
        .await
        .unwrap();
    runtime
        .cloud_login("UploadUser".to_owned(), "password123".to_owned())
        .await
        .unwrap();

    let epub_bytes = sample_epub_bytes();
    let f1 = tmp.path().join("Same Content A.epub");
    let f2 = tmp.path().join("Same Content B.epub");
    fs::write(&f1, &epub_bytes).unwrap();
    fs::write(&f2, &epub_bytes).unwrap();

    // Case 1: Enqueue f1 alone first so it deterministically becomes Leader and enters POST /files/book.
    let snap1 = runtime
        .upload_enqueue_paths(vec![f1.clone()])
        .await
        .unwrap();
    assert_eq!(snap1.tasks.len(), 1);
    let leader_id = snap1.tasks[0].id;

    tokio::time::timeout(Duration::from_secs(5), first_post_entered.notified())
        .await
        .expect("leader entered POST /files/book");

    // Enqueue f2 while Leader is blocked inside POST and wait until f2 joins FlightRole::Follower.
    let snap2 = runtime
        .upload_enqueue_paths(vec![f2.clone()])
        .await
        .unwrap();
    let follower_id = snap2
        .tasks
        .iter()
        .find(|t| t.id != leader_id)
        .expect("follower task")
        .id;

    sink.wait_for_upload(Duration::from_secs(5), |s| {
        s.tasks.iter().any(|t| {
            t.id == follower_id
                && matches!(
                    t.status,
                    UploadTaskStatusDto::Active {
                        stage: UploadActiveStage::Uploading,
                        ..
                    }
                )
        })
    })
    .await;

    // Cancel Leader BEFORE releasing the blocked first POST; Follower must take over and upload!
    let _ = runtime.upload_cancel_task(leader_id).await;

    sink.wait_for_upload(Duration::from_secs(5), |s| {
        s.completed_counter == 1 && s.stats.remaining == 0
    })
    .await;

    assert_eq!(
        post_count.load(Ordering::SeqCst),
        2,
        "cancelling leader must trigger follower takeover for a second POST /files/book"
    );
    let case1_books = runtime.list_books().await.unwrap();
    assert_eq!(case1_books.len(), 1);
    assert_eq!(case1_books[0].title, "Same Content B");

    first_post_release.notify_waiters();

    // Now test pure coalescing with two fresh PDF copies (identical content, neither cancelled):
    // POST /files/book must be invoked ONLY ONCE for the PDF hash!
    let pdf_bytes = sample_pdf_bytes();
    let p1 = tmp.path().join("Coalesced PDF 1.pdf");
    let p2 = tmp.path().join("Coalesced PDF 2.pdf");
    fs::write(&p1, &pdf_bytes).unwrap();
    fs::write(&p2, &pdf_bytes).unwrap();

    let before_pdf_posts = post_count.load(Ordering::SeqCst);
    pause_first_post.store(true, Ordering::SeqCst);

    runtime
        .upload_enqueue_paths(vec![p1, p2])
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(5), first_post_entered.notified())
        .await
        .expect("pdf leader entered POST");
    // Give sibling task a short moment to join the flight map as Follower.
    tokio::time::sleep(Duration::from_millis(80)).await;
    first_post_release.notify_waiters();

    sink.wait_for_upload(Duration::from_secs(5), |s| {
        s.completed_counter == 3 && s.stats.remaining == 0
    })
    .await;

    let pdf_posts = post_count.load(Ordering::SeqCst) - before_pdf_posts;
    assert_eq!(
        pdf_posts, 1,
        "two identical concurrent uploads must coalesce to 1 POST /files/book"
    );

    let books = runtime.list_books().await.unwrap();
    assert_eq!(books.len(), 3);
}

#[tokio::test]
async fn cloud_session_invalidation_during_upload_aborts_without_deadlock() {
    let tmp = tempfile::tempdir().unwrap();
    let server_root = tmp.path().join("server");
    let config_dir = tmp.path().join("client_cfg");
    let lib_root = tmp.path().join("client_lib");

    let state = build_real_v1_server_state(&server_root).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "RevokedUser".to_owned(),
            password: "password123".to_owned(),
        })
        .await
        .unwrap();

    let revoke = Arc::new(AtomicBool::new(false));
    let revoke_mw = Arc::clone(&revoke);
    let router = build_router(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        let revoke = Arc::clone(&revoke_mw);
        async move {
            if revoke.load(Ordering::SeqCst) && req.uri().path().starts_with("/api/v1/library")
                || (revoke.load(Ordering::SeqCst) && req.uri().path() == paths::AUTH_REFRESH)
            {
                return (
                    StatusCode::UNAUTHORIZED,
                    axum::Json(serde_json::json!({
                        "error": { "code": 1003, "message": "identity.authentication.invalid_token" }
                    })),
                )
                    .into_response();
            }
            next.run(req).await
        }
    }));

    let server = RunningServer::start(router).await;
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Cloud,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: Some(server.origin.clone()),
        })
        .await
        .unwrap();
    runtime
        .cloud_login("RevokedUser".to_owned(), "password123".to_owned())
        .await
        .unwrap();

    // Revoke session right before uploading 3 books.
    revoke.store(true, Ordering::SeqCst);
    let epub = sample_epub_bytes();
    let mut paths_to_upload = Vec::new();
    for i in 1..=3 {
        let p = tmp.path().join(format!("Revoked {i}.epub"));
        fs::write(&p, &epub).unwrap();
        paths_to_upload.push(p);
    }

    runtime.upload_enqueue_paths(paths_to_upload).await.unwrap();

    // Must transition to CloudAuthRequired and abort all queued/active uploads without deadlocking!
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let boot = runtime.bootstrap_state();
            let snap = runtime.upload_snapshot();
            if boot.phase == StartupPhase::CloudAuthRequired && snap.stats.remaining == 0 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("session invalidation must abort uploads cleanly without deadlocking");
}

#[tokio::test]
async fn shutdown_drains_active_uploads_and_checkpoints_sqlite_wal_cleanly() {
    struct StalledOpener {
        entered: Arc<Notify>,
    }

    impl UploadSourceOpener for StalledOpener {
        fn open_source(
            &self,
            _locator: &UploadSourceLocator,
        ) -> Pin<Box<dyn Future<Output = Result<(BoxedUploadReader, Option<u64>), ErrorCode>> + Send>>
        {
            let entered = Arc::clone(&self.entered);
            Box::pin(async move {
                let (mut writer, reader) = tokio::io::duplex(64);
                let _ = writer.write_all(b"%PDF-1.4\n").await;
                entered.notify_one();
                // Keep writer alive inside a background task so reader stalls until cancelled.
                tokio::spawn(async move {
                    let _hold = writer;
                    futures_util::future::pending::<()>().await;
                });
                Ok((Box::pin(reader) as BoxedUploadReader, Some(1024)))
            })
        }
    }

    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let entered = Arc::new(Notify::new());

    let runtime = ClientRuntime::initialize_with_source_opener(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
        Arc::new(StalledOpener {
            entered: Arc::clone(&entered),
        }),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();

    runtime
        .upload_enqueue(vec![EnqueueUploadItem {
            file_name: "Stalled.pdf".to_owned(),
            total_bytes: Some(1024),
            locator: UploadSourceLocator::Uri("mem://stalled".to_owned()),
        }])
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(3), entered.notified())
        .await
        .expect("worker entered stalled read");

    // Shutdown must abort & drain the stalled worker, drop Arc<LocalLibrary>, and checkpoint WAL.
    tokio::time::timeout(Duration::from_secs(3), runtime.shutdown())
        .await
        .expect("shutdown completes promptly");

    let wal_path = lib_root.join("library.db-wal");
    if wal_path.exists() {
        assert_eq!(
            fs::metadata(&wal_path).unwrap().len(),
            0,
            "SQLite WAL must be cleanly truncated after shutdown"
        );
    }
    let temp_dir = lib_root.join("books").join(".temp");
    if temp_dir.exists() {
        assert_eq!(
            fs::read_dir(&temp_dir).unwrap().count(),
            0,
            "staged temp files must be cleaned up when worker is aborted"
        );
    }
}

#[tokio::test]
async fn empty_directory_mobile_block_error_relocalization_and_retry_without_global_toast_spam() {
    let tmp = tempfile::tempdir().unwrap();

    // 1. Unsupported platform (can_import_books = false) blocks upload enqueue with ImportNotSupportedOnPlatform (3030).
    let mobile_sink = Arc::new(RecordingSink::default());
    let mobile_rt = ClientRuntime::initialize(
        tmp.path().join("mobile_cfg"),
        unsupported_platform_caps(tmp.path().join("mobile_lib")),
        AppLocale::En,
        None,
        mobile_sink.clone(),
    )
    .await;
    let boot = mobile_rt
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    assert!(!boot.can_import_books);

    let err = mobile_rt
        .upload_enqueue_paths(vec![tmp.path().join("any.epub")])
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ImportNotSupportedOnPlatform.code());
    assert_eq!(mobile_sink.errors().len(), 1);

    // 2. Desktop: empty directory -> ImportNoSupportedBooksFound (3031) emits global toast.
    let desk_sink = Arc::new(RecordingSink::default());
    let desk_rt = ClientRuntime::initialize(
        tmp.path().join("desk_cfg"),
        desktop_caps(tmp.path().join("desk_lib")),
        AppLocale::En,
        None,
        desk_sink.clone(),
    )
    .await;
    let desk_boot = desk_rt
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    assert!(desk_boot.can_import_books);

    let empty_dir = tmp.path().join("empty_books");
    fs::create_dir_all(&empty_dir).unwrap();
    fs::write(empty_dir.join("notes.txt"), b"not a book").unwrap();
    let dir_err = desk_rt
        .upload_enqueue_paths(vec![empty_dir])
        .await
        .unwrap_err();
    assert_eq!(dir_err.code, ErrorCode::ImportNoSupportedBooksFound.code());
    assert_eq!(desk_sink.errors().len(), 1);

    // 3. Per-task failure (corrupt file) stays inline inside UploadPanel (NO global toast!),
    //    relocalizes on set_locale & update_settings, and succeeds on retry once fixed!
    desk_sink.clear_errors();
    let fixable_path = tmp.path().join("Fixable.epub");
    fs::write(&fixable_path, b"corrupt bytes").unwrap();

    desk_rt
        .upload_enqueue_paths(vec![fixable_path.clone()])
        .await
        .unwrap();

    let failed_snap = desk_sink
        .wait_for_upload(Duration::from_secs(3), |s| s.stats.failed == 1)
        .await;
    assert!(
        desk_sink.errors().is_empty(),
        "per-task failures must NOT flood global shiro://error alerts"
    );
    assert!(failed_snap.has_dismissible_errors);
    let task_id = failed_snap.tasks[0].id;
    let en_msg = match &failed_snap.tasks[0].status {
        UploadTaskStatusDto::Failed { error } => error.message.clone(),
        other => panic!("expected Failed status, got {other:?}"),
    };
    assert_eq!(en_msg, ErrorCode::BookFileUnsupported.localize(AppLocale::En));

    // Switch locale to Zh via set_locale -> failed task message relocalizes in-place!
    desk_rt.set_locale(AppLocale::Zh).unwrap();
    let zh_snap = desk_sink
        .wait_for_upload(Duration::from_secs(2), |s| {
            matches!(
                s.tasks.first().map(|t| &t.status),
                Some(UploadTaskStatusDto::Failed { error }) if error.locale == AppLocale::Zh
            )
        })
        .await;
    if let UploadTaskStatusDto::Failed { error } = &zh_snap.tasks[0].status {
        assert_eq!(error.message, ErrorCode::BookFileUnsupported.localize(AppLocale::Zh));
    }

    // Switch back to En via update_settings -> relocalizes again!
    desk_rt
        .update_settings(UpdateSettingsRequest {
            mode: None,
            locale: Some(AppLocale::En),
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    desk_sink
        .wait_for_upload(Duration::from_secs(2), |s| {
            matches!(
                s.tasks.first().map(|t| &t.status),
                Some(UploadTaskStatusDto::Failed { error }) if error.locale == AppLocale::En
            )
        })
        .await;

    // Fix the file on disk and retry without re-picking!
    fs::write(&fixable_path, sample_epub_bytes()).unwrap();
    desk_rt.upload_retry_task(task_id).await.unwrap();
    let retried_ok = desk_sink
        .wait_for_upload(Duration::from_secs(5), |s| s.completed_counter == 1)
        .await;
    assert_eq!(retried_ok.stats.failed, 0);
    assert_eq!(retried_ok.stats.succeeded, 1);
}

#[tokio::test]
async fn cloud_logout_and_session_invalidation_clear_failed_tasks_across_sessions() {
    let tmp = tempfile::tempdir().unwrap();
    let server_root = tmp.path().join("server");
    let config_dir = tmp.path().join("client_cfg");
    let lib_root = tmp.path().join("client_lib");

    let state = build_real_v1_server_state(&server_root).await;
    for name in ["AliceUser", "BobUser"] {
        state
            .user_service
            .register(UserRegisterCommand {
                name: name.to_owned(),
                password: "password123".to_owned(),
            })
            .await
            .unwrap();
    }

    let server = RunningServer::start(build_router(state)).await;
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Cloud,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: Some(server.origin.clone()),
        })
        .await
        .unwrap();
    runtime
        .cloud_login("AliceUser".to_owned(), "password123".to_owned())
        .await
        .unwrap();

    // Enqueue a failing file under AliceUser so a Failed task remains in the upload panel.
    let bad_path = tmp.path().join("CrossSessionBook.epub");
    fs::write(&bad_path, b"not a valid epub").unwrap();
    runtime
        .upload_enqueue_paths(vec![bad_path.clone()])
        .await
        .unwrap();
    let failed_snap = sink
        .wait_for_upload(Duration::from_secs(3), |s| s.stats.failed == 1 && s.visible)
        .await;
    let failed_id = failed_snap.tasks[0].id;

    // Logout must clear ALL tasks (including Failed) and hide the panel.
    runtime.cloud_logout().await.unwrap();
    let after_logout = runtime.upload_snapshot();
    assert!(!after_logout.visible);
    assert!(after_logout.tasks.is_empty());
    assert_eq!(after_logout.stats, UploadSessionStatsDto::default());

    // Log in as BobUser and verify AliceUser's failed task cannot be retried into BobUser's library.
    runtime
        .cloud_login("BobUser".to_owned(), "password123".to_owned())
        .await
        .unwrap();
    fs::write(&bad_path, sample_epub_bytes()).unwrap();
    let retry_snap = runtime.upload_retry_task(failed_id).await.unwrap();
    assert!(!retry_snap.visible);
    assert!(retry_snap.tasks.is_empty());
    assert!(runtime.list_books().await.unwrap().is_empty());
}

#[tokio::test]
async fn cloud_single_flight_propagates_deterministic_error_without_redundant_reuploads() {
    let tmp = tempfile::tempdir().unwrap();
    let server_root = tmp.path().join("server");
    let config_dir = tmp.path().join("client_cfg");
    let lib_root = tmp.path().join("client_lib");

    let state = build_real_v1_server_state(&server_root).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "CorruptUser".to_owned(),
            password: "password123".to_owned(),
        })
        .await
        .unwrap();

    let post_count = Arc::new(AtomicUsize::new(0));
    let pause_first_post = Arc::new(AtomicBool::new(true));
    let first_post_entered = Arc::new(Notify::new());
    let first_post_release = Arc::new(Notify::new());

    let post_count_mw = Arc::clone(&post_count);
    let pause_mw = Arc::clone(&pause_first_post);
    let entered_mw = Arc::clone(&first_post_entered);
    let release_mw = Arc::clone(&first_post_release);

    let router = build_router(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        let post_count = Arc::clone(&post_count_mw);
        let pause = Arc::clone(&pause_mw);
        let entered = Arc::clone(&entered_mw);
        let release = Arc::clone(&release_mw);
        async move {
            if req.method() == Method::POST && req.uri().path() == paths::BOOK_FILES {
                post_count.fetch_add(1, Ordering::SeqCst);
                if pause.swap(false, Ordering::SeqCst) {
                    entered.notify_waiters();
                    release.notified().await;
                }
                return (
                    StatusCode::BAD_REQUEST,
                    axum::Json(serde_json::json!({
                        "error": { "code": 2106, "message": "library.book.file.unsupported" }
                    })),
                )
                    .into_response();
            }
            next.run(req).await
        }
    }));

    let server = RunningServer::start(router).await;
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Cloud,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: Some(server.origin.clone()),
        })
        .await
        .unwrap();
    runtime
        .cloud_login("CorruptUser".to_owned(), "password123".to_owned())
        .await
        .unwrap();

    let epub_bytes = sample_epub_bytes();
    let c1 = tmp.path().join("Broken Copy 1.epub");
    let c2 = tmp.path().join("Broken Copy 2.epub");
    let c3 = tmp.path().join("Broken Copy 3.epub");
    for path in [&c1, &c2, &c3] {
        fs::write(path, &epub_bytes).unwrap();
    }

    runtime.upload_enqueue_paths(vec![c1]).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), first_post_entered.notified())
        .await
        .expect("leader entered POST /files/book");

    runtime.upload_enqueue_paths(vec![c2, c3]).await.unwrap();
    sink.wait_for_upload(Duration::from_secs(5), |s| {
        s.tasks.len() == 3
            && s.tasks.iter().all(|t| {
                matches!(
                    t.status,
                    UploadTaskStatusDto::Active {
                        stage: UploadActiveStage::Uploading,
                        ..
                    }
                )
            })
    })
    .await;

    first_post_release.notify_waiters();

    let failed_snap = sink
        .wait_for_upload(Duration::from_secs(5), |s| {
            s.stats.remaining == 0 && s.stats.failed == 3
        })
        .await;

    assert_eq!(
        post_count.load(Ordering::SeqCst),
        1,
        "deterministic upload error must not cause followers to re-upload the identical file"
    );
    assert_eq!(failed_snap.tasks.len(), 3);
    for task in &failed_snap.tasks {
        match &task.status {
            UploadTaskStatusDto::Failed { error } => {
                assert_eq!(error.code, ErrorCode::BookFileUnsupported.code());
            }
            other => panic!("expected Failed status, got {other:?}"),
        }
    }
}

#[tokio::test]
async fn android_retains_completed_results_clears_explicitly_and_drops_prior_succeeded_on_new_enqueue() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        android_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    let boot = runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();
    assert!(boot.can_import_books);

    let ok_1 = tmp.path().join("Retained Book 1.epub");
    let bad_1 = tmp.path().join("Retained Bad 1.epub");
    fs::write(&ok_1, sample_epub_bytes()).unwrap();
    fs::write(&bad_1, b"not valid epub").unwrap();

    runtime
        .upload_enqueue_paths(vec![ok_1, bad_1])
        .await
        .unwrap();

    let done = sink
        .wait_for_upload(Duration::from_secs(5), |s| {
            s.stats.remaining == 0 && s.stats.succeeded == 1 && s.stats.failed == 1
        })
        .await;
    assert!(done.visible);
    assert_eq!(done.tasks.len(), 2);

    // Wait past desktop's 1_000ms auto_dismiss_delay: Android must still retain both tasks.
    tokio::time::sleep(Duration::from_millis(1_150)).await;
    let still_retained = runtime.upload_snapshot();
    assert!(still_retained.visible);
    assert_eq!(still_retained.tasks.len(), 2);
    assert_eq!(still_retained.stats.succeeded, 1);
    assert_eq!(still_retained.stats.failed, 1);

    // Enqueuing a new batch after completion clears prior Succeeded tasks while keeping un-dismissed Failed tasks.
    let ok_2 = tmp.path().join("Retained Book 2.pdf");
    fs::write(&ok_2, sample_pdf_bytes()).unwrap();
    runtime.upload_enqueue_paths(vec![ok_2]).await.unwrap();

    let second_done = sink
        .wait_for_upload(Duration::from_secs(5), |s| {
            s.stats.remaining == 0 && s.completed_counter == 2
        })
        .await;
    assert!(second_done.visible);
    assert_eq!(second_done.stats.succeeded, 1);
    assert_eq!(second_done.stats.failed, 1);
    assert_eq!(second_done.tasks.len(), 2);
    assert!(second_done
        .tasks
        .iter()
        .any(|t| t.file_name == "Retained Book 2.pdf" && matches!(t.status, UploadTaskStatusDto::Succeeded { .. })));
    assert!(second_done
        .tasks
        .iter()
        .any(|t| t.file_name == "Retained Bad 1.epub" && matches!(t.status, UploadTaskStatusDto::Failed { .. })));

    // Explicit clear resets everything and hides the panel.
    let cleared = runtime.upload_clear_completed().await;
    assert!(!cleared.visible);
    assert!(cleared.tasks.is_empty());
    assert_eq!(cleared.stats, UploadSessionStatsDto::default());
}

#[tokio::test]
async fn upload_clear_completed_is_noop_while_active() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    let mut payloads = HashMap::new();
    payloads.insert("mem://gated.epub".to_owned(), sample_epub_bytes());
    let opener = Arc::new(BarrierOpener {
        barrier: Arc::new(Barrier::new(2)),
        gated_uris: vec!["mem://gated.epub".to_owned()],
        payloads,
        release_notify: None,
        opened_count: Arc::new(AtomicUsize::new(0)),
    });

    let runtime = ClientRuntime::initialize_with_source_opener(
        &config_dir,
        android_caps(lib_root),
        AppLocale::En,
        None,
        sink.clone(),
        opener.clone(),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap();

    runtime
        .upload_enqueue(vec![EnqueueUploadItem {
            file_name: "Gated.epub".to_owned(),
            total_bytes: None,
            locator: UploadSourceLocator::Uri("mem://gated.epub".to_owned()),
        }])
        .await
        .unwrap();

    let active_snap = sink
        .wait_for_upload(Duration::from_secs(3), |s| s.stats.remaining == 1)
        .await;
    assert!(active_snap.visible);

    // Calling upload_clear_completed while active must be a no-op.
    let unchanged = runtime.upload_clear_completed().await;
    assert!(unchanged.visible);
    assert_eq!(unchanged.stats.remaining, 1);

    opener.barrier.wait().await;
    sink.wait_for_upload(Duration::from_secs(5), |s| s.stats.remaining == 0 && s.stats.succeeded == 1)
        .await;
    let cleared = runtime.upload_clear_completed().await;
    assert!(!cleared.visible);
    assert!(cleared.tasks.is_empty());
}
