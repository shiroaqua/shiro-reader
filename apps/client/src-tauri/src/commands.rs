use std::{io, path::PathBuf};

use shiro_compatibility::{
    AppTheme, BookNavigationOutput, BookTagOutput, BootstrapStateDto,
    CloudOriginCheckResult, DeleteTagStrategyDto, EnqueueUploadItem, EpubSessionId, GetBookOutput,
    InitialSetupRequest, NavigationLocatorDto, OpenEpubResponseDto, ReaderProfileDto,
    ReaderProfilePatchDto, ReaderSettingsSnapshotDto, RestoreBooksResult, SystemFontFamilyDto,
    TagDefinitionDto, TagTreeNodeDto, TagValueType, TrashedBookOutput, UpdateSettingsRequest,
    UploadPanelSnapshotDto, UploadSourceLocator,
};
use shiro_error_catalog::{AppLocale, CommandErrorPayload, ErrorCode};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};
use tokio::io::AsyncRead;
use uuid::Uuid;

use crate::{
    ClientAppState, android_upload_notification,
    reader_windows::{ReaderWindowSnapshotDto, TabStateUpdate},
};

// -----------------------------------------------------------------------------
// Lifecycle, Settings & Platform Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn get_bootstrap_state(app: AppHandle, state: State<'_, ClientAppState>) -> BootstrapStateDto {
    let snapshot = state.runtime.bootstrap_state();
    sync_android_status_bar(&app, snapshot.theme);
    snapshot
}

#[tauri::command]
pub async fn complete_initial_setup(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    request: InitialSetupRequest,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.complete_initial_setup(request).await?;
    crate::sync_window_title(&app, snapshot.locale);
    android_upload_notification::on_locale_changed(
        snapshot.locale,
        &state.runtime.upload_snapshot(),
    );
    Ok(snapshot)
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    request: UpdateSettingsRequest,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.update_settings(request).await?;
    if snapshot.pending_restart {
        state
            .reader_windows
            .close_all_windows_async(&app, &state.runtime)
            .await;
    }
    crate::sync_window_title(&app, snapshot.locale);
    android_upload_notification::on_locale_changed(
        snapshot.locale,
        &state.runtime.upload_snapshot(),
    );
    Ok(snapshot)
}

#[tauri::command]
pub fn set_app_locale(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    locale: AppLocale,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.set_locale(locale)?;
    crate::sync_window_title(&app, snapshot.locale);
    android_upload_notification::on_locale_changed(
        snapshot.locale,
        &state.runtime.upload_snapshot(),
    );
    Ok(snapshot)
}

#[tauri::command]
pub fn set_app_theme(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    theme: AppTheme,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.set_theme(theme)?;
    sync_android_status_bar(&app, snapshot.theme);
    Ok(snapshot)
}

#[cfg(target_os = "android")]
fn sync_android_status_bar(app: &AppHandle, theme: AppTheme) {
    use jni::objects::JValue;

    let Some(webview) = app.get_webview_window("main") else {
        return;
    };
    // Keep these values aligned with --theme-system-bar-color in index.css.
    let color = match theme {
        AppTheme::Default => "#fafafa",
        AppTheme::Pink => "#fff4fe",
        AppTheme::Green => "#effff2",
        AppTheme::Blue => "#e8ffff",
        AppTheme::Yellow => "#fff8e9",
    }
    .to_owned();

    if let Err(error) = webview.with_webview(move |webview| {
        webview.jni_handle().exec(move |env, activity, _| {
            let result = (|| {
                let window = env
                    .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                    .l()?;
                let java_color = env.new_string(color)?;
                let color = env
                    .call_static_method(
                        "android/graphics/Color",
                        "parseColor",
                        "(Ljava/lang/String;)I",
                        &[JValue::Object(&java_color)],
                    )?
                    .i()?;
                env.call_method(&window, "setStatusBarColor", "(I)V", &[JValue::Int(color)])?;
                let decor_view = env
                    .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                    .l()?;
                env.call_method(
                    &decor_view,
                    "setBackgroundColor",
                    "(I)V",
                    &[JValue::Int(color)],
                )?;
                Ok::<(), jni::errors::Error>(())
            })();

            if let Err(error) = result {
                tracing::warn!(?error, "failed to synchronize Android status bar color");
            }
        });
    }) {
        tracing::warn!(
            ?error,
            "failed to access Android WebView for status bar color"
        );
    }
}

