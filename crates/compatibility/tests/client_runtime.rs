use std::{
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use futures_util::TryStreamExt;
use shiro_compatibility::{
    AppTheme, ClientRuntime, DeleteTagStrategyDto, ErrorSink, InitialSetupRequest, LaunchMode,
    PlatformCapabilities, StartupPhase, TagValueType, UpdateSettingsRequest,
};
use shiro_error_catalog::{AppLocale, ErrorCode, ErrorEventPayload};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingSink {
    events: Mutex<Vec<ErrorEventPayload>>,
}

impl RecordingSink {
    fn events(&self) -> Vec<ErrorEventPayload> {
        self.events.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.events.lock().unwrap().clear();
    }
}

impl ErrorSink for RecordingSink {
    fn emit_error(&self, event: &ErrorEventPayload) {
        self.events.lock().unwrap().push(event.clone());
    }
}

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
        epub_cache_max_bytes: 24 * 1024 * 1024,
    }
}

#[tokio::test]
async fn first_launch_setup_mode_lock_settings_persistence_and_restart_isolation() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let default_root = tmp.path().join("default_lib");
    let custom_root = tmp.path().join("custom_windows_lib");
    let sink = Arc::new(RecordingSink::default());

    // 1. First launch: SetupRequired, no library DB or identity DB created.
    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root.clone()),
        AppLocale::Zh,
        None,
        sink.clone(),
    )
    .await;
    let boot = runtime.bootstrap_state();
    assert_eq!(boot.phase, StartupPhase::SetupRequired);
    assert_eq!(boot.active_mode, None);
    assert_eq!(boot.locale, AppLocale::Zh);
    assert!(!default_root.exists());
    assert!(!custom_root.exists());

    // Operations before setup fail with SetupRequired and emit a paired localized event.
    let err = runtime.list_books().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::SetupRequired.code());
    let events = sink.events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].operation_id, err.operation_id);
    assert_eq!(events[0].error_id, err.error_id);
    assert_eq!(events[0].locale, AppLocale::Zh);
    sink.clear();

    // 2. Complete initial setup in Local mode with custom desktop library root.
    let boot = runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::Zh,
            local_library_root: Some(custom_root.display().to_string()),
            cloud_origin: None,
        })
        .await
        .expect("complete setup");
    assert_eq!(boot.phase, StartupPhase::Ready);
    assert_eq!(boot.active_mode, Some(LaunchMode::Local));
    assert_eq!(boot.configured_mode, Some(LaunchMode::Local));
    assert!(!boot.pending_restart);
    assert_eq!(
        boot.configured_library_root,
        custom_root.to_string_lossy().as_ref()
    );
    assert!(custom_root.join("library.db").is_file());
    assert!(!custom_root.join("user.db").exists());
    assert!(config_dir.join("settings.json").is_file());

    // Attempting initial setup again must fail with LaunchModeLocked.
    let relock = runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Cloud,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: Some("https://books.example.com".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(relock.code, ErrorCode::LaunchModeLocked.code());

    // 3. Import a book in local mode, then change configured mode to Cloud (restart required).
    let epub_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_cover.epub"))
        .expect("smoke_cover.epub");
    let imported = runtime
        .import_stream(
            "op-import-1",
            "My Local Book.epub",
            Cursor::new(epub_bytes),
            None,
            None,
        )
        .await
        .expect("import epub");
    assert_eq!(imported.title, "My Local Book");
    assert_eq!(imported.file_type, "EPUB");

    // Switching to an unreachable cloud server must be rejected and must NOT overwrite settings
    // or mutate error_dispatcher locale.
    let unreachable_err = runtime
        .update_settings(UpdateSettingsRequest {
            mode: Some(LaunchMode::Cloud),
            locale: Some(AppLocale::En),
            local_library_root: None,
            cloud_origin: Some("http://127.0.0.1:1/".into()),
        })
        .await
        .unwrap_err();
    assert_eq!(
        unreachable_err.code,
        ErrorCode::CloudNetworkUnreachable.code()
    );
    assert_eq!(runtime.bootstrap_state().configured_mode, Some(LaunchMode::Local));
    assert_eq!(runtime.bootstrap_state().locale, AppLocale::Zh);
    assert_eq!(runtime.locale(), AppLocale::Zh);
    assert!(!runtime.bootstrap_state().pending_restart);

    // Local mode still operates normally in the current process.
    assert_eq!(runtime.list_books().await.unwrap().len(), 1);
    runtime.shutdown().await;
    drop(runtime);

    // 4. Simulate a previously configured cloud server going offline before next launch:
    // restarting enters CloudUnreachable without touching or wiping local data.
    fs::write(
        config_dir.join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "mode": "cloud",
            "locale": "en",
            "localLibraryRoot": custom_root,
            "cloudOrigin": "http://127.0.0.1:1"
        }))
        .unwrap(),
    )
    .unwrap();
    let cloud_runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root.clone()),
        AppLocale::Zh,
        None,
        sink.clone(),
    )
    .await;
    let cloud_boot = cloud_runtime.bootstrap_state();
    assert_eq!(cloud_boot.phase, StartupPhase::CloudUnreachable);
    assert_eq!(cloud_boot.active_mode, Some(LaunchMode::Cloud));
    assert_eq!(cloud_boot.locale, AppLocale::En);
    assert_eq!(
        cloud_boot.configured_library_root,
        custom_root.display().to_string()
    );
    assert_eq!(
        cloud_boot.cloud.as_ref().map(|c| c.origin.as_str()),
        Some("http://127.0.0.1:1")
    );

    // Switch back to Local mode and restart -> local books, covers & cloud_origin all preserved.
    let switched_back = cloud_runtime
        .update_settings(UpdateSettingsRequest {
            mode: Some(LaunchMode::Local),
            locale: None,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .expect("switch back to local");
    assert!(switched_back.pending_restart);
    drop(cloud_runtime);

    let restarted_local = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root),
        AppLocale::Zh,
        None,
        sink,
    )
    .await;
    let local_boot = restarted_local.bootstrap_state();
    assert_eq!(local_boot.phase, StartupPhase::Ready);
    assert_eq!(local_boot.active_mode, Some(LaunchMode::Local));
    assert_eq!(
        local_boot.cloud_origin.as_deref(),
        Some("http://127.0.0.1:1")
    );
    let books = restarted_local.list_books().await.unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].id, imported.id);
    let (mut stream, mime) = restarted_local
        .get_cover_stream(imported.id)
        .await
        .expect("cover stream");
    assert_eq!(mime, "image/png");
    let mut cover = Vec::new();
    while let Some(chunk) = stream.try_next().await.unwrap() {
        cover.extend_from_slice(&chunk);
    }
    assert!(cover.starts_with(b"\x89PNG\r\n\x1a\n"));
}

