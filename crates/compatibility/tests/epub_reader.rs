use std::{
    collections::BTreeMap,
    fs,
    io::{Cursor, Write as _},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use axum::{
    Router,
    extract::Request,
    http::Method,
    middleware::{self, Next},
};
use flate2::{Compression, Crc, write::DeflateEncoder};
use futures_util::future::join_all;
use shiro_compatibility::{
    BootstrapStateDto, ClientRuntime, ErrorSink, InitialSetupRequest, LaunchMode,
    PlatformCapabilities, StartupPhase,
};
use shiro_error_catalog::{AppLocale, ErrorCode, ErrorEventPayload};
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

#[derive(Default)]
struct RecordingSink {
    errors: Mutex<Vec<ErrorEventPayload>>,
    states: Mutex<Vec<BootstrapStateDto>>,
}

impl ErrorSink for RecordingSink {
    fn emit_error(&self, event: &ErrorEventPayload) {
        self.errors.lock().unwrap().push(event.clone());
    }

    fn emit_runtime_state(&self, state: &BootstrapStateDto) {
        self.states.lock().unwrap().push(state.clone());
    }
}

impl RecordingSink {
    fn error_count(&self) -> usize {
        self.errors.lock().unwrap().len()
    }

    fn clear_errors(&self) {
        self.errors.lock().unwrap().clear();
    }
}

struct RunningHttpServer {
    origin: String,
    shutdown_tx: Option<oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl RunningHttpServer {
    async fn start(router: Router) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
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
            origin,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    async fn stop(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.await;
        }
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

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

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
            jwt_secret: "test-epub-reader-jwt-secret-key-0123456789".into(),
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

/// Builds a valid multi-chunk EPUB (> 320 KiB) with stored `mimetype` + deflated container/OPF/chapters.
fn build_multi_chunk_epub() -> (Vec<u8>, BTreeMap<String, Vec<u8>>) {
    let container_xml = br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#
        .to_vec();
    let content_opf = br#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">urn:uuid:multi-chunk-test</dc:identifier>
    <dc:title>Multi Chunk EPUB</dc:title>
    <dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="ch1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="blob" href="large.bin" media-type="application/octet-stream"/>
  </manifest>
  <spine><itemref idref="ch1"/></spine>
</package>"#
        .to_vec();
    let chapter1 = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Chunked</p></body></html>"#.to_vec();
    let large_blob: Vec<u8> = (0..(340 * 1024))
        .map(|i| ((i * 73 + 19) % 251) as u8)
        .collect();

    let mut files = BTreeMap::new();
    files.insert("mimetype".to_owned(), b"application/epub+zip".to_vec());
    files.insert("META-INF/container.xml".to_owned(), container_xml);
    files.insert("OEBPS/content.opf".to_owned(), content_opf);
    files.insert("OEBPS/chapter1.xhtml".to_owned(), chapter1);
    files.insert("OEBPS/large.bin".to_owned(), large_blob);

    // Order `mimetype` first (stored), remaining entries deflated/stored so total ZIP > 340 KiB.
    let ordered_keys = [
        ("mimetype", false),
        ("META-INF/container.xml", true),
        ("OEBPS/content.opf", true),
        ("OEBPS/chapter1.xhtml", true),
        ("OEBPS/large.bin", false),
    ];

    let mut out = Vec::new();
    let mut cd = Vec::new();
    for (name, use_deflate) in ordered_keys {
        let raw = &files[name];
        let mut crc = Crc::new();
        crc.update(raw);
        let crc32 = crc.sum();

        let (method, payload) = if use_deflate {
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::default());
            enc.write_all(raw).unwrap();
            (8u16, enc.finish().unwrap())
        } else {
            (0u16, raw.clone())
        };

        let lh_offset = out.len() as u32;
        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0800u16.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc32.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&payload);

        cd.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0x0800u16.to_le_bytes());
        cd.extend_from_slice(&method.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&crc32.to_le_bytes());
        cd.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&lh_offset.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
    }

    let cd_offset = out.len() as u32;
    let cd_size = cd.len() as u32;
    out.extend_from_slice(&cd);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(ordered_keys.len() as u16).to_le_bytes());
    out.extend_from_slice(&(ordered_keys.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());

    (out, files)
}