#[cfg(not(target_os = "android"))]
fn sync_android_status_bar(_app: &AppHandle, _theme: AppTheme) {}

#[tauri::command]
pub async fn pick_library_dir(
    app: AppHandle,
    state: State<'_, ClientAppState>,
) -> Result<Option<String>, CommandErrorPayload> {
    if !state.runtime.capabilities().can_pick_library_dir {
        return Err(state
            .runtime
            .error_dispatcher()
            .dispatch_operation(ErrorCode::FolderPickerNotImplemented));
    }

    #[cfg(desktop)]
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        app.dialog().file().pick_folder(move |folder| {
            let _ = tx.send(folder);
        });
        let picked = rx.await.ok().flatten();
        Ok(picked.map(|fp| match fp {
            FilePath::Path(path) => path.display().to_string(),
            FilePath::Url(url) => url.to_string(),
        }))
    }

    #[cfg(not(desktop))]
    {
        let _ = app;
        Err(state
            .runtime
            .error_dispatcher()
            .dispatch_operation(ErrorCode::FolderPickerNotImplemented))
    }
}

#[tauri::command]
pub async fn repair_local_library_root(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    path: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state
        .runtime
        .repair_local_library_root(PathBuf::from(path))
        .await?;
    state
        .reader_windows
        .close_all_windows_async(&app, &state.runtime)
        .await;
    Ok(snapshot)
}

#[tauri::command]
pub async fn retry_startup(
    state: State<'_, ClientAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.retry_startup().await
}

#[tauri::command]
pub fn exit_app(app: AppHandle, state: State<'_, ClientAppState>) {
    state
        .reader_windows
        .close_all_windows_sync(&app, &state.runtime);
    app.exit(0);
}

#[tauri::command]
pub fn trigger_long_press_haptic(app: AppHandle) {
    perform_android_long_press_haptic(&app);
}

#[cfg(target_os = "android")]
fn perform_android_long_press_haptic(app: &AppHandle) {
    use jni::objects::JValue;

    // android.view.HapticFeedbackConstants.LONG_PRESS = 0
    // android.view.HapticFeedbackConstants.FLAG_IGNORE_VIEW_SETTING = 1
    const HAPTIC_FEEDBACK_LONG_PRESS: jni::sys::jint = 0;
    const HAPTIC_FEEDBACK_FLAG_IGNORE_VIEW_SETTING: jni::sys::jint = 1;

    let Some(webview) = app.get_webview_window("main") else {
        return;
    };

    if let Err(error) = webview.with_webview(move |webview| {
        webview.jni_handle().exec(move |env, activity, _| {
            let result = (|| {
                let window = env
                    .call_method(activity, "getWindow", "()Landroid/view/Window;", &[])?
                    .l()?;
                if window.is_null() {
                    return Ok::<(), jni::errors::Error>(());
                }
                let decor_view = env
                    .call_method(&window, "getDecorView", "()Landroid/view/View;", &[])?
                    .l()?;
                if decor_view.is_null() {
                    return Ok::<(), jni::errors::Error>(());
                }
                let _ = env
                    .call_method(
                        &decor_view,
                        "performHapticFeedback",
                        "(II)Z",
                        &[
                            JValue::Int(HAPTIC_FEEDBACK_LONG_PRESS),
                            JValue::Int(HAPTIC_FEEDBACK_FLAG_IGNORE_VIEW_SETTING),
                        ],
                    )?
                    .z()?;
                Ok::<(), jni::errors::Error>(())
            })();

            if let Err(error) = result {
                tracing::warn!(?error, "failed to trigger Android long-press haptic feedback");
                let _ = env.exception_clear();
            }
        });
    }) {
        tracing::warn!(
            ?error,
            "failed to access Android WebView for long-press haptic feedback"
        );
    }
}

#[cfg(not(target_os = "android"))]
fn perform_android_long_press_haptic(_app: &AppHandle) {}

// -----------------------------------------------------------------------------
// Cloud Mode Commands (Probe, Origin Recovery, Auth)
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn cloud_check_origin(
    state: State<'_, ClientAppState>,
    origin: String,
) -> Result<CloudOriginCheckResult, CommandErrorPayload> {
    Ok(state.runtime.check_cloud_origin(origin).await)
}

#[tauri::command]
pub async fn cloud_probe(
    state: State<'_, ClientAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_probe().await
}

#[tauri::command]
pub async fn cloud_update_origin(
    state: State<'_, ClientAppState>,
    origin: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_update_origin(origin).await
}