#[tokio::test]
async fn broken_library_path_and_corrupted_db_never_create_empty_library_and_can_be_repaired() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let default_root = tmp.path().join("default_lib");
    let corrupt_root = tmp.path().join("broken_lib");
    let repaired_root = tmp.path().join("repaired_lib");
    let sink = Arc::new(RecordingSink::default());

    // Pre-write a valid config pointing to a directory with a corrupted library.db.
    fs::create_dir_all(&corrupt_root).unwrap();
    let sentinel = b"CORRUPTED_BYTES_DO_NOT_OVERWRITE";
    fs::write(corrupt_root.join("library.db"), sentinel).unwrap();
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "mode": "local",
            "locale": "zh",
            "localLibraryRoot": corrupt_root,
            "cloudOrigin": null
        }))
        .unwrap(),
    )
    .unwrap();

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;

    let boot = runtime.bootstrap_state();
    assert_eq!(boot.phase, StartupPhase::StartupFailed);
    let diag = boot.startup_error.as_ref().expect("startup diagnostic");
    assert_eq!(diag.code, ErrorCode::LocalDatabaseCorrupted.code());
    assert!(diag.message.contains("损坏"));
    // Early startup error is preserved in bounded buffer for frontend replay.
    assert!(
        boot.buffered_errors
            .iter()
            .any(|e| e.error_id == diag.error_id)
    );
    // Must NOT silently overwrite corrupted DB or fall back to default_root!
    assert_eq!(fs::read(corrupt_root.join("library.db")).unwrap(), sentinel);
    assert!(!default_root.exists());

    // Switching language immediately re-localizes startup diagnostic.
    let en_boot = runtime.set_locale(AppLocale::En).expect("switch locale");
    assert!(
        en_boot
            .startup_error
            .as_ref()
            .unwrap()
            .message
            .contains("corrupted")
    );

    // Repairing to a valid folder opens the library cleanly and clears StartupFailed.
    let repaired = runtime
        .repair_local_library_root(repaired_root.clone())
        .await
        .expect("repair root");
    assert_eq!(repaired.phase, StartupPhase::Ready);
    assert!(repaired.startup_error.is_none());
    assert_eq!(
        repaired.configured_library_root,
        repaired_root.to_string_lossy().as_ref()
    );
    assert!(repaired_root.join("library.db").is_file());
    assert!(repaired.buffered_errors.is_empty());
    assert!(runtime.list_books().await.unwrap().is_empty());
}

