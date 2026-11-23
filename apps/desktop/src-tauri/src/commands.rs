use std::{io, path::PathBuf};

use shiro_compatibility::{
    AppTheme, BookTagOutput, BootstrapStateDto, CloudOriginCheckResult, DeleteTagStrategyDto,
    GetBookOutput, ImportBookItemFailure, ImportBooksResult, ImportProgressEvent,
    InitialSetupRequest, LaunchMode, RestoreBooksResult, TagDefinitionOutput, TagTreeNodeOutput,
    TrashedBookOutput, UpdateSettingsRequest, is_cloud_abort_import_code, is_opaque_book_file_name,
};
use shiro_error_catalog::{AppLocale, CommandErrorPayload, ErrorCode};
#[cfg(target_os = "android")]
use tauri::Manager;
use tauri::{AppHandle, State, ipc::Channel};
use tauri_plugin_dialog::{DialogExt, FilePath};
use tauri_plugin_fs::{FsExt, OpenOptions};
use tokio::io::AsyncRead;
use tracing::warn;
use uuid::Uuid;

use crate::DesktopAppState;

const PROGRESS_EMIT_STEP_BYTES: u64 = 256 * 1024;

// -----------------------------------------------------------------------------
// Lifecycle, Settings & Platform Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn get_bootstrap_state(app: AppHandle, state: State<'_, DesktopAppState>) -> BootstrapStateDto {
    let snapshot = state.runtime.bootstrap_state();
    sync_android_status_bar(&app, snapshot.theme);
    snapshot
}

#[tauri::command]
pub async fn complete_initial_setup(
    app: AppHandle,
    state: State<'_, DesktopAppState>,
    request: InitialSetupRequest,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.complete_initial_setup(request).await?;
    crate::sync_window_title(&app, snapshot.locale);
    Ok(snapshot)
}

#[tauri::command]
pub async fn update_settings(
    app: AppHandle,
    state: State<'_, DesktopAppState>,
    request: UpdateSettingsRequest,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.update_settings(request).await?;
    crate::sync_window_title(&app, snapshot.locale);
    Ok(snapshot)
}

#[tauri::command]
pub fn set_app_locale(
    app: AppHandle,
    state: State<'_, DesktopAppState>,
    locale: AppLocale,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    let snapshot = state.runtime.set_locale(locale)?;
    crate::sync_window_title(&app, snapshot.locale);
    Ok(snapshot)
}

#[tauri::command]
pub fn set_app_theme(
    app: AppHandle,
    state: State<'_, DesktopAppState>,
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
        AppTheme::Sakura => "#fff4fe",
        AppTheme::Mint => "#effff2",
        AppTheme::Sky => "#e8ffff",
        AppTheme::Peach => "#fff8e9",
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
                warn!(?error, "failed to synchronize Android status bar color");
            }
        });
    }) {
        warn!(
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
    state: State<'_, DesktopAppState>,
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
    state: State<'_, DesktopAppState>,
    path: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state
        .runtime
        .repair_local_library_root(PathBuf::from(path))
        .await
}

#[tauri::command]
pub async fn retry_startup(
    state: State<'_, DesktopAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.retry_startup().await
}

#[tauri::command]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}

// -----------------------------------------------------------------------------
// Cloud Mode Commands (Probe, Origin Recovery, Auth)
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn cloud_check_origin(
    state: State<'_, DesktopAppState>,
    origin: String,
) -> Result<CloudOriginCheckResult, CommandErrorPayload> {
    Ok(state.runtime.check_cloud_origin(origin).await)
}

#[tauri::command]
pub async fn cloud_probe(
    state: State<'_, DesktopAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_probe().await
}

#[tauri::command]
pub async fn cloud_update_origin(
    state: State<'_, DesktopAppState>,
    origin: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_update_origin(origin).await
}

#[tauri::command]
pub async fn cloud_login(
    state: State<'_, DesktopAppState>,
    username: String,
    password: String,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_login(username, password).await
}

#[tauri::command]
pub async fn cloud_logout(
    state: State<'_, DesktopAppState>,
) -> Result<BootstrapStateDto, CommandErrorPayload> {
    state.runtime.cloud_logout().await
}

// -----------------------------------------------------------------------------
// Bookshelf & Trash Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn books_list(
    state: State<'_, DesktopAppState>,
) -> Result<Vec<GetBookOutput>, CommandErrorPayload> {
    state.runtime.list_books().await
}