#[tauri::command]
pub async fn cloud_login(
    state: State<'_, ClientAppState>,
    username: String,
    password: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_login(username, password).await
}

#[tauri::command]
pub async fn cloud_logout(
    app: AppHandle,
    state: State<'_, ClientAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.cloud_logout().await?;
    state
        .reader_windows
        .close_all_windows_async(&app, &state.runtime)
        .await;
    Ok(snapshot)
}

// -----------------------------------------------------------------------------
// Bookshelf & Trash Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn books_list(
    state: State<'_, ClientAppState>,
) -> Result<Vec<GetBookOutput>, CommandErrorPayload> {
    state.runtime.list_books().await
}

#[tauri::command]
pub async fn book_rename(
    state: State<'_, ClientAppState>,
    id: String,
    title: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.rename_book(id, title).await
}

#[tauri::command]
pub async fn books_delete(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    ids: Vec<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.delete_books(ids.clone()).await?;
    state
        .reader_windows
        .evict_books(&app, &state.runtime, &ids)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn trash_list(
    state: State<'_, ClientAppState>,
) -> Result<Vec<TrashedBookOutput>, CommandErrorPayload> {
    state.runtime.list_trash().await
}

#[tauri::command]
pub async fn trash_restore(
    state: State<'_, ClientAppState>,
    ids: Vec<String>,
) -> Result<RestoreBooksResult, CommandErrorPayload> {
    state.runtime.restore_books(ids).await
}

#[tauri::command]
pub async fn trash_permanently_delete(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    ids: Vec<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.permanently_delete_books(ids.clone()).await?;
    state
        .reader_windows
        .evict_books(&app, &state.runtime, &ids)
        .await;
    Ok(())
}

#[tauri::command]
pub async fn book_navigation(
    state: State<'_, ClientAppState>,
    id: String,
) -> Result<BookNavigationOutput, CommandErrorPayload> {
    state.runtime.get_book_navigation(id).await
}

// -----------------------------------------------------------------------------
// EPUB Reader Binary IPC Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn epub_open(
    state: State<'_, ClientAppState>,
    book_id: String,
) -> Result<OpenEpubResponseDto, CommandErrorPayload> {
    state.runtime.open_epub(book_id).await
}

#[tauri::command]
pub async fn epub_attach_or_open(
    state: State<'_, ClientAppState>,
    book_id: String,
    session_id: Option<EpubSessionId>,
) -> Result<OpenEpubResponseDto, CommandErrorPayload> {
    state.runtime.attach_or_open_epub(book_id, session_id).await
}

#[tauri::command]
pub async fn epub_read_entry(
    state: State<'_, ClientAppState>,
    session_id: EpubSessionId,
    path: String,
) -> Result<tauri::ipc::Response, CommandErrorPayload> {
    let bytes = state.runtime.read_epub_entry(session_id, &path).await?;
    Ok(tauri::ipc::Response::new(Vec::from(bytes)))
}

#[tauri::command]
pub async fn epub_close(
    state: State<'_, ClientAppState>,
    session_id: EpubSessionId,
) -> Result<(), CommandErrorPayload> {
    state.runtime.close_epub(session_id).await
}

// -----------------------------------------------------------------------------
// Pure-Local Reader Settings & Custom Font Commands
// -----------------------------------------------------------------------------

fn emit_reader_settings_snapshot(app: &AppHandle, snapshot: &ReaderSettingsSnapshotDto) {
    let _ = app.emit(crate::READER_SETTINGS_EVENT_NAME, snapshot);
}

#[tauri::command]
pub fn reader_get_settings(state: State<'_, ClientAppState>) -> ReaderSettingsSnapshotDto {
    state.runtime.get_reader_settings()
}

