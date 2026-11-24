mod android_upload_notification;
mod commands;
mod cover;

use std::{
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use shiro_compatibility::{
    BootstrapStateDto, ClientLogLevel, ClientRuntime, ConfigStore, ErrorSink,
    OpenUploadSourceFuture, PlatformCapabilities, UploadPanelSnapshotDto, UploadSourceLocator,
    UploadSourceOpener,
};
use shiro_error_catalog::{AppLocale, ErrorCode, ErrorEventPayload};
use shiro_infrastructure::pdfium::load_bundled_pdfium;
use tauri::{AppHandle, Emitter, Manager, RunEvent};
use tauri_plugin_dialog::FilePath;

/// Tauri event name carrying Rust-localized client error notifications.
pub const ERROR_EVENT_NAME: &str = "shiro://error";

/// Tauri event name carrying asynchronous runtime state transitions (e.g. cloud session revocation).
pub const RUNTIME_STATE_EVENT_NAME: &str = "shiro://runtime-state";

/// Tauri event name carrying concurrent upload queue snapshots.
pub const UPLOAD_STATE_EVENT_NAME: &str = "shiro://upload-state";

/// Tauri event name emitted when the user taps the Android upload notification.
pub const UPLOAD_OPEN_UI_EVENT_NAME: &str = "shiro://upload-open-ui";

pub struct DesktopAppState {
    pub runtime: Arc<ClientRuntime>,
}

impl DesktopAppState {
    pub fn new(runtime: Arc<ClientRuntime>) -> Self {
        Self { runtime }
    }
}

struct TauriSourceOpener {
    app: AppHandle,
}

impl UploadSourceOpener for TauriSourceOpener {
    fn open_source(&self, locator: &UploadSourceLocator) -> OpenUploadSourceFuture {
        let app = self.app.clone();
        let locator = locator.clone();
        Box::pin(async move {
            let file_path = match locator {
                UploadSourceLocator::Path(path) => FilePath::Path(path),
                UploadSourceLocator::Uri(uri) => {
                    let parsed = url::Url::parse(&uri).map_err(|_| ErrorCode::ImportReadFailed)?;
                    FilePath::Url(parsed)
                }
            };
            tokio::task::spawn_blocking(move || {
                commands::open_book_source(&app, file_path)
                    .map(|(reader, size)| (Pin::from(reader), size))
                    .map_err(|_| ErrorCode::ImportReadFailed)
            })
            .await
            .map_err(|_| ErrorCode::ImportReadFailed)?
        })
    }
}

struct TauriErrorSink {
    app: AppHandle,
}

impl ErrorSink for TauriErrorSink {
    fn emit_error(&self, event: &ErrorEventPayload) {
        let _ = self.app.emit(ERROR_EVENT_NAME, event);
    }

    fn emit_runtime_state(&self, state: &BootstrapStateDto) {
        let _ = self.app.emit(RUNTIME_STATE_EVENT_NAME, state);
    }

    fn emit_upload_state(&self, snapshot: &UploadPanelSnapshotDto) {
        let _ = self.app.emit(UPLOAD_STATE_EVENT_NAME, snapshot);
        android_upload_notification::on_upload_snapshot(snapshot);
    }
}

fn detect_system_locale() -> AppLocale {
    tauri_plugin_os::locale()
        .as_deref()
        .map(AppLocale::detect_from_tag)
        .unwrap_or(AppLocale::En)
}

#[cfg(any(desktop, test))]
pub(crate) fn localized_app_title(locale: AppLocale) -> &'static str {
    match locale {
        AppLocale::Zh => "白读",
        AppLocale::En => "ShiroReader",
    }
}

pub(crate) fn sync_window_title(app: &AppHandle, locale: AppLocale) {
    #[cfg(desktop)]
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.set_title(localized_app_title(locale));
    }
    #[cfg(not(desktop))]
    let _ = (app, locale);
}

fn build_client_env_filter(
    capabilities: &PlatformCapabilities,
    configured_level: Option<ClientLogLevel>,
    rust_log: Option<&str>,
) -> tracing_subscriber::EnvFilter {
    if !capabilities.can_configure_log_level {
        return tracing_subscriber::EnvFilter::new(ClientLogLevel::compile_default().as_str());
    }

    if let Some(raw) = rust_log.map(str::trim).filter(|raw| !raw.is_empty())
        && let Ok(filter) = tracing_subscriber::EnvFilter::try_new(raw)
    {
        return filter;
    }

    let level = configured_level.unwrap_or_else(ClientLogLevel::compile_default);
    tracing_subscriber::EnvFilter::new(level.as_str())
}