#[tauri::command]
pub async fn book_rename(
    state: State<'_, DesktopAppState>,
    id: String,
    title: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.rename_book(id, title).await
}

#[tauri::command]
pub async fn books_delete(
    state: State<'_, DesktopAppState>,
    ids: Vec<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.delete_books(ids).await
}

#[tauri::command]
pub async fn trash_list(
    state: State<'_, DesktopAppState>,
) -> Result<Vec<TrashedBookOutput>, CommandErrorPayload> {
    state.runtime.list_trash().await
}

#[tauri::command]
pub async fn trash_restore(
    state: State<'_, DesktopAppState>,
    ids: Vec<String>,
) -> Result<RestoreBooksResult, CommandErrorPayload> {
    state.runtime.restore_books(ids).await
}

#[tauri::command]
pub async fn trash_permanently_delete(
    state: State<'_, DesktopAppState>,
    ids: Vec<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.permanently_delete_books(ids).await
}

// -----------------------------------------------------------------------------
// Tag Tree & Book-Tag Binding Commands
// -----------------------------------------------------------------------------

#[tauri::command]
pub async fn tags_list(
    state: State<'_, DesktopAppState>,
) -> Result<Vec<TagTreeNodeOutput>, CommandErrorPayload> {
    state.runtime.get_tag_tree().await
}

#[tauri::command]
pub async fn tag_define(
    state: State<'_, DesktopAppState>,
    name: String,
    parent_id: Option<String>,
    color: String,
    allows_value: bool,
    value_regex: Option<String>,
) -> Result<TagDefinitionOutput, CommandErrorPayload> {
    state
        .runtime
        .define_tag(name, parent_id, color, allows_value, value_regex)
        .await
}

#[tauri::command]
pub async fn tag_rename(
    state: State<'_, DesktopAppState>,
    id: String,
    name: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.rename_tag(id, name).await
}

#[tauri::command]
pub async fn tag_recolor(
    state: State<'_, DesktopAppState>,
    id: String,
    color: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.change_tag_color(id, color).await
}

#[tauri::command]
pub async fn tag_move(
    state: State<'_, DesktopAppState>,
    id: String,
    parent_id: Option<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.move_tag(id, parent_id).await
}

#[tauri::command]
pub async fn tag_undefine(
    state: State<'_, DesktopAppState>,
    id: String,
    strategy: DeleteTagStrategyDto,
) -> Result<(), CommandErrorPayload> {
    state.runtime.undefine_tag(id, strategy).await
}

#[tauri::command]
pub async fn book_tags_list(
    state: State<'_, DesktopAppState>,
    book_id: String,
) -> Result<Vec<BookTagOutput>, CommandErrorPayload> {
    state.runtime.get_book_tags(book_id).await
}

#[tauri::command]
pub async fn book_tag_bind(
    state: State<'_, DesktopAppState>,
    book_id: String,
    tag_id: String,
    value: Option<String>,
) -> Result<(), CommandErrorPayload> {
    state.runtime.bind_book_tag(book_id, tag_id, value).await
}

#[tauri::command]
pub async fn book_tag_unbind(
    state: State<'_, DesktopAppState>,
    book_id: String,
    tag_id: String,
) -> Result<(), CommandErrorPayload> {
    state.runtime.unbind_book_tag(book_id, tag_id).await
}

// -----------------------------------------------------------------------------
// Native File Selection & Streaming Import (Local & Cloud, Desktop Paths + Android SAF URIs)
// -----------------------------------------------------------------------------