#[tauri::command]
pub fn reader_update_global_settings(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    settings: ReaderProfileDto,
) -> Result<ReaderSettingsSnapshotDto, CommandErrorPayload> {
    let snapshot = state.runtime.update_reader_global_settings(settings)?;
    emit_reader_settings_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn reader_set_book_override(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    book_hash: String,
    patch: ReaderProfilePatchDto,
) -> Result<ReaderSettingsSnapshotDto, CommandErrorPayload> {
    let snapshot = state
        .runtime
        .set_reader_book_override(book_hash, patch)?;
    emit_reader_settings_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn reader_reset_book_override(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    book_hash: String,
) -> Result<ReaderSettingsSnapshotDto, CommandErrorPayload> {
    let snapshot = state.runtime.reset_reader_book_override(book_hash)?;
    emit_reader_settings_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn reader_list_system_fonts(
    state: State<'_, ClientAppState>,
) -> Result<Vec<SystemFontFamilyDto>, CommandErrorPayload> {
    Ok(state.runtime.list_system_fonts().await)
}

async fn pick_font_file(app: &AppHandle) -> Option<FilePath> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file();

    // Avoid restrictive extension filters on Android SAF where OEM pickers may gray out .ttf/.otf.
    #[cfg(not(target_os = "android"))]
    let dialog = dialog.add_filter("Fonts (TTF, OTF)", &["ttf", "otf"]);

    dialog.pick_file(move |file| {
        let _ = tx.send(file);
    });
    rx.await.ok().flatten()
}

#[tauri::command]
pub async fn reader_pick_and_import_font(
    app: AppHandle,
    state: State<'_, ClientAppState>,
) -> Result<ReaderSettingsSnapshotDto, CommandErrorPayload> {
    let Some(file_path) = pick_font_file(&app).await else {
        return Ok(state.runtime.get_reader_settings());
    };

    let file_name = extract_file_name(&file_path);
    let app_for_open = app.clone();
    let (reader, expected_size) = tokio::task::spawn_blocking(move || {
        open_book_source(&app_for_open, file_path)
    })
    .await
    .map_err(|_| {
        state
            .runtime
            .error_dispatcher()
            .dispatch_operation(ErrorCode::ImportReadFailed)
    })?
    .map_err(|_| {
        state
            .runtime
            .error_dispatcher()
            .dispatch_operation(ErrorCode::ImportReadFailed)
    })?;

    let snapshot = state
        .runtime
        .import_custom_font_stream(file_name, reader, expected_size)
        .await?;
    emit_reader_settings_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub fn reader_delete_custom_font(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    font_id: String,
) -> Result<ReaderSettingsSnapshotDto, CommandErrorPayload> {
    let snapshot = state.runtime.delete_custom_font(font_id)?;
    emit_reader_settings_snapshot(&app, &snapshot);
    Ok(snapshot)
}

#[tauri::command]
pub async fn reader_read_custom_font(
    state: State<'_, ClientAppState>,
    font_id: String,
) -> Result<tauri::ipc::Response, CommandErrorPayload> {
    let (bytes, _format) = state
        .runtime
        .read_custom_font(&font_id)
        .await
        .map_err(|code| state.runtime.error_dispatcher().command_only(code))?;
    Ok(tauri::ipc::Response::new(bytes))
}

// -----------------------------------------------------------------------------
// Desktop Multi-Window Reader Coordinator Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn reader_open_book(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    book_id: String,
    book_hash: Option<String>,
    title: Option<String>,
    format: Option<String>,
    new_window: bool,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state
        .reader_windows
        .open_book(
            &app,
            &state.runtime,
            book_id,
            book_hash,
            title,
            format,
            new_window,
        )
        .await
}

#[tauri::command]
pub fn reader_get_window_snapshot(
    state: State<'_, ClientAppState>,
    window_id: String,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state
        .reader_windows
        .get_window_snapshot(&state.runtime, &window_id)
}

#[tauri::command]
pub fn reader_activate_tab(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
    tab_id: String,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state
        .reader_windows
        .activate_tab(&app, &state.runtime, &window_id, &tab_id)
}

#[tauri::command]
pub fn reader_reorder_tabs(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
    tab_ids: Vec<String>,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state
        .reader_windows
        .reorder_tabs(&app, &state.runtime, &window_id, &tab_ids)
}

#[tauri::command]
pub async fn reader_close_tab(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
    tab_id: String,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state
        .reader_windows
        .close_tab(&app, &state.runtime, &window_id, &tab_id)
        .await
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn reader_update_tab_state(
    state: State<'_, ClientAppState>,
    window_id: String,
    tab_id: String,
    session_id: Option<EpubSessionId>,
    current_locator: Option<NavigationLocatorDto>,
    fraction: Option<f64>,
    translation_mode: Option<String>,
    target_language: Option<String>,
) {
    state.reader_windows.update_tab_state(
        &state.runtime,
        &window_id,
        &tab_id,
        TabStateUpdate {
            session_id,
            current_locator,
            fraction,
            translation_mode,
            target_language,
        },
    );
}

#[tauri::command]
pub fn reader_report_tab_strip_metrics(
    state: State<'_, ClientAppState>,
    window_id: String,
    height_px: f64,
    drag_width_px: f64,
) {
    state
        .reader_windows
        .report_tab_strip_metrics(&window_id, height_px, drag_width_px);
}

#[tauri::command]
pub fn reader_move_tab_to_window(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    source_window_id: String,
    tab_id: String,
    target_window_id: String,
    target_index: Option<usize>,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state.reader_windows.move_tab_to_window(
        &app,
        &state.runtime,
        &source_window_id,
        &tab_id,
        &target_window_id,
        target_index,
    )
}

#[tauri::command]
pub async fn reader_tear_off_tab(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    source_window_id: String,
    tab_id: String,
    screen_x: Option<f64>,
    screen_y: Option<f64>,
) -> Result<ReaderWindowSnapshotDto, CommandErrorPayload> {
    state.reader_windows.tear_off_tab(
        &app,
        &state.runtime,
        &source_window_id,
        &tab_id,
        screen_x,
        screen_y,
    )
}

#[tauri::command]
pub fn reader_drag_start(
    state: State<'_, ClientAppState>,
    window_id: String,
    tab_id: String,
) {
    state.reader_windows.drag_start(&window_id, &tab_id);
}

#[tauri::command]
pub fn reader_drag_over(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
    client_x: f64,
    client_y: f64,
) {
    state
        .reader_windows
        .drag_over(&app, &window_id, client_x, client_y);
}

#[tauri::command]
pub async fn reader_drag_commit(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
    client_x: f64,
    client_y: f64,
) -> Result<(), CommandErrorPayload> {
    state
        .reader_windows
        .drag_commit(&app, &state.runtime, &window_id, client_x, client_y)
}

#[tauri::command]
pub fn reader_drag_cancel(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    window_id: String,
) {
    state.reader_windows.drag_cancel(&app, &window_id);
}

// -----------------------------------------------------------------------------
// Tag Tree & Book-Tag Binding Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn tags_list(
    state: State<'_, ClientAppState>,
) -> Result<Vec<TagTreeNodeDto>, CommandErrorPayload> {
    state.runtime.get_tag_tree().await
}

#[tauri::command]
pub async fn tag_define(
    state: State<'_, ClientAppState>,
    name: String,
    parent_id: Option<String>,
    color: String,
    value_type: TagValueType,
) -> Result<TagDefinitionDto, CommandErrorPayload> {
    state
        .runtime
        .define_tag(name, parent_id, color, value_type)
        .await
}

#[tauri::command]
pub async fn tag_rename(
    state: State<'_, ClientAppState>,
    id: String,
    name: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.rename_tag(id, name).await
}

#[tauri::command]
pub async fn tag_recolor(
    state: State<'_, ClientAppState>,
    id: String,
    color: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.change_tag_color(id, color).await
}

#[tauri::command]
pub async fn tag_move(
    state: State<'_, ClientAppState>,
    id: String,
    parent_id: Option<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.move_tag(id, parent_id).await
}

#[tauri::command]
pub async fn tag_undefine(
    state: State<'_, ClientAppState>,
    id: String,
    strategy: DeleteTagStrategyDto,
) -> Result<(), CommandErrorPayload> {
    state.runtime.undefine_tag(id, strategy).await
}

#[tauri::command]
pub async fn book_tags_list(
    state: State<'_, ClientAppState>,
    book_id: String,
) -> Result<Vec<BookTagOutput>, CommandErrorPayload> {
    state.runtime.get_book_tags(book_id).await
}

#[tauri::command]
pub async fn book_tag_bind(
    state: State<'_, ClientAppState>,
    book_id: String,
    tag_id: String,
    value: Option<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.bind_book_tag(book_id, tag_id, value).await
}

#[tauri::command]
pub async fn book_tag_unbind(
    state: State<'_, ClientAppState>,
    book_id: String,
    tag_id: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.unbind_book_tag(book_id, tag_id).await
}

// -----------------------------------------------------------------------------
// Concurrent Upload Queue Commands (Local & Cloud, Desktop Paths + SAF URIs)
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn upload_get_state(state: State<'_, ClientAppState>) -> UploadPanelSnapshotDto {
    state.runtime.upload_snapshot()
}

#[tauri::command]
pub async fn upload_pick_and_enqueue(
    app: AppHandle,
    state: State<'_, ClientAppState>,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    if !state.runtime.capabilities().can_import_books {
        return Err(state
            .runtime
            .error_dispatcher()
            .dispatch_operation(ErrorCode::ImportNotSupportedOnPlatform));
    }

    let operation_id = Uuid::new_v4().to_string();
    state.runtime.ensure_library_ready(&operation_id)?;

    android_upload_notification::ensure_permission(&app)
        .await
        .map_err(|code| state.runtime.error_dispatcher().dispatch_operation(code))?;

    let Some(selected_files) = pick_book_files(&app).await.filter(|list| !list.is_empty()) else {
        return Ok(state.runtime.upload_snapshot());
    };

    let items: Vec<EnqueueUploadItem> = selected_files
        .into_iter()
        .map(|file_path| {
            let file_name = extract_file_name(&file_path);
            match file_path {
                FilePath::Path(path) => {
                    let total_bytes = std::fs::metadata(&path).ok().map(|meta| meta.len());
                    EnqueueUploadItem {
                        file_name,
                        total_bytes,
                        locator: UploadSourceLocator::Path(path),
                    }
                }
                FilePath::Url(url) => EnqueueUploadItem {
                    file_name,
                    total_bytes: None,
                    locator: UploadSourceLocator::Uri(url.to_string()),
                },
            }
        })
        .collect();

    state.runtime.upload_enqueue(items).await
}

#[tauri::command]
pub async fn upload_enqueue_paths(
    state: State<'_, ClientAppState>,
    paths: Vec<String>,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    let path_bufs = paths.into_iter().map(PathBuf::from).collect();
    state.runtime.upload_enqueue_paths(path_bufs).await
}

#[tauri::command]
pub async fn upload_retry_task(
    app: AppHandle,
    state: State<'_, ClientAppState>,
    task_id: u64,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    android_upload_notification::ensure_permission(&app)
        .await
        .map_err(|code| state.runtime.error_dispatcher().dispatch_operation(code))?;
    state.runtime.upload_retry_task(task_id).await
}

#[tauri::command]
pub async fn upload_cancel_task(
    state: State<'_, ClientAppState>,
    task_id: u64,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    Ok(state.runtime.upload_cancel_task(task_id).await)
}

#[tauri::command]
pub async fn upload_cancel_all(
    state: State<'_, ClientAppState>,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    Ok(state.runtime.upload_cancel_all().await)
}

#[tauri::command]
pub async fn upload_dismiss_task(
    state: State<'_, ClientAppState>,
    task_id: u64,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    Ok(state.runtime.upload_dismiss_task(task_id).await)
}

#[tauri::command]
pub async fn upload_dismiss_errors(
    state: State<'_, ClientAppState>,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    Ok(state.runtime.upload_dismiss_failed().await)
}

#[tauri::command]
pub async fn upload_set_concurrency(
    state: State<'_, ClientAppState>,
    concurrency: usize,
) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
    state.runtime.set_upload_concurrency(concurrency).await
}