#[tokio::test]
async fn android_uses_fixed_private_dir_and_rejects_custom_library_root() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let android_private = tmp.path().join("android_private_library");
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        android_caps(android_private.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;

    // Attempting to supply a custom local root on Android must return FolderPickerNotImplemented.
    let err = runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: Some(tmp.path().join("other").display().to_string()),
            cloud_origin: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::FolderPickerNotImplemented.code());

    // Valid setup with None uses fixed Android private directory.
    let boot = runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .expect("android local setup");
    assert_eq!(boot.phase, StartupPhase::Ready);
    assert!(!boot.can_pick_library_dir);
    assert_eq!(
        boot.configured_library_root,
        android_private.to_string_lossy().as_ref()
    );
    assert!(android_private.join("library.db").is_file());
}

#[tokio::test]
async fn bookshelf_trash_tags_streaming_import_cancellation_and_error_id_pairing() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        sink.clone(),
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

    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf");

    // 1. Streaming import with progress callback + dedup on second book with same file hash.
    let mut progress_bytes = 0u64;
    let book1 = runtime
        .import_stream(
            "op-1",
            "Pattern Book.pdf",
            Cursor::new(pdf_bytes.clone()),
            None,
            Some(&mut |bytes| progress_bytes = bytes),
        )
        .await
        .expect("import pdf 1");
    assert_eq!(progress_bytes, pdf_bytes.len() as u64);

    // Same file bytes under a different title deduplicates cleanly with ZERO error events!
    sink.clear();
    let book2 = runtime
        .import_stream(
            "op-2",
            "Pattern Book Copy.pdf",
            Cursor::new(pdf_bytes.clone()),
            None,
            None,
        )
        .await
        .expect("import pdf 2 dedup");
    assert_eq!(book2.hash, book1.hash);
    assert!(sink.events().is_empty(), "dedup must not emit error events");

    // 2. Fake extension (.pdf with plain text bytes) -> BookFileUnsupported + temp cleanup.
    let bad = runtime
        .import_stream(
            "op-bad",
            "fake.pdf",
            Cursor::new(b"not a real pdf file".to_vec()),
            None,
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(bad.code, ErrorCode::BookFileUnsupported.code());
    assert_eq!(bad.operation_id, "op-bad");
    assert_eq!(bad.item_id.as_deref(), Some("fake.pdf"));
    let ev = sink.events().pop().expect("emitted unsupported event");
    assert_eq!(ev.error_id, bad.error_id);
    assert_eq!(ev.operation_id, "op-bad");

    // 3. Cancellation returns ImportCancelled WITHOUT emitting an error toast event.
    sink.clear();
    let cancel_flag = CancellationToken::new();
    cancel_flag.cancel();
    let cancelled = runtime
        .import_stream(
            "op-cancel",
            "cancelled.pdf",
            Cursor::new(pdf_bytes),
            Some(&cancel_flag),
            None,
        )
        .await
        .unwrap_err();
    assert_eq!(cancelled.code, ErrorCode::ImportCancelled.code());
    assert!(
        sink.events().is_empty(),
        "cancellation must not emit error event"
    );

    // This source stays open without producing bytes, like a stalled provider pipe.
    let (reader, _stalled_writer) = tokio::io::duplex(64);
    let cancel = CancellationToken::new();
    let mut import = std::pin::pin!(runtime.import_stream(
        "op-stalled",
        "stalled.pdf",
        reader,
        Some(&cancel),
        None,
    ));
    assert!(futures_util::poll!(&mut import).is_pending());
    cancel.cancel();
    let error = tokio::time::timeout(std::time::Duration::from_secs(1), import)
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::ImportCancelled.code());
    assert!(sink.events().is_empty());

    // Temp dir must have no leftover temp files.
    let temp_entries = fs::read_dir(lib_root.join("books/.temp")).unwrap().count();
    assert_eq!(temp_entries, 0, "temp files must be cleaned up");

    // 4. Tags CRUD & book binding.
    let root_tag = runtime
        .define_tag("Science".into(), None, "#336699".into(), TagValueType::None)
        .await
        .expect("define root tag");
    assert_eq!(root_tag.value_type, TagValueType::None);
    assert_eq!(root_tag.value_regex, None);

    let child_tag = runtime
        .define_tag(
            "Physics".into(),
            Some(root_tag.id.clone()),
            "#224466".into(),
            TagValueType::Number,
        )
        .await
        .expect("define child tag");
    assert_eq!(child_tag.value_type, TagValueType::Number);
    assert_eq!(
        child_tag.value_regex.as_deref(),
        TagValueType::Number.default_regex()
    );

    let invalid_bind = runtime
        .bind_book_tag(
            book1.id.clone(),
            child_tag.id.clone(),
            Some("not-a-number".into()),
        )
        .await
        .unwrap_err();
    assert_eq!(invalid_bind.code, ErrorCode::TagValueRegexMismatch.code());

    runtime
        .bind_book_tag(book1.id.clone(), child_tag.id.clone(), Some("101".into()))
        .await
        .expect("bind tag");
    assert_eq!(
        runtime.get_book_tags(book1.id.clone()).await.unwrap().len(),
        1
    );
    runtime
        .rename_tag(child_tag.id.clone(), "Astrophysics".into())
        .await
        .unwrap();
    runtime
        .change_tag_color(child_tag.id.clone(), "#112233".into())
        .await
        .unwrap();
    runtime
        .undefine_tag(root_tag.id.clone(), DeleteTagStrategyDto::Promote)
        .await
        .unwrap();
    let tree = runtime.get_tag_tree().await.unwrap();
    assert!(
        tree.iter()
            .any(|n| n.tag.id == child_tag.id && n.tag.name == "Astrophysics")
    );

    // 5. Trash & item-by-item restore with paired error ID for failed item.
    runtime
        .rename_book(book1.id.clone(), "Renamed Pattern".into())
        .await
        .unwrap();
    runtime
        .delete_books(vec![book1.id.clone(), book2.id.clone()])
        .await
        .unwrap();
    assert_eq!(runtime.list_trash().await.unwrap().len(), 2);

    sink.clear();
    let missing_id = "00000000-0000-0000-0000-000000000000".to_owned();
    let restore = runtime
        .restore_books(vec![book1.id.clone(), missing_id.clone()])
        .await
        .expect("item-by-item restore");
    assert_eq!(restore.restored_ids, vec![book1.id.clone()]);
    assert_eq!(restore.failed.len(), 1);
    let item_err = restore.failed[0].error.as_ref().unwrap();
    assert_eq!(item_err.code, ErrorCode::BookNotFound.code());
    assert_eq!(item_err.operation_id, restore.operation_id);
    assert_eq!(item_err.item_id.as_deref(), Some(missing_id.as_str()));
    let emitted = sink.events();
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0].error_id, item_err.error_id);
    assert_eq!(emitted[0].item_id.as_deref(), Some(missing_id.as_str()));

    // Permanently delete book2; original file still exists for book1.
    runtime
        .permanently_delete_books(vec![book2.id.clone()])
        .await
        .unwrap();
    assert!(runtime.list_trash().await.unwrap().is_empty());
    assert_eq!(runtime.list_books().await.unwrap().len(), 1);
}