#[tauri::command]
pub fn cancel_book_import(state: State<'_, DesktopAppState>) -> bool {
    state.cancel_active_import()
}

#[tauri::command]
pub async fn pick_and_import_books(
    app: AppHandle,
    state: State<'_, DesktopAppState>,
    on_event: Channel<ImportProgressEvent>,
) -> Result<ImportBooksResult, CommandErrorPayload> {
    let operation_id = Uuid::new_v4().to_string();

    state.runtime.ensure_library_ready(&operation_id)?;
    let cancel_guard = state.begin_import_session().ok_or_else(|| {
        state
            .runtime
            .error_dispatcher()
            .dispatch(ErrorCode::ImportAlreadyRunning, &operation_id, None)
            .command
    })?;
    let cancel = cancel_guard.token();

    let is_cloud_mode = state.runtime.active_mode() == Some(LaunchMode::Cloud);

    let picked_files = pick_book_files(&app).await;
    let Some(selected_files) = picked_files.filter(|list| !list.is_empty()) else {
        // Picker cancellation is an expected control branch: never emit an error toast.
        return Ok(ImportBooksResult {
            operation_id,
            cancelled: true,
            imported: Vec::new(),
            failed: Vec::new(),
            skipped: Vec::new(),
            aborted_reason: None,
            summary_message: None,
        });
    };

    let total = selected_files.len();
    let all_names: Vec<String> = selected_files.iter().map(extract_file_name).collect();

    let _ = on_event.send(ImportProgressEvent::Started {
        operation_id: operation_id.clone(),
        total,
    });

    let mut imported = Vec::new();
    let mut failed = Vec::new();
    let mut skipped = Vec::new();
    let mut aborted_reason: Option<CommandErrorPayload> = None;
    let mut was_cancelled = false;

    for (index, file_path) in selected_files.into_iter().enumerate() {
        if cancel.is_cancelled() {
            was_cancelled = true;
            skipped.extend(all_names[index..].iter().cloned());
            break;
        }

        let mut file_name = all_names[index].clone();

        let (reader, total_bytes, resolved_name) = match open_book_source(&app, file_path) {
            Ok(source) => source,
            Err(err) => {
                warn!(file_name = %file_name, error = %err, "failed to open picked book file");
                let cmd_err = state
                    .runtime
                    .error_dispatcher()
                    .dispatch(
                        ErrorCode::ImportReadFailed,
                        operation_id.clone(),
                        Some(file_name.clone()),
                    )
                    .command;
                let _ = on_event.send(ImportProgressEvent::ItemFailed {
                    operation_id: operation_id.clone(),
                    index,
                    total,
                    file_name: file_name.clone(),
                    error: cmd_err.clone(),
                });
                failed.push(ImportBookItemFailure {
                    file_name,
                    error: cmd_err,
                });
                continue;
            }
        };

        if is_opaque_book_file_name(&file_name)
            && let Some(real_name) = resolved_name
            && !is_opaque_book_file_name(&real_name)
        {
            file_name = real_name;
        }

        let _ = on_event.send(ImportProgressEvent::ItemProgress {
            operation_id: operation_id.clone(),
            index,
            total,
            file_name: file_name.clone(),
            bytes_read: 0,
            total_bytes,
        });

        let mut last_emitted_bytes: u64 = 0;
        let op_for_cb = operation_id.clone();
        let name_for_cb = file_name.clone();
        let channel_for_cb = on_event.clone();
        let mut on_progress = move |bytes_read: u64| {
            let reached_end = total_bytes.is_some_and(|tb| bytes_read >= tb);
            if reached_end
                || bytes_read.saturating_sub(last_emitted_bytes) >= PROGRESS_EMIT_STEP_BYTES
            {
                last_emitted_bytes = bytes_read;
                let _ = channel_for_cb.send(ImportProgressEvent::ItemProgress {
                    operation_id: op_for_cb.clone(),
                    index,
                    total,
                    file_name: name_for_cb.clone(),
                    bytes_read,
                    total_bytes,
                });
            }
        };

        match state
            .runtime
            .import_stream(
                &operation_id,
                &file_name,
                reader,
                Some(cancel),
                Some(&mut on_progress),
            )
            .await
        {
            Ok(book) => {
                let _ = on_event.send(ImportProgressEvent::ItemSucceeded {
                    operation_id: operation_id.clone(),
                    index,
                    total,
                    book: book.clone(),
                });
                imported.push(book);
            }
            Err(err) if err.code == ErrorCode::ImportCancelled.code() => {
                was_cancelled = true;
                skipped.extend(all_names[index..].iter().cloned());
                break;
            }
            Err(err) => {
                let _ = on_event.send(ImportProgressEvent::ItemFailed {
                    operation_id: operation_id.clone(),
                    index,
                    total,
                    file_name: file_name.clone(),
                    error: err.clone(),
                });
                let should_abort_cloud = is_cloud_mode
                    && ErrorCode::from_code(err.code).is_some_and(is_cloud_abort_import_code);
                failed.push(ImportBookItemFailure {
                    file_name,
                    error: err.clone(),
                });
                if should_abort_cloud {
                    aborted_reason = Some(err);
                    if index + 1 < all_names.len() {
                        skipped.extend(all_names[(index + 1)..].iter().cloned());
                    }
                    break;
                }
            }
        }
    }

    drop(cancel_guard);

    let _ = on_event.send(ImportProgressEvent::Finished {
        operation_id: operation_id.clone(),
        cancelled: was_cancelled,
        imported_count: imported.len(),
        failed_count: failed.len(),
        skipped_count: skipped.len(),
    });

    let summary_message = (!failed.is_empty() || !skipped.is_empty()).then(|| {
        state
            .runtime
            .locale()
            .import_summary(imported.len(), failed.len(), skipped.len())
    });
    Ok(ImportBooksResult {
        operation_id,
        cancelled: was_cancelled,
        imported,
        failed,
        skipped,
        aborted_reason,
        summary_message,
    })
}