#[tauri::command]
pub fn upload_consume_pending_open_ui() -> bool {
    android_upload_notification::consume_pending_open_ui()
}

#[tauri::command]
pub async fn upload_set_mobile_ui_open(
    state: State<'_, ClientAppState>,
    open: bool,
) -> Result<(), CommandErrorPayload> {
    if android_upload_notification::set_mobile_ui_open(open) {
        state.runtime.upload_clear_completed().await;
    }
    Ok(())
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch,
        '\u{061C}' | '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}'
    )
}

fn has_unsafe_external_url_char(value: &str) -> bool {
    value.chars().any(|ch| {
        ch.is_control()
            || ch.is_whitespace()
            || is_bidi_control(ch)
            || matches!(ch, '"' | '<' | '>' | '`' | '\\')
    })
}

pub(crate) fn validate_external_url(raw_url: &str) -> Result<url::Url, &'static str> {
    const MAX_EXTERNAL_URL_LEN: usize = 4096;
    let trimmed = raw_url.trim();
    if trimmed.is_empty()
        || trimmed.len() > MAX_EXTERNAL_URL_LEN
        || trimmed
            .chars()
            .any(|ch| ch.is_control() || is_bidi_control(ch))
    {
        return Err("external URL is empty, too long, or contains control characters");
    }
    let parsed = url::Url::parse(trimmed).map_err(|_| "invalid external URL")?;
    if parsed.as_str().len() > MAX_EXTERNAL_URL_LEN || has_unsafe_external_url_char(parsed.as_str())
    {
        return Err("external URL contains unsafe characters");
    }
    match parsed.scheme() {
        "http" | "https" => {
            if !parsed.has_host() {
                return Err("HTTP(S) external URL must include a host");
            }
            Ok(parsed)
        }
        "mailto" => {
            if parsed.path().trim().is_empty() || has_unsafe_external_url_char(trimmed) {
                return Err("mailto external URL must include a valid recipient");
            }
            Ok(parsed)
        }
        _ => Err("unsupported external URL scheme"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
fn spawn_and_check_external_launcher(mut cmd: std::process::Command) -> io::Result<()> {
    let mut child = cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Always wait on the child in a background thread so no <defunct> zombie persists.
        let _ = tx.send(child.wait());
    });

    match rx.recv_timeout(std::time::Duration::from_millis(1500)) {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => Err(io::Error::other(format!(
            "external URL launcher exited with status {status}"
        ))),
        Ok(Err(err)) => Err(err),
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(()),
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(io::Error::other(
            "external URL launcher monitor exited unexpectedly",
        )),
    }
}