#[tokio::test]
async fn corrupted_config_and_retry_startup_recover_cleanly_without_silent_reset() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    let sink = Arc::new(RecordingSink::default());

    fs::create_dir_all(&config_dir).unwrap();
    let corrupt_json = b"{ not valid json !!!";
    fs::write(config_dir.join("settings.json"), corrupt_json).unwrap();

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        sink,
    )
    .await;

    let boot = runtime.bootstrap_state();
    assert_eq!(boot.phase, StartupPhase::StartupFailed);
    assert_eq!(
        boot.startup_error.as_ref().unwrap().code,
        ErrorCode::ConfigInvalid.code()
    );
    // Corrupted config file must NOT be overwritten on startup.
    assert_eq!(
        fs::read(config_dir.join("settings.json")).unwrap(),
        corrupt_json
    );
    assert!(!lib_root.exists());

    // Switching locale on StartupError screen must relocalize diagnostics WITHOUT overwriting
    // the corrupted settings.json on disk, and partial update_settings must be rejected.
    let zh_boot = runtime.set_locale(AppLocale::Zh).expect("switch locale");
    assert_eq!(zh_boot.locale, AppLocale::Zh);
    assert!(
        zh_boot
            .startup_error
            .as_ref()
            .unwrap()
            .message
            .contains("配置")
    );
    assert_eq!(
        fs::read(config_dir.join("settings.json")).unwrap(),
        corrupt_json
    );
    let update_err = runtime
        .update_settings(UpdateSettingsRequest {
            mode: Some(LaunchMode::Local),
            locale: None,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .unwrap_err();
    assert_eq!(update_err.code, ErrorCode::ConfigInvalid.code());
    assert_eq!(
        fs::read(config_dir.join("settings.json")).unwrap(),
        corrupt_json
    );

    // User fixes settings.json externally and calls retry_startup.
    fs::write(
        config_dir.join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "mode": "local",
            "locale": "zh",
            "localLibraryRoot": lib_root,
            "cloudOrigin": null
        }))
        .unwrap(),
    )
    .unwrap();

    let recovered = runtime.retry_startup().await.expect("retry startup");
    assert_eq!(recovered.phase, StartupPhase::Ready);
    assert_eq!(recovered.active_mode, Some(LaunchMode::Local));
    assert_eq!(recovered.locale, AppLocale::Zh);
    assert!(lib_root.join("library.db").is_file());
}