fn open_book_source(
    app: &AppHandle,
    file_path: FilePath,
) -> io::Result<(Box<dyn AsyncRead + Unpin + Send>, Option<u64>, Option<String>)> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = app.fs().open(file_path, options)?;
    let resolved_name = resolve_file_name_from_fd(&file);
    let metadata = file.metadata()?;

    #[cfg(target_os = "android")]
    {
        use std::os::unix::fs::FileTypeExt;

        if metadata.file_type().is_fifo() {
            let pipe = tokio::net::unix::pipe::Receiver::from_file(file)?;
            return Ok((Box::new(pipe), None, resolved_name));
        }
    }

    let bytes = Some(metadata.len()).filter(|&len| len > 0);
    Ok((Box::new(tokio::fs::File::from_std(file)), bytes, resolved_name))
}

#[cfg(unix)]
fn resolve_file_name_from_fd(file: &std::fs::File) -> Option<String> {
    use std::os::fd::AsRawFd;

    let target = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).ok()?;
    let target_str = target.to_string_lossy();
    if target_str.starts_with("pipe:")
        || target_str.starts_with("socket:")
        || target_str.starts_with("anon_inode:")
        || target_str.ends_with(" (deleted)")
    {
        return None;
    }
    target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.trim().is_empty())
}

#[cfg(not(unix))]
fn resolve_file_name_from_fd(_file: &std::fs::File) -> Option<String> {
    None
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
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2])) {
                out.push((hi << 4) | lo);
                i += 3;
                continue;
            }
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
    use super::{extract_file_name_from_uri, resolve_file_name_from_fd};

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

    #[cfg(unix)]
    #[test]
    fn resolves_real_file_name_from_open_file_descriptor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Real Book Title.epub");
        std::fs::write(&path, b"placeholder").unwrap();
        let file = std::fs::File::open(&path).unwrap();
        assert_eq!(
            resolve_file_name_from_fd(&file).as_deref(),
            Some("Real Book Title.epub")
        );
    }
}