#[cfg(target_os = "windows")]
fn open_external_url_windows(url: &str) -> io::Result<()> {
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt, ptr};

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn ShellExecuteW(
            hwnd: *mut std::ffi::c_void,
            lp_operation: *const u16,
            lp_file: *const u16,
            lp_parameters: *const u16,
            lp_directory: *const u16,
            n_show_cmd: i32,
        ) -> isize;
    }

    let operation: Vec<u16> = OsStr::new("open").encode_wide().chain(Some(0)).collect();
    let target: Vec<u16> = OsStr::new(url).encode_wide().chain(Some(0)).collect();
    // SW_SHOWNORMAL = 1; ShellExecuteW returns > 32 on success.
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            ptr::null(),
            ptr::null(),
            1,
        )
    };
    if result > 32 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "ShellExecuteW failed with code {result}"
        )))
    }
}

#[tauri::command]
pub async fn reader_open_external_url(
    state: State<'_, ClientAppState>,
    url: String,
) -> Result<(), CommandErrorPayload> {
    let parsed = validate_external_url(&url).map_err(|_| {
        state
            .runtime
            .error_dispatcher()
            .command_only(ErrorCode::CommonInternalServerError)
    })?;
    let target = parsed.as_str().to_owned();

    let launch_result = tauri::async_runtime::spawn_blocking(move || -> io::Result<()> {
        #[cfg(target_os = "linux")]
        {
            let mut cmd = std::process::Command::new("xdg-open");
            cmd.arg(&target);
            spawn_and_check_external_launcher(cmd)
        }

        #[cfg(target_os = "windows")]
        {
            open_external_url_windows(&target)
        }

        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new("open");
            cmd.arg(&target);
            spawn_and_check_external_launcher(cmd)
        }

        #[cfg(target_os = "android")]
        {
            let _ = target;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Android uses ShiroReaderAndroidBridge.openExternalUrl",
            ))
        }

        #[cfg(not(any(
            target_os = "linux",
            target_os = "windows",
            target_os = "macos",
            target_os = "android"
        )))]
        {
            let _ = target;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "external browser launch unsupported on this target",
            ))
        }
    })
    .await;

    match launch_result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(state
            .runtime
            .error_dispatcher()
            .command_only(ErrorCode::CommonInternalServerError)),
    }
}