#[tokio::test]
async fn retry_startup_never_falls_through_to_local_when_cloud_origin_missing_or_mode_unset() {
    // Case 1: mode = cloud, cloudOrigin = null -> retry_startup must fail with CloudInvalidOrigin.
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");
    fs::create_dir_all(&config_dir).unwrap();
    fs::write(
        config_dir.join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "mode": "cloud",
            "locale": "en",
            "localLibraryRoot": null,
            "cloudOrigin": null
        }))
        .unwrap(),
    )
    .unwrap();

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
    )
    .await;
    assert_eq!(runtime.bootstrap_state().phase, StartupPhase::StartupFailed);

    let err = runtime.retry_startup().await.unwrap_err();
    assert_eq!(err.code, ErrorCode::CloudInvalidOrigin.code());
    assert_eq!(runtime.bootstrap_state().phase, StartupPhase::StartupFailed);
    assert!(!lib_root.exists());

    // Case 2: corrupted config repaired to mode = null -> retry_startup enters SetupRequired.
    let tmp2 = tempfile::tempdir().unwrap();
    let config_dir2 = tmp2.path().join("config");
    let lib_root2 = tmp2.path().join("lib");
    fs::create_dir_all(&config_dir2).unwrap();
    fs::write(config_dir2.join("settings.json"), b"{ bad json").unwrap();

    let runtime2 = ClientRuntime::initialize(
        &config_dir2,
        desktop_caps(lib_root2.clone()),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
    )
    .await;
    assert_eq!(
        runtime2.bootstrap_state().phase,
        StartupPhase::StartupFailed
    );

    fs::write(
        config_dir2.join("settings.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schemaVersion": 1,
            "mode": null,
            "locale": "en",
            "localLibraryRoot": null,
            "cloudOrigin": null
        }))
        .unwrap(),
    )
    .unwrap();

    let next = runtime2.retry_startup().await.expect("retry startup");
    assert_eq!(next.phase, StartupPhase::SetupRequired);
    assert_eq!(next.active_mode, None);
    assert!(!lib_root2.exists());
}