fn init_client_logging(config_dir: &Path, capabilities: &PlatformCapabilities) {
    let (configured_level, rust_log) = if capabilities.can_configure_log_level {
        let configured = ConfigStore::new(config_dir)
            .load()
            .ok()
            .flatten()
            .and_then(|cfg| cfg.log_level);
        let env_filter = std::env::var(tracing_subscriber::EnvFilter::DEFAULT_ENV).ok();
        (configured, env_filter)
    } else {
        (None, None)
    };

    let filter = build_client_env_filter(capabilities, configured_level, rust_log.as_deref());
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_os::init())
        .register_asynchronous_uri_scheme_protocol("shiro-cover", |ctx, request, responder| {
            let runtime = Arc::clone(&ctx.app_handle().state::<DesktopAppState>().runtime);
            let path = request.uri().path().to_owned();
            tauri::async_runtime::spawn(async move {
                let response = cover::handle_cover_request(&runtime, &path).await;
                responder.respond(response);
            });
        })
        .setup(|app| {
            let handle = app.handle();

            // Keep client settings separate from library data on every platform.
            let config_dir = handle
                .path()
                .app_config_dir()
                .or_else(|_| handle.path().app_data_dir())
                .unwrap_or_else(|_| PathBuf::from("config"));

            let default_library_root = handle
                .path()
                .app_local_data_dir()
                .or_else(|_| handle.path().app_data_dir())
                .map(|dir| dir.join("library"))
                .unwrap_or_else(|_| PathBuf::from("library"));

            let capabilities = PlatformCapabilities::current_host(default_library_root);
            init_client_logging(&config_dir, &capabilities);

            let resource_dir = handle.path().resource_dir().ok();
            let pdfium = load_bundled_pdfium(resource_dir.as_deref());
            let detected_locale = detect_system_locale();
            let sink = Arc::new(TauriErrorSink {
                app: handle.clone(),
            });
            let source_opener = Arc::new(TauriSourceOpener {
                app: handle.clone(),
            });

            let runtime = tauri::async_runtime::block_on(
                ClientRuntime::initialize_with_source_opener(
                    config_dir,
                    capabilities,
                    detected_locale,
                    pdfium,
                    sink,
                    source_opener,
                ),
            );

            let initial_locale = runtime.bootstrap_state().locale;
            sync_window_title(handle, initial_locale);
            android_upload_notification::init(
                handle.clone(),
                Arc::downgrade(&runtime),
                initial_locale,
            );
            app.manage(DesktopAppState::new(runtime));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_bootstrap_state,
            commands::complete_initial_setup,
            commands::update_settings,
            commands::set_app_locale,
            commands::set_app_theme,
            commands::pick_library_dir,
            commands::repair_local_library_root,
            commands::retry_startup,
            commands::exit_app,
            commands::cloud_check_origin,
            commands::cloud_probe,
            commands::cloud_update_origin,
            commands::cloud_login,
            commands::cloud_logout,
            commands::books_list,
            commands::book_rename,
            commands::books_delete,
            commands::trash_list,
            commands::trash_restore,
            commands::trash_permanently_delete,
            commands::tags_list,
            commands::tag_define,
            commands::tag_rename,
            commands::tag_recolor,
            commands::tag_move,
            commands::tag_undefine,
            commands::book_tags_list,
            commands::book_tag_bind,
            commands::book_tag_unbind,
            commands::upload_get_state,
            commands::upload_pick_and_enqueue,
            commands::upload_enqueue_paths,
            commands::upload_retry_task,
            commands::upload_cancel_task,
            commands::upload_cancel_all,
            commands::upload_dismiss_task,
            commands::upload_dismiss_errors,
            commands::upload_set_concurrency,
            commands::upload_consume_pending_open_ui,
            commands::upload_set_mobile_ui_open,
        ])
        .build(tauri::generate_context!())
        .expect("error while building ShiroReader desktop client");

    app.run(|app_handle, event| {
        if let RunEvent::Exit = event {
            android_upload_notification::clear_immediately();
            let state = app_handle.state::<DesktopAppState>();
            tauri::async_runtime::block_on(state.runtime.shutdown());
        }
    });
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        io::Cursor,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use shiro_compatibility::{
        ClientLogLevel, ClientRuntime, InitialSetupRequest, LaunchMode, NoopErrorSink,
        PlatformCapabilities, StartupPhase,
    };
    use shiro_error_catalog::AppLocale;

    use crate::{build_client_env_filter, cover::handle_cover_request, localized_app_title};

    #[test]
    fn localized_app_title_matches_locale() {
        assert_eq!(localized_app_title(AppLocale::Zh), "白读");
        assert_eq!(localized_app_title(AppLocale::En), "ShiroReader");
    }

    #[tokio::test]
    async fn cover_protocol_handles_bad_paths_and_uninitialized_runtime_without_panicking() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ClientRuntime::initialize(
            tmp.path().join("config"),
            PlatformCapabilities::current_host(tmp.path().join("library")),
            AppLocale::En,
            None,
            Arc::new(NoopErrorSink),
        )
        .await;
        assert_eq!(runtime.bootstrap_state().phase, StartupPhase::SetupRequired);

        for bad_path in ["/", "/book/", "/book/a/b", "/other/123"] {
            let resp = handle_cover_request(&runtime, bad_path).await;
            assert_eq!(resp.status(), 404, "path={bad_path}");
        }

        let resp = handle_cover_request(&runtime, "/book/valid-id").await;
        assert_eq!(resp.status(), 503);
    }

    #[tokio::test]
    async fn cover_protocol_serves_imported_epub_cover_and_404_for_missing_book() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = ClientRuntime::initialize(
            tmp.path().join("config"),
            PlatformCapabilities::current_host(tmp.path().join("library")),
            AppLocale::En,
            None,
            Arc::new(NoopErrorSink),
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
            .expect("initial local setup");

        let epub_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/packaging/fixtures/smoke_cover.epub");
        let epub_bytes = fs::read(&epub_path).expect("read smoke_cover.epub");

        let book = runtime
            .import_stream(
                "op-cover-test",
                "Smoke EPUB.epub",
                Cursor::new(epub_bytes),
                None,
                None,
            )
            .await
            .expect("import epub");

        let resp = handle_cover_request(&runtime, &format!("/book/{}", book.id)).await;
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.headers().get("content-type").unwrap(), "image/png");
        assert_eq!(resp.headers().get("cache-control").unwrap(), "no-store");
        assert!(resp.body().starts_with(b"\x89PNG\r\n\x1a\n"));

        let missing =
            handle_cover_request(&runtime, "/book/00000000-0000-0000-0000-000000000000").await;
        assert_eq!(missing.status(), 404);
    }

    #[tokio::test]
    async fn cover_protocol_returns_401_when_cloud_session_is_missing() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind mock meta server");
        let addr = listener.local_addr().unwrap();

        let server_task = tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                let body = br#"{"service":"shiro-reader","api_version":1,"server_version":"0.0.1","max_upload_bytes":1024}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.write_all(body).await;
                let _ = socket.shutdown().await;
            }
        });

        let tmp = tempfile::tempdir().unwrap();
        let config_dir = tmp.path().join("config");
        fs::create_dir_all(&config_dir).unwrap();
        let origin = format!("http://127.0.0.1:{}", addr.port());
        fs::write(
            config_dir.join("settings.json"),
            format!(
                r#"{{"schemaVersion":1,"mode":"cloud","locale":"en","localLibraryRoot":null,"cloudOrigin":"{origin}"}}"#
            ),
        )
        .unwrap();

        let runtime = ClientRuntime::initialize(
            &config_dir,
            PlatformCapabilities::current_host(tmp.path().join("library")),
            AppLocale::En,
            None,
            Arc::new(NoopErrorSink),
        )
        .await;
        assert_eq!(
            runtime.bootstrap_state().phase,
            StartupPhase::CloudAuthRequired
        );

        let resp = handle_cover_request(&runtime, "/book/any-id").await;
        assert_eq!(resp.status(), 401);
        let _ = server_task.await;
    }

    #[test]
    fn client_env_filter_respects_platform_capabilities_and_precedence() {
        let desktop_caps = PlatformCapabilities {
            can_pick_library_dir: true,
            can_configure_log_level: true,
            can_import_books: true,
            retain_completed_uploads: false,
            default_library_root: PathBuf::from("library"),
            cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
        };
        let mobile_caps = PlatformCapabilities {
            can_pick_library_dir: false,
            can_configure_log_level: false,
            can_import_books: true,
            retain_completed_uploads: true,
            default_library_root: PathBuf::from("library"),
            cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
        };

        assert_eq!(
            build_client_env_filter(&desktop_caps, None, None).to_string(),
            ClientLogLevel::compile_default().as_str()
        );
        assert_eq!(
            build_client_env_filter(&desktop_caps, Some(ClientLogLevel::Warn), None).to_string(),
            "warn"
        );
        assert_eq!(
            build_client_env_filter(
                &desktop_caps,
                Some(ClientLogLevel::Warn),
                Some("shiro_reader_desktop_lib=trace"),
            )
            .to_string(),
            "shiro_reader_desktop_lib=trace"
        );
        assert_eq!(
            build_client_env_filter(&desktop_caps, Some(ClientLogLevel::Error), Some("   "))
                .to_string(),
            "error"
        );
        assert_eq!(
            build_client_env_filter(
                &desktop_caps,
                Some(ClientLogLevel::Error),
                Some("invalid[directive"),
            )
            .to_string(),
            "error"
        );

        assert_eq!(
            build_client_env_filter(
                &mobile_caps,
                Some(ClientLogLevel::Trace),
                Some("error"),
            )
            .to_string(),
            ClientLogLevel::compile_default().as_str()
        );
    }
}