pub(crate) type OpenedBookSource = (Box<dyn AsyncRead + Unpin + Send>, Option<u64>);

pub(crate) fn open_book_source(
    app: &AppHandle,
    file_path: FilePath,
) -> io::Result<OpenedBookSource> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = app.fs().open(file_path, options)?;
    let metadata = file.metadata()?;

    #[cfg(target_os = "android")]
    {
        use std::os::unix::fs::FileTypeExt;

        if metadata.file_type().is_fifo() {
            let pipe = tokio::net::unix::pipe::Receiver::from_file(file)?;
            return Ok((Box::new(pipe), None));
        }
    }

    let bytes = Some(metadata.len()).filter(|&len| len > 0);
    Ok((Box::new(tokio::fs::File::from_std(file)), bytes))
}

async fn pick_book_files(app: &AppHandle) -> Option<Vec<FilePath>> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let dialog = app.dialog().file();

    // Avoid restrictive extension filters on Android SAF where OEM pickers may disable .epub.
    #[cfg(not(target_os = "android"))]
    let dialog = dialog.add_filter("E-Books (PDF, EPUB)", &["pdf", "epub"]);

    dialog.pick_files(move |files| {
        let _ = tx.send(files);
    });
    rx.await.ok().flatten()
}

pub(crate) fn extract_file_name(file_path: &FilePath) -> String {
    match file_path {
        FilePath::Path(path) => path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "book".to_owned()),
        FilePath::Url(url) => extract_file_name_from_uri(url.as_str()),
    }
}