#[tokio::test]
async fn concurrent_complete_initial_setup_serializes_and_locks_second_caller() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("lib");

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
    )
    .await;

    let req1 = InitialSetupRequest {
        mode: LaunchMode::Local,
        locale: AppLocale::En,
        local_library_root: None,
        cloud_origin: None,
    };
    let req2 = req1.clone();
    let (r1, r2) = tokio::join!(
        runtime.complete_initial_setup(req1),
        runtime.complete_initial_setup(req2),
    );

    let (ok_count, locked_count) = [r1, r2].into_iter().fold((0, 0), |(ok, locked), res| match res {
        Ok(_) => (ok + 1, locked),
        Err(err) if err.code == ErrorCode::LaunchModeLocked.code() => (ok, locked + 1),
        Err(other) => panic!("unexpected error: {other:?}"),
    });
    assert_eq!(ok_count, 1);
    assert_eq!(locked_count, 1);
}

#[tokio::test]
async fn theme_selection_applies_before_setup_and_persists_across_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let default_root = tmp.path().join("default_lib");
    let sink = Arc::new(RecordingSink::default());

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root.clone()),
        AppLocale::En,
        None,
        sink.clone(),
    )
    .await;
    assert_eq!(runtime.bootstrap_state().theme, AppTheme::Default);

    // Before setup: applied in memory only, settings.json is not created yet.
    let boot = runtime.set_theme(AppTheme::Green).expect("set theme");
    assert_eq!(boot.theme, AppTheme::Green);
    assert!(!config_dir.join("settings.json").exists());

    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: None,
            cloud_origin: None,
        })
        .await
        .expect("complete setup");
    let boot = runtime.set_theme(AppTheme::Yellow).expect("set theme");
    assert_eq!(boot.theme, AppTheme::Yellow);
    drop(runtime);

    let restarted = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(default_root),
        AppLocale::En,
        None,
        sink,
    )
    .await;
    assert_eq!(restarted.bootstrap_state().theme, AppTheme::Yellow);
}