#[tokio::test]
async fn local_epub_reader_reads_entries_blocks_pdf_evicts_deleted_and_enforces_lru_session_cap() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_dir = tmp.path().join("library");
    let sink = Arc::new(RecordingSink::default());

    let rt = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(lib_dir),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    rt.complete_initial_setup(InitialSetupRequest {
        mode: LaunchMode::Local,
        locale: AppLocale::En,
        local_library_root: None,
        cloud_origin: None,
    })
    .await
    .unwrap();

    let epub_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub");
    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf");
    let (multi_epub_bytes, multi_expected) = build_multi_chunk_epub();

    let epub_book = rt
        .import_stream("op-1", "Smoke.epub", Cursor::new(epub_bytes.clone()), None, None)
        .await
        .unwrap();
    let pdf_book = rt
        .import_stream("op-2", "Pattern.pdf", Cursor::new(pdf_bytes), None, None)
        .await
        .unwrap();
    let multi_book = rt
        .import_stream(
            "op-3",
            "MultiChunk.epub",
            Cursor::new(multi_epub_bytes),
            None,
            None,
        )
        .await
        .unwrap();

    // 1. Open smoke_cover.epub and concurrently read every entry.
    let opened = rt.open_epub(epub_book.id.clone()).await.unwrap();
    assert_eq!(opened.book_id, epub_book.id);
    assert_eq!(opened.file_size, epub_bytes.len() as u64);
    assert!(!opened.entries.is_empty());

    let entry_reads = join_all(opened.entries.iter().map(|(path, expected_len)| {
        let rt = &rt;
        let session_id = opened.session_id;
        let path = path.clone();
        let expected_len = *expected_len;
        async move {
            let bytes = rt.read_epub_entry(session_id, &path).await.unwrap();
            assert_eq!(bytes.len() as u64, expected_len, "entry length for {path}");
            (path, bytes)
        }
    }))
    .await;
    let map: BTreeMap<String, _> = entry_reads.into_iter().collect();
    assert_eq!(map["mimetype"].as_ref(), b"application/epub+zip");
    assert!(
        std::str::from_utf8(&map["META-INF/container.xml"])
            .unwrap()
            .contains("rootfile")
    );

    // Also verify multi-chunk EPUB matches every original entry byte-for-byte.
    let opened_multi = rt.open_epub(multi_book.id.clone()).await.unwrap();
    for (path, expected) in &multi_expected {
        let got = rt
            .read_epub_entry(opened_multi.session_id, path)
            .await
            .unwrap();
        assert_eq!(got.as_ref(), expected.as_slice(), "mismatch for {path}");
    }

    // 2. Non-EPUB (PDF) is rejected with ReaderUnsupportedFormat.
    sink.clear_errors();
    let pdf_err = rt.open_epub(pdf_book.id.clone()).await.unwrap_err();
    assert_eq!(pdf_err.code, ErrorCode::ReaderUnsupportedFormat.code());
    assert_eq!(sink.error_count(), 1);

    // 3. Missing entry error is command-only (no global error event emitted).
    sink.clear_errors();
    let miss_err = rt
        .read_epub_entry(opened.session_id, "missing/chapter.xhtml")
        .await
        .unwrap_err();
    assert_eq!(miss_err.code, ErrorCode::ReaderEntryNotFound.code());
    assert_eq!(sink.error_count(), 0);

    // 4. Deleting epub_book (recycle bin) evicts active session and blocks re-open.
    rt.delete_books(vec![epub_book.id.clone()]).await.unwrap();
    let evicted_err = rt
        .read_epub_entry(opened.session_id, "mimetype")
        .await
        .unwrap_err();
    assert_eq!(evicted_err.code, ErrorCode::ReaderSessionNotFound.code());
    let reopen_deleted_err = rt.open_epub(epub_book.id.clone()).await.unwrap_err();
    assert_eq!(reopen_deleted_err.code, ErrorCode::BookAlreadyDeleted.code());

    // Also verify permanently_delete_books evicts any remaining active session after trash/restore check.
    let epub_again = rt
        .import_stream("op-4", "SmokePerm.epub", Cursor::new(epub_bytes), None, None)
        .await
        .unwrap();
    let perm_opened = rt.open_epub(epub_again.id.clone()).await.unwrap();
    rt.delete_books(vec![epub_again.id.clone()]).await.unwrap();
    rt.permanently_delete_books(vec![epub_again.id]).await.unwrap();
    assert_eq!(
        rt.read_epub_entry(perm_opened.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );

    // 5. close_epub is idempotent.
    rt.close_epub(opened_multi.session_id).await.unwrap();
    rt.close_epub(opened_multi.session_id).await.unwrap();
    assert_eq!(
        rt.read_epub_entry(opened_multi.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );

    // 6. Opening > MAX_OPEN_SESSIONS (8) evicts the least-recently-accessed session.
    let mut sessions = Vec::new();
    for _ in 0..8 {
        sessions.push(rt.open_epub(multi_book.id.clone()).await.unwrap().session_id);
    }
    // Touch sessions[0] so sessions[1] becomes least recently accessed.
    rt.read_epub_entry(sessions[0], "mimetype").await.unwrap();
    let ninth = rt.open_epub(multi_book.id.clone()).await.unwrap().session_id;

    assert!(rt.read_epub_entry(sessions[0], "mimetype").await.is_ok());
    assert_eq!(
        rt.read_epub_entry(sessions[1], "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );
    assert!(rt.read_epub_entry(ninth, "mimetype").await.is_ok());

    rt.shutdown().await;
}

#[tokio::test]
async fn cloud_epub_reader_streams_ranges_reuses_chunk_cache_avoids_error_storms_and_clears_lifecycle()
{
    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "CloudReader".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();

    let file_head_count = Arc::new(AtomicUsize::new(0));
    let file_get_count = Arc::new(AtomicUsize::new(0));
    let head_ctr = Arc::clone(&file_head_count);
    let get_ctr = Arc::clone(&file_get_count);
    let router = build_router(state.clone()).layer(middleware::from_fn(move |req: Request, next: Next| {
        let head_ctr = Arc::clone(&head_ctr);
        let get_ctr = Arc::clone(&get_ctr);
        async move {
            if req.uri().path().starts_with("/api/v1/library/bookshelf/books/")
                && req.uri().path().ends_with("/file")
            {
                if req.method() == Method::HEAD {
                    head_ctr.fetch_add(1, Ordering::SeqCst);
                } else if req.method() == Method::GET {
                    get_ctr.fetch_add(1, Ordering::SeqCst);
                }
            }
            next.run(req).await
        }
    }));
    let server = RunningHttpServer::start(router).await;

    let app_root = tempfile::tempdir().unwrap();
    let config_dir = app_root.path().join("config");
    let lib_dir = app_root.path().join("library");
    let sink = Arc::new(RecordingSink::default());

    let rt = ClientRuntime::initialize(
        &config_dir,
        PlatformCapabilities::current_host(lib_dir),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    rt.complete_initial_setup(InitialSetupRequest {
        mode: LaunchMode::Cloud,
        locale: AppLocale::En,
        local_library_root: None,
        cloud_origin: Some(server.origin.clone()),
    })
    .await
    .unwrap();
    let boot = rt
        .cloud_login("CloudReader".into(), "password123".into())
        .await
        .unwrap();
    assert_eq!(boot.phase, StartupPhase::Ready);

    let smoke_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub");
    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf");
    let (multi_bytes, multi_expected) = build_multi_chunk_epub();

    let smoke_book = rt
        .import_stream("c-1", "SmokeCloud.epub", Cursor::new(smoke_bytes.clone()), None, None)
        .await
        .unwrap();
    let pdf_book = rt
        .import_stream("c-2", "PatternCloud.pdf", Cursor::new(pdf_bytes), None, None)
        .await
        .unwrap();
    let multi_book = rt
        .import_stream("c-3", "MultiCloud.epub", Cursor::new(multi_bytes), None, None)
        .await
        .unwrap();

    // 1. Open smoke_cover.epub (< 256 KiB): HEAD once + 1 chunk GET covers open AND all entries!
    file_head_count.store(0, Ordering::SeqCst);
    file_get_count.store(0, Ordering::SeqCst);
    let opened_smoke = rt.open_epub(smoke_book.id.clone()).await.unwrap();
    assert_eq!(opened_smoke.file_size, smoke_bytes.len() as u64);
    assert_eq!(file_head_count.load(Ordering::SeqCst), 1);

    for (path, expected_len) in &opened_smoke.entries {
        let got = rt
            .read_epub_entry(opened_smoke.session_id, path)
            .await
            .unwrap();
        assert_eq!(got.len() as u64, *expected_len);
    }
    assert_eq!(
        file_get_count.load(Ordering::SeqCst),
        1,
        "all entries in a <256 KiB EPUB must be served from the single cached chunk"
    );

    // 2. Open and read multi-chunk (> 340 KiB) EPUB over Cloud Range 206 -> exact byte match.
    let opened_multi = rt.open_epub(multi_book.id.clone()).await.unwrap();
    for (path, expected) in &multi_expected {
        let got = rt
            .read_epub_entry(opened_multi.session_id, path)
            .await
            .unwrap();
        assert_eq!(got.as_ref(), expected.as_slice(), "cloud mismatch for {path}");
    }

    // 3. PDF blocked with ReaderUnsupportedFormat.
    let pdf_err = rt.open_epub(pdf_book.id.clone()).await.unwrap_err();
    assert_eq!(pdf_err.code, ErrorCode::ReaderUnsupportedFormat.code());

    // 4. Missing entry in read_epub_entry -> CommandErrorPayload with ZERO global ErrorSink events.
    sink.clear_errors();
    let miss_err = rt
        .read_epub_entry(opened_smoke.session_id, "does/not/exist.xhtml")
        .await
        .unwrap_err();
    assert_eq!(miss_err.code, ErrorCode::ReaderEntryNotFound.code());
    assert_eq!(sink.error_count(), 0);

    // 5. close_epub is idempotent.
    rt.close_epub(opened_smoke.session_id).await.unwrap();
    rt.close_epub(opened_smoke.session_id).await.unwrap();

    // 6. delete_books evicts cloud session; logout clears remaining sessions and chunk cache.
    let opened_smoke2 = rt.open_epub(smoke_book.id.clone()).await.unwrap();
    rt.delete_books(vec![smoke_book.id.clone()]).await.unwrap();
    assert_eq!(
        rt.read_epub_entry(opened_smoke2.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );

    let multi_uncached_session = rt.open_epub(multi_book.id.clone()).await.unwrap();
    rt.cloud_logout().await.unwrap();
    assert_eq!(
        rt.read_epub_entry(multi_uncached_session.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );

    // 7. Re-login, open session, revoke session server-side -> handle_cloud_session_invalidated clears sessions.
    rt.cloud_login("CloudReader".into(), "password123".into())
        .await
        .unwrap();
    let invalidated_session = rt.open_epub(multi_book.id.clone()).await.unwrap();
    let cred_store = shiro_compatibility::EncryptedFileCredentialStore::new(
        config_dir.join("keys"),
        app_root.path().join("cloud-credentials"),
    );
    let saved_refresh = shiro_compatibility::CredentialStore::load(&cred_store, &server.origin)
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
    assert_eq!(
        rt.list_books().await.unwrap_err().code,
        ErrorCode::AuthenticationInvalidToken.code()
    );
    assert_eq!(
        rt.read_epub_entry(invalidated_session.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );

    // 8. cloud_update_origin succeeds to a valid server and clears state cleanly.
    let second_server_root = tempfile::tempdir().unwrap();
    let second_state = build_real_v1_server_state(second_server_root.path()).await;
    let second_server = RunningHttpServer::start(build_router(second_state)).await;
    let updated = rt
        .cloud_update_origin(second_server.origin.clone())
        .await
        .unwrap();
    assert_eq!(updated.phase, StartupPhase::CloudAuthRequired);
    second_server.stop().await;

    // 9. Point back to primary server, re-login, open multi-chunk EPUB, stop server, and verify
    //    network error in read_epub_entry produces zero global error events, then shutdown clears session.
    rt.cloud_update_origin(server.origin.clone()).await.unwrap();
    rt.cloud_login("CloudReader".into(), "password123".into())
        .await
        .unwrap();
    let multi_session_before_drop = rt.open_epub(multi_book.id.clone()).await.unwrap();
    server.stop().await;
    sink.clear_errors();
    let net_err = rt
        .read_epub_entry(multi_session_before_drop.session_id, "mimetype")
            .await
            .unwrap_err();
    assert_eq!(net_err.code, ErrorCode::CloudNetworkUnreachable.code());
    assert_eq!(
        sink.error_count(),
        0,
        "read_epub_entry network failure must not emit global error toast events"
    );

    rt.shutdown().await;
    assert_eq!(
        rt.read_epub_entry(multi_session_before_drop.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );
}

fn build_unique_small_epub(label: &str) -> Vec<u8> {
    let chapter = format!(
        "<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>{label}</p></body></html>"
    )
    .into_bytes();
    let entries: [(&str, &[u8]); 2] = [
        ("mimetype", b"application/epub+zip"),
        ("OEBPS/chapter1.xhtml", &chapter),
    ];

    let mut out = Vec::new();
    let mut cd = Vec::new();
    for (name, raw) in entries {
        let mut crc = Crc::new();
        crc.update(raw);
        let crc32 = crc.sum();
        let lh_offset = out.len() as u32;

        out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]);
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&0x0800u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&crc32.to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(raw);

        cd.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]);
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&20u16.to_le_bytes());
        cd.extend_from_slice(&0x0800u16.to_le_bytes());
        cd.extend_from_slice(&0u16.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&crc32.to_le_bytes());
        cd.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(raw.len() as u32).to_le_bytes());
        cd.extend_from_slice(&(name.len() as u16).to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&0u32.to_le_bytes());
        cd.extend_from_slice(&lh_offset.to_le_bytes());
        cd.extend_from_slice(name.as_bytes());
    }

    let cd_offset = out.len() as u32;
    let cd_size = cd.len() as u32;
    out.extend_from_slice(&cd);
    out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&cd_size.to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[tokio::test]
async fn open_epub_rejects_zombie_session_on_concurrent_delete_and_logout_but_allows_unrelated_delete()
{
    struct GateHandle {
        entered: oneshot::Sender<()>,
        release: oneshot::Receiver<()>,
    }

    let server_root = tempfile::tempdir().unwrap();
    let state = build_real_v1_server_state(server_root.path()).await;
    state
        .user_service
        .register(UserRegisterCommand {
            name: "RaceReader".into(),
            password: "password123".into(),
        })
        .await
        .unwrap();

    let gate: Arc<tokio::sync::Mutex<Option<GateHandle>>> = Arc::new(tokio::sync::Mutex::new(None));
    let gate_mw = Arc::clone(&gate);
    let router = build_router(state).layer(middleware::from_fn(move |req: Request, next: Next| {
        let gate_mw = Arc::clone(&gate_mw);
        async move {
            let is_file_get = req.method() == Method::GET
                && req.uri().path().starts_with("/api/v1/library/bookshelf/books/")
                && req.uri().path().ends_with("/file");
            let resp = next.run(req).await;
            if is_file_get {
                let active_gate = gate_mw.lock().await.take();
                if let Some(handle) = active_gate {
                    let _ = handle.entered.send(());
                    let _ = handle.release.await;
                }
            }
            resp
        }
    }));
    let server = RunningHttpServer::start(router).await;

    let app_root = tempfile::tempdir().unwrap();
    let sink = Arc::new(RecordingSink::default());
    let rt = Arc::new(
        ClientRuntime::initialize(
            &app_root.path().join("config"),
            PlatformCapabilities::current_host(app_root.path().join("library")),
            AppLocale::En,
            None,
            sink,
        )
        .await,
    );
    rt.complete_initial_setup(InitialSetupRequest {
        mode: LaunchMode::Cloud,
        locale: AppLocale::En,
        local_library_root: None,
        cloud_origin: Some(server.origin.clone()),
    })
    .await
    .unwrap();
    rt.cloud_login("RaceReader".into(), "password123".into())
        .await
        .unwrap();

    let book_a = rt
        .import_stream(
            "r-1",
            "BookA.epub",
            Cursor::new(build_unique_small_epub("book-a")),
            None,
            None,
        )
        .await
        .unwrap();
    let book_b = rt
        .import_stream(
            "r-2",
            "BookB.epub",
            Cursor::new(build_unique_small_epub("book-b")),
            None,
            None,
        )
        .await
        .unwrap();
    let book_c = rt
        .import_stream(
            "r-3",
            "BookC.epub",
            Cursor::new(build_unique_small_epub("book-c")),
            None,
            None,
        )
        .await
        .unwrap();
    let book_d = rt
        .import_stream(
            "r-4",
            "BookD.epub",
            Cursor::new(build_unique_small_epub("book-d")),
            None,
            None,
        )
        .await
        .unwrap();

    // 1. Concurrent delete of the same book while ZipIndex::parse is mid-flight -> rejected, no zombie session.
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    *gate.lock().await = Some(GateHandle {
        entered: entered_tx,
        release: release_rx,
    });
    let open_a = {
        let rt = Arc::clone(&rt);
        let id = book_a.id.clone();
        tokio::spawn(async move { rt.open_epub(id).await })
    };
    entered_rx.await.unwrap();
    rt.delete_books(vec![book_a.id.clone()]).await.unwrap();
    let _ = release_tx.send(());
    let err_a = open_a.await.unwrap().unwrap_err();
    assert_eq!(err_a.code, ErrorCode::BookAlreadyDeleted.code());

    // 2. Concurrent delete of an unrelated book while ZipIndex::parse is mid-flight -> retries and succeeds.
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    *gate.lock().await = Some(GateHandle {
        entered: entered_tx,
        release: release_rx,
    });
    let open_b = {
        let rt = Arc::clone(&rt);
        let id = book_b.id.clone();
        tokio::spawn(async move { rt.open_epub(id).await })
    };
    entered_rx.await.unwrap();
    rt.delete_books(vec![book_c.id.clone()]).await.unwrap();
    let _ = release_tx.send(());
    let opened_b = open_b.await.unwrap().expect("unrelated delete must allow retry");
    assert_eq!(
        rt.read_epub_entry(opened_b.session_id, "mimetype")
            .await
            .unwrap()
            .as_ref(),
        b"application/epub+zip"
    );

    // 3. Concurrent cloud_logout while ZipIndex::parse is mid-flight -> rejected with CloudNotAuthenticated.
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    *gate.lock().await = Some(GateHandle {
        entered: entered_tx,
        release: release_rx,
    });
    let open_d = {
        let rt = Arc::clone(&rt);
        let id = book_d.id.clone();
        tokio::spawn(async move { rt.open_epub(id).await })
    };
    entered_rx.await.unwrap();
    rt.cloud_logout().await.unwrap();
    let _ = release_tx.send(());
    let err_d = open_d.await.unwrap().unwrap_err();
    assert_eq!(err_d.code, ErrorCode::CloudNotAuthenticated.code());
    assert_eq!(
        rt.read_epub_entry(opened_b.session_id, "mimetype")
            .await
            .unwrap_err()
            .code,
        ErrorCode::ReaderSessionNotFound.code()
    );
}