pub(crate) fn extract_file_name_from_uri(raw_uri: &str) -> String {
    if let Some((_, query_and_frag)) = raw_uri.split_once('?') {
        let query = query_and_frag.split('#').next().unwrap_or(query_and_frag);
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=')
                && matches!(
                    key.to_ascii_lowercase().as_str(),
                    "displayname" | "display_name" | "filename" | "name"
                )
            {
                let decoded = percent_decode_utf8(value);
                if let Some(candidate) = decoded
                    .rsplit(['/', '\\', ':'])
                    .find(|part| !part.trim().is_empty())
                {
                    return candidate.to_owned();
                }
            }
        }
    }

    let without_query = raw_uri.split(['?', '#']).next().unwrap_or(raw_uri);
    let last_segment = without_query.rsplit('/').next().unwrap_or(without_query);
    let decoded = percent_decode_utf8(last_segment);
    decoded
        .rsplit(['/', '\\', ':'])
        .find(|part| !part.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "book".to_owned())
}

fn percent_decode_utf8(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_file_name_from_uri, validate_external_url};

    #[test]
    fn validates_external_url_schemes_and_rejects_unsafe_inputs() {
        assert!(validate_external_url("https://example.com/docs?q=1#top").is_ok());
        assert!(validate_external_url("http://example.org").is_ok());
        assert!(validate_external_url("mailto:reader@example.com").is_ok());
        assert!(
            validate_external_url("mailto:reader@example.com?subject=Hello%20World").is_ok()
        );

        assert!(validate_external_url("").is_err());
        assert!(validate_external_url("javascript:alert(1)").is_err());
        assert!(validate_external_url("file:///etc/passwd").is_err());
        assert!(validate_external_url("data:text/html,<script>1</script>").is_err());
        assert!(validate_external_url("https://").is_err());
        assert!(validate_external_url("mailto:").is_err());
        assert!(validate_external_url("https://example.com/\nfoo").is_err());
        assert!(validate_external_url("https://example.com/\u{202E}evil").is_err());
        assert!(validate_external_url("mailto:a@b.com\" /a \"\\\\attacker\\share\\payload").is_err());
        assert!(validate_external_url("mailto:a@b.com /arg").is_err());
        assert!(validate_external_url("mailto:a@b.com<script>").is_err());
        assert!(validate_external_url("mailto:a@b.com`cmd`").is_err());
        assert!(validate_external_url("mailto:a@b.com\u{202E}").is_err());
        assert!(validate_external_url(&format!("https://example.com/{}", "a".repeat(4100))).is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn external_launcher_reaps_child_and_reports_nonzero_exit() {
        let mut ok_cmd = std::process::Command::new("sh");
        ok_cmd.args(["-c", "exit 0"]);
        assert!(super::spawn_and_check_external_launcher(ok_cmd).is_ok());

        let mut fail_cmd = std::process::Command::new("sh");
        fail_cmd.args(["-c", "exit 3"]);
        assert!(super::spawn_and_check_external_launcher(fail_cmd).is_err());
    }

    #[test]
    fn extracts_file_names_from_android_saf_and_utf8_uris() {
        assert_eq!(
            extract_file_name_from_uri(
                "content://com.android.externalstorage.documents/document/primary%3ADownload%2Fsmoke_pattern.pdf"
            ),
            "smoke_pattern.pdf"
        );
        assert_eq!(
            extract_file_name_from_uri(
                "content://com.android.providers.downloads.documents/document/msf%3A100001"
            ),
            "100001"
        );
        assert_eq!(
            extract_file_name_from_uri(
                "content://com.android.providers.downloads.documents/document/msf%3A100001?displayName=My%20Novel.epub"
            ),
            "My Novel.epub"
        );
        assert_eq!(
            extract_file_name_from_uri(
                "content://example/document/primary%3A%E4%B8%AD%E6%96%87%20Book.epub"
            ),
            "中文 Book.epub"
        );
    }
}