#[tokio::test]
async fn custom_regex_tags_resolve_to_text_while_preserving_regex_validation() {
    let tmp = tempfile::tempdir().unwrap();
    let config_dir = tmp.path().join("config");
    let lib_root = tmp.path().join("library");

    let (custom_tag_id, date_tag_id, plain_text_tag_id, none_tag_id) = {
        let local = shiro_infrastructure::library_runtime::LocalLibrary::open(&lib_root, None)
            .await
            .expect("open local library");

        let custom_tag = local
            .tag_service()
            .define_tag(
                shiro_application::library::tag::commands::CreateTagDefinitionCommand {
                    name: "Code".into(),
                    parent_id: None,
                    color: "#336699".into(),
                    allows_value: true,
                    value_regex: Some("[A-Z]{3}".into()),
                },
            )
            .await
            .expect("define custom regex tag");

        let date_tag = local
            .tag_service()
            .define_tag(
                shiro_application::library::tag::commands::CreateTagDefinitionCommand {
                    name: "Published".into(),
                    parent_id: None,
                    color: "#336699".into(),
                    allows_value: true,
                    value_regex: TagValueType::Date.default_regex().map(str::to_owned),
                },
            )
            .await
            .expect("define preset date tag");

        let plain_text_tag = local
            .tag_service()
            .define_tag(
                shiro_application::library::tag::commands::CreateTagDefinitionCommand {
                    name: "Note".into(),
                    parent_id: None,
                    color: "#336699".into(),
                    allows_value: true,
                    value_regex: None,
                },
            )
            .await
            .expect("define plain text tag");

        let none_tag = local
            .tag_service()
            .define_tag(
                shiro_application::library::tag::commands::CreateTagDefinitionCommand {
                    name: "Favorite".into(),
                    parent_id: None,
                    color: "#336699".into(),
                    allows_value: false,
                    value_regex: None,
                },
            )
            .await
            .expect("define none tag");

        (custom_tag.id, date_tag.id, plain_text_tag.id, none_tag.id)
    };

    let runtime = ClientRuntime::initialize(
        &config_dir,
        desktop_caps(lib_root.clone()),
        AppLocale::En,
        None,
        Arc::new(RecordingSink::default()),
    )
    .await;
    runtime
        .complete_initial_setup(InitialSetupRequest {
            mode: LaunchMode::Local,
            locale: AppLocale::En,
            local_library_root: Some(lib_root.display().to_string()),
            cloud_origin: None,
        })
        .await
        .expect("complete setup");

    let pdf_bytes = fs::read(workspace_root().join("scripts/packaging/fixtures/smoke_pattern.pdf"))
        .expect("smoke_pattern.pdf");
    let book = runtime
        .import_stream(
            "op-custom-regex",
            "CustomTag.pdf",
            Cursor::new(pdf_bytes),
            None,
            None,
        )
        .await
        .expect("import pdf");

    let tree = runtime.get_tag_tree().await.expect("get tag tree");
    let custom = tree.iter().find(|n| n.tag.id == custom_tag_id).unwrap();
    assert_eq!(custom.tag.value_type, TagValueType::Text);
    assert_eq!(custom.tag.value_regex.as_deref(), Some("[A-Z]{3}"));

    let date = tree.iter().find(|n| n.tag.id == date_tag_id).unwrap();
    assert_eq!(date.tag.value_type, TagValueType::Date);
    assert_eq!(
        date.tag.value_regex.as_deref(),
        Some("^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
    );

    let plain = tree.iter().find(|n| n.tag.id == plain_text_tag_id).unwrap();
    assert_eq!(plain.tag.value_type, TagValueType::Text);
    assert_eq!(plain.tag.value_regex, None);

    let none = tree.iter().find(|n| n.tag.id == none_tag_id).unwrap();
    assert_eq!(none.tag.value_type, TagValueType::None);
    assert_eq!(none.tag.value_regex, None);

    let invalid = runtime
        .bind_book_tag(book.id.clone(), custom_tag_id.clone(), Some("ab".into()))
        .await
        .unwrap_err();
    assert_eq!(invalid.code, ErrorCode::TagValueRegexMismatch.code());

    runtime
        .bind_book_tag(book.id.clone(), custom_tag_id, Some("ABC".into()))
        .await
        .expect("custom regex matching value should succeed");
}
