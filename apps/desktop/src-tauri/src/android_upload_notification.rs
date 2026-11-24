use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock, Weak, atomic::AtomicI64},
};

use shiro_compatibility::{ClientRuntime, UploadPanelSnapshotDto, UploadTaskStatusDto};
use shiro_error_catalog::{AppLocale, ErrorCode};
use tauri::AppHandle;
#[cfg(target_os = "android")]
use tauri::{Emitter, Manager};
use tokio::sync::{Mutex as AsyncMutex, oneshot, watch};

#[cfg(target_os = "android")]
use crate::UPLOAD_OPEN_UI_EVENT_NAME;

#[cfg(target_os = "android")]
const ONGOING_THROTTLE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(350);
#[cfg(target_os = "android")]
const PERMISSION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum NotificationPhase {
    #[default]
    Idle,
    Uploading,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DesiredNotification {
    Clear,
    Ongoing {
        title: String,
        body: String,
        progress: i32,
        max: i32,
        indeterminate: bool,
    },
    Completed {
        title: String,
        body: String,
    },
}

#[derive(Debug, Default)]
pub(crate) struct NotificationPlanner {
    locale: AppLocale,
    phase: NotificationPhase,
    completed_notification_active: bool,
    completed_notification_consumed: bool,
    mobile_ui_open: bool,
    pending_open_ui: bool,
}

impl NotificationPlanner {
    pub(crate) fn new(initial_locale: AppLocale) -> Self {
        Self {
            locale: initial_locale,
            ..Self::default()
        }
    }

    pub(crate) fn set_locale(&mut self, locale: AppLocale) {
        self.locale = locale;
    }

    pub(crate) fn on_snapshot(&mut self, snapshot: &UploadPanelSnapshotDto) -> Option<DesiredNotification> {
        if !snapshot.visible || snapshot.stats.total == 0 {
            let was_active = self.phase != NotificationPhase::Idle;
            self.phase = NotificationPhase::Idle;
            self.completed_notification_active = false;
            self.completed_notification_consumed = false;
            self.mobile_ui_open = false;
            return was_active.then_some(DesiredNotification::Clear);
        }

        if snapshot.stats.remaining > 0 {
            self.phase = NotificationPhase::Uploading;
            self.completed_notification_active = false;
            self.completed_notification_consumed = false;
            let (progress, indeterminate) = compute_overall_progress(snapshot);
            let (title, body) = format_ongoing_text(self.locale, snapshot, progress);
            return Some(DesiredNotification::Ongoing {
                title,
                body,
                progress,
                max: 100,
                indeterminate,
            });
        }

        if self.completed_notification_consumed {
            self.phase = NotificationPhase::Completed;
            self.completed_notification_active = false;
            return None;
        }

        self.phase = NotificationPhase::Completed;
        self.completed_notification_active = true;
        let (title, body) = format_completed_text(self.locale, snapshot);
        Some(DesiredNotification::Completed { title, body })
    }

    pub(crate) fn on_notification_tapped(&mut self, is_completed: bool) {
        if is_completed {
            self.completed_notification_active = false;
            self.completed_notification_consumed = true;
        }
        self.pending_open_ui = true;
    }

    /// Returns `true` when retained terminal results should be cleared immediately.
    pub(crate) fn on_notification_dismissed(&mut self) -> bool {
        self.completed_notification_active = false;
        self.completed_notification_consumed = true;
        self.phase == NotificationPhase::Completed && !self.mobile_ui_open
    }

    pub(crate) fn consume_pending_open_ui(&mut self) -> bool {
        std::mem::take(&mut self.pending_open_ui)
    }

    /// Updates `mobile_ui_open` and returns `true` when closing the UI should clear completed tasks.
    pub(crate) fn set_mobile_ui_open(&mut self, open: bool) -> bool {
        self.mobile_ui_open = open;
        if open {
            self.pending_open_ui = false;
            return false;
        }
        if self.phase == NotificationPhase::Completed {
            self.completed_notification_active = false;
            self.completed_notification_consumed = true;
            return true;
        }
        false
    }

    pub(crate) fn clear_state(&mut self) {
        self.phase = NotificationPhase::Idle;
        self.completed_notification_active = false;
        self.completed_notification_consumed = false;
        self.mobile_ui_open = false;
        self.pending_open_ui = false;
    }
}

pub(crate) fn compute_overall_progress(snapshot: &UploadPanelSnapshotDto) -> (i32, bool) {
    let total = snapshot.stats.total;
    if total == 0 {
        return (0, true);
    }

    let terminal_done = (snapshot.stats.succeeded + snapshot.stats.failed).min(total);
    let mut sum_percent: u64 = (terminal_done as u64) * 100;
    let mut active_with_progress = false;

    for task in &snapshot.tasks {
        if let UploadTaskStatusDto::Active {
            progress_percent, ..
        } = &task.status
        {
            sum_percent = sum_percent.saturating_add(u64::from((*progress_percent).min(100)));
            active_with_progress = true;
        }
    }

    let avg = (sum_percent / (total as u64)).min(100) as i32;
    let indeterminate = terminal_done == 0 && !active_with_progress;
    (avg, indeterminate)
}

fn format_ongoing_text(
    locale: AppLocale,
    snapshot: &UploadPanelSnapshotDto,
    progress: i32,
) -> (String, String) {
    let total = snapshot.stats.total;
    let done = total.saturating_sub(snapshot.stats.remaining);
    match locale {
        AppLocale::Zh => (
            format!("正在上传书籍 ({done}/{total})"),
            format!("已完成 {progress}% · 点击查看详情"),
        ),
        AppLocale::En => (
            format!("Uploading books ({done}/{total})"),
            format!("{progress}% complete · Tap to view details"),
        ),
    }
}

fn format_completed_text(locale: AppLocale, snapshot: &UploadPanelSnapshotDto) -> (String, String) {
    let succeeded = snapshot.stats.succeeded;
    let failed = snapshot.stats.failed;
    match locale {
        AppLocale::Zh => {
            if failed == 0 {
                (
                    "上传完成".to_owned(),
                    format!("已成功上传 {succeeded} 本书籍 · 点击查看结果"),
                )
            } else if succeeded == 0 {
                (
                    "上传未成功".to_owned(),
                    format!("{failed} 本书籍上传失败 · 点击查看详情"),
                )
            } else {
                (
                    "上传完成".to_owned(),
                    format!("成功 {succeeded} 本，失败 {failed} 本 · 点击查看结果"),
                )
            }
        }
        AppLocale::En => {
            if failed == 0 {
                (
                    "Upload complete".to_owned(),
                    format!("{succeeded} uploaded · Tap to view results"),
                )
            } else if succeeded == 0 {
                (
                    "Upload failed".to_owned(),
                    format!("{failed} failed · Tap to view details"),
                )
            } else {
                (
                    "Upload complete".to_owned(),
                    format!("{succeeded} uploaded, {failed} failed · Tap to view results"),
                )
            }
        }
    }
}

#[cfg(target_os = "android")]
struct AndroidNativeEnv {
    vm: jni::JavaVM,
    bridge_class: jni::objects::GlobalRef,
    app_context: jni::objects::GlobalRef,
}

#[cfg(target_os = "android")]
static ANDROID_NATIVE_ENV: OnceLock<AndroidNativeEnv> = OnceLock::new();

#[cfg_attr(not(target_os = "android"), allow(dead_code))]
struct AndroidUploadNotificationController {
    app: AppHandle,
    runtime: Weak<ClientRuntime>,
    planner: Mutex<NotificationPlanner>,
    permission_lock: AsyncMutex<()>,
    pending_permissions: Mutex<HashMap<i64, oneshot::Sender<bool>>>,
    next_request_id: AtomicI64,
    desired_tx: watch::Sender<Option<DesiredNotification>>,
}

static CONTROLLER: OnceLock<Arc<AndroidUploadNotificationController>> = OnceLock::new();

pub(crate) fn init(app: AppHandle, runtime: Weak<ClientRuntime>, initial_locale: AppLocale) {
    if CONTROLLER.get().is_some() {
        return;
    }

    let (desired_tx, desired_rx) = watch::channel(None);
    let controller = Arc::new(AndroidUploadNotificationController {
        app,
        runtime,
        planner: Mutex::new(NotificationPlanner::new(initial_locale)),
        permission_lock: AsyncMutex::new(()),
        pending_permissions: Mutex::new(HashMap::new()),
        next_request_id: AtomicI64::new(1),
        desired_tx,
    });

    if CONTROLLER.set(Arc::clone(&controller)).is_ok() {
        #[cfg(target_os = "android")]
        tauri::async_runtime::spawn(run_dispatch_loop(desired_rx));
        #[cfg(not(target_os = "android"))]
        let _ = desired_rx;
    }
}

pub(crate) fn on_upload_snapshot(snapshot: &UploadPanelSnapshotDto) {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    let desired = {
        let mut planner = controller
            .planner
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        planner.on_snapshot(snapshot)
    };
    if let Some(desired) = desired {
        let _ = controller.desired_tx.send(Some(desired));
    }
}

pub(crate) fn on_locale_changed(locale: AppLocale, snapshot: &UploadPanelSnapshotDto) {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    let desired = {
        let mut planner = controller
            .planner
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        planner.set_locale(locale);
        planner.on_snapshot(snapshot)
    };
    if let Some(desired) = desired {
        let _ = controller.desired_tx.send(Some(desired));
    }
}

pub(crate) fn consume_pending_open_ui() -> bool {
    let Some(controller) = CONTROLLER.get() else {
        return false;
    };
    controller
        .planner
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .consume_pending_open_ui()
}

/// Updates mobile upload UI visibility; returns `true` if retained results should be cleared.
pub(crate) fn set_mobile_ui_open(open: bool) -> bool {
    let Some(controller) = CONTROLLER.get() else {
        return false;
    };
    controller
        .planner
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .set_mobile_ui_open(open)
}

pub(crate) fn clear_immediately() {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    controller
        .planner
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .clear_state();
    let _ = controller.desired_tx.send(Some(DesiredNotification::Clear));
    #[cfg(target_os = "android")]
    dispatch_to_android(DesiredNotification::Clear);
}

#[cfg(not(target_os = "android"))]
pub(crate) async fn ensure_permission(_app: &AppHandle) -> Result<(), ErrorCode> {
    Ok(())
}

#[cfg(target_os = "android")]
pub(crate) async fn ensure_permission(app: &AppHandle) -> Result<(), ErrorCode> {
    let Some(controller) = CONTROLLER.get() else {
        return Err(ErrorCode::ImportNotificationPermissionRequired);
    };

    let _guard = controller.permission_lock.lock().await;
    let request_id = controller
        .next_request_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (tx, rx) = oneshot::channel();
    controller
        .pending_permissions
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .insert(request_id, tx);

    struct PendingGuard<'a> {
        map: &'a Mutex<HashMap<i64, oneshot::Sender<bool>>>,
        request_id: i64,
    }
    impl Drop for PendingGuard<'_> {
        fn drop(&mut self) {
            self.map
                .lock()
                .unwrap_or_else(|err| err.into_inner())
                .remove(&self.request_id);
        }
    }
    let _pending_guard = PendingGuard {
        map: &controller.pending_permissions,
        request_id,
    };

    if !dispatch_request_permission(app, request_id) {
        return Err(ErrorCode::ImportNotificationPermissionRequired);
    }

    match tokio::time::timeout(PERMISSION_TIMEOUT, rx).await {
        Ok(Ok(true)) => Ok(()),
        _ => Err(ErrorCode::ImportNotificationPermissionRequired),
    }
}

#[cfg(target_os = "android")]
async fn run_dispatch_loop(mut desired_rx: watch::Receiver<Option<DesiredNotification>>) {
    let mut last_ongoing_sent: Option<tokio::time::Instant> = None;
    let mut last_ongoing_dispatched: Option<DesiredNotification> = None;
    let mut last_was_ongoing = false;

    while desired_rx.changed().await.is_ok() {
        loop {
            let next = desired_rx.borrow_and_update().clone();
            let Some(desired) = next else {
                break;
            };

            let is_ongoing = matches!(desired, DesiredNotification::Ongoing { .. });
            if is_ongoing {
                if last_ongoing_dispatched.as_ref() == Some(&desired) {
                    if !desired_rx.has_changed().unwrap_or(false) {
                        break;
                    }
                    continue;
                }
                if last_was_ongoing && let Some(last) = last_ongoing_sent {
                    let deadline = last + ONGOING_THROTTLE_INTERVAL;
                    let now = tokio::time::Instant::now();
                    if now < deadline {
                        tokio::select! {
                            _ = tokio::time::sleep_until(deadline) => {}
                            changed = desired_rx.changed() => {
                                if changed.is_err() {
                                    return;
                                }
                                continue;
                            }
                        }
                    }
                }
                last_ongoing_dispatched = Some(desired.clone());
                dispatch_to_android(desired);
                last_ongoing_sent = Some(tokio::time::Instant::now());
                last_was_ongoing = true;
            } else {
                dispatch_to_android(desired);
                last_ongoing_dispatched = None;
                last_ongoing_sent = None;
                last_was_ongoing = false;
            }

            if !desired_rx.has_changed().unwrap_or(false) {
                break;
            }
        }
    }
}

#[cfg(target_os = "android")]
fn check_and_clear_jni_exception(env: &mut jni::JNIEnv<'_>) -> bool {
    match env.exception_check() {
        Ok(true) => {
            let _ = env.exception_describe();
            let _ = env.exception_clear();
            true
        }
        _ => false,
    }
}

#[cfg(target_os = "android")]
fn dispatch_request_permission(app: &AppHandle, request_id: i64) -> bool {
    use jni::objects::JValue;

    let Some(webview) = app.get_webview_window("main") else {
        return false;
    };

    webview
        .with_webview(move |webview| {
            webview.jni_handle().exec(move |env, activity, _| {
                if let Err(err) = env.call_method(
                    activity,
                    "requestUploadNotificationPermission",
                    "(J)V",
                    &[JValue::Long(request_id)],
                ) {
                    tracing::warn!(?err, "failed to invoke requestUploadNotificationPermission");
                    check_and_clear_jni_exception(env);
                    if let Some(controller) = CONTROLLER.get() {
                        if let Some(tx) = controller
                            .pending_permissions
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .remove(&request_id)
                        {
                            let _ = tx.send(false);
                        }
                    }
                }
            });
        })
        .is_ok()
}

#[cfg(target_os = "android")]
fn dispatch_to_android(desired: DesiredNotification) {
    use jni::objects::JValue;

    let Some(native) = ANDROID_NATIVE_ENV.get() else {
        tracing::warn!("Android notification bridge context not registered yet");
        return;
    };

    let Ok(mut env) = native.vm.attach_current_thread() else {
        tracing::warn!("failed to attach current thread to JavaVM for notification");
        return;
    };

    let result: jni::errors::Result<()> = (|| {
        let context = native.app_context.as_obj();
        match desired {
            DesiredNotification::Clear => {
                env.call_static_method(
                    &native.bridge_class,
                    "clearFromNative",
                    "(Landroid/content/Context;)V",
                    &[JValue::Object(context)],
                )?;
            }
            DesiredNotification::Ongoing {
                title,
                body,
                progress,
                max,
                indeterminate,
            } => {
                let j_title = env.new_string(title)?;
                let j_body = env.new_string(body)?;
                env.call_static_method(
                    &native.bridge_class,
                    "showOngoingFromNative",
                    "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;IIZ)V",
                    &[
                        JValue::Object(context),
                        JValue::Object(&j_title),
                        JValue::Object(&j_body),
                        JValue::Int(progress),
                        JValue::Int(max),
                        JValue::Bool(u8::from(indeterminate)),
                    ],
                )?;
            }
            DesiredNotification::Completed { title, body } => {
                let j_title = env.new_string(title)?;
                let j_body = env.new_string(body)?;
                env.call_static_method(
                    &native.bridge_class,
                    "showCompletedFromNative",
                    "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)V",
                    &[
                        JValue::Object(context),
                        JValue::Object(&j_title),
                        JValue::Object(&j_body),
                    ],
                )?;
            }
        }
        Ok(())
    })();

    if let Err(err) = result {
        tracing::warn!(?err, "failed to update Android upload notification");
        check_and_clear_jni_exception(&mut env);
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_shiroaqua_shiroreader_UploadNotificationBridge_nativeRegisterContext(
    env: jni::JNIEnv<'_>,
    class: jni::objects::JClass<'_>,
    app_context: jni::objects::JObject<'_>,
) {
    if ANDROID_NATIVE_ENV.get().is_some() {
        return;
    }
    let Ok(vm) = env.get_java_vm() else {
        tracing::warn!("failed to acquire JavaVM in nativeRegisterContext");
        return;
    };
    let Ok(bridge_class) = env.new_global_ref(&class) else {
        tracing::warn!("failed to retain UploadNotificationBridge global ref");
        return;
    };
    let Ok(app_context) = env.new_global_ref(&app_context) else {
        tracing::warn!("failed to retain applicationContext global ref");
        return;
    };
    let _ = ANDROID_NATIVE_ENV.set(AndroidNativeEnv {
        vm,
        bridge_class,
        app_context,
    });
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_shiroaqua_shiroreader_UploadNotificationBridge_nativeOnPermissionResult(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    request_id: jni::sys::jlong,
    granted: jni::sys::jboolean,
) {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    if let Some(tx) = controller
        .pending_permissions
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .remove(&request_id)
    {
        let _ = tx.send(granted != 0);
    }
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_shiroaqua_shiroreader_UploadNotificationBridge_nativeOnNotificationTapped(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
    is_completed: jni::sys::jboolean,
) {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    {
        let mut planner = controller
            .planner
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        planner.on_notification_tapped(is_completed != 0);
    }
    let _ = controller.app.emit(UPLOAD_OPEN_UI_EVENT_NAME, ());
}

#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub extern "system" fn Java_com_shiroaqua_shiroreader_UploadNotificationBridge_nativeOnNotificationDismissed(
    _env: jni::JNIEnv<'_>,
    _class: jni::objects::JClass<'_>,
) {
    let Some(controller) = CONTROLLER.get() else {
        return;
    };
    let should_clear = {
        let mut planner = controller
            .planner
            .lock()
            .unwrap_or_else(|err| err.into_inner());
        planner.on_notification_dismissed()
    };
    if should_clear && let Some(runtime) = controller.runtime.upgrade() {
        tauri::async_runtime::spawn(async move {
            runtime.upload_clear_completed().await;
        });
    }
}

#[cfg(test)]
mod tests {
    use shiro_compatibility::{UploadActiveStage, UploadSessionStatsDto, UploadTaskDto};
    use shiro_error_catalog::PairedClientError;

    use super::*;

    fn snapshot_with(
        visible: bool,
        remaining: usize,
        succeeded: usize,
        failed: usize,
        tasks: Vec<UploadTaskDto>,
    ) -> UploadPanelSnapshotDto {
        UploadPanelSnapshotDto {
            revision: 1,
            visible,
            concurrency: 4,
            completed_counter: succeeded as u64,
            stats: UploadSessionStatsDto {
                total: remaining + succeeded + failed,
                remaining,
                active: tasks
                    .iter()
                    .filter(|t| matches!(t.status, UploadTaskStatusDto::Active { .. }))
                    .count(),
                succeeded,
                failed,
            },
            has_dismissible_errors: failed > 0,
            tasks,
        }
    }

    fn active_task(id: u64, progress_percent: u8) -> UploadTaskDto {
        UploadTaskDto {
            id,
            file_name: format!("book-{id}.epub"),
            status: UploadTaskStatusDto::Active {
                stage: UploadActiveStage::Uploading,
                progress_percent,
                bytes_processed: 50,
                total_bytes: Some(100),
                speed_bytes_per_sec: Some(1024),
                eta_seconds: Some(1),
            },
        }
    }

    fn succeeded_task(id: u64) -> UploadTaskDto {
        UploadTaskDto {
            id,
            file_name: format!("book-{id}.epub"),
            status: UploadTaskStatusDto::Succeeded {
                book_id: id.to_string(),
                title: format!("Book {id}"),
            },
        }
    }

    fn failed_task(id: u64) -> UploadTaskDto {
        UploadTaskDto {
            id,
            file_name: format!("bad-{id}.epub"),
            status: UploadTaskStatusDto::Failed {
                error: PairedClientError::new(
                    ErrorCode::BookFileUnsupported,
                    AppLocale::En,
                    "op-1",
                    None,
                )
                .event,
            },
        }
    }

    #[test]
    fn planner_tracks_ongoing_completed_consumption_and_clear() {
        let mut planner = NotificationPlanner::new(AppLocale::Zh);

        let initial = snapshot_with(
            true,
            2,
            0,
            0,
            vec![
                active_task(1, 0),
                UploadTaskDto {
                    id: 2,
                    file_name: "queued.epub".to_owned(),
                    status: UploadTaskStatusDto::Queued,
                },
            ],
        );
        let desired = planner.on_snapshot(&initial);
        assert_eq!(
            desired,
            Some(DesiredNotification::Ongoing {
                title: "正在上传书籍 (0/2)".to_owned(),
                body: "已完成 0% · 点击查看详情".to_owned(),
                progress: 0,
                max: 100,
                indeterminate: false,
            })
        );

        let partial = snapshot_with(true, 1, 1, 0, vec![succeeded_task(1), active_task(2, 50)]);
        assert_eq!(
            planner.on_snapshot(&partial),
            Some(DesiredNotification::Ongoing {
                title: "正在上传书籍 (1/2)".to_owned(),
                body: "已完成 75% · 点击查看详情".to_owned(),
                progress: 75,
                max: 100,
                indeterminate: false,
            })
        );

        let done = snapshot_with(true, 0, 1, 1, vec![succeeded_task(1), failed_task(2)]);
        assert_eq!(
            planner.on_snapshot(&done),
            Some(DesiredNotification::Completed {
                title: "上传完成".to_owned(),
                body: "成功 1 本，失败 1 本 · 点击查看结果".to_owned(),
            })
        );

        // User taps the completed notification: notification is consumed and UI opens.
        planner.on_notification_tapped(true);
        assert!(planner.consume_pending_open_ui());
        assert!(!planner.consume_pending_open_ui());
        assert!(!planner.set_mobile_ui_open(true));

        // Subsequent remaining==0 snapshot (e.g., concurrency edit or dismissing a failed task)
        // MUST NOT re-post a completed notification.
        let updated_after_tap = snapshot_with(true, 0, 1, 0, vec![succeeded_task(1)]);
        assert_eq!(planner.on_snapshot(&updated_after_tap), None);

        // Closing the mobile UI after the completed notification was consumed requests clear.
        assert!(planner.set_mobile_ui_open(false));

        // Clearing backend hides the panel and emits Clear once.
        let hidden = snapshot_with(false, 0, 0, 0, vec![]);
        assert_eq!(planner.on_snapshot(&hidden), Some(DesiredNotification::Clear));
        assert_eq!(planner.on_snapshot(&hidden), None);
    }

    #[test]
    fn dismiss_completed_notification_clears_only_when_ui_closed() {
        let mut planner = NotificationPlanner::new(AppLocale::En);
        let done = snapshot_with(true, 0, 2, 0, vec![succeeded_task(1), succeeded_task(2)]);
        assert!(matches!(
            planner.on_snapshot(&done),
            Some(DesiredNotification::Completed { .. })
        ));

        // Swiped away while UI is closed -> immediately clears retained tasks.
        assert!(planner.on_notification_dismissed());

        // New batch resets consumed flag and starts ongoing again.
        let next_batch = snapshot_with(true, 1, 0, 0, vec![active_task(3, 20)]);
        assert!(matches!(
            planner.on_snapshot(&next_batch),
            Some(DesiredNotification::Ongoing { .. })
        ));

        let next_done = snapshot_with(true, 0, 1, 0, vec![succeeded_task(3)]);
        assert!(matches!(
            planner.on_snapshot(&next_done),
            Some(DesiredNotification::Completed { .. })
        ));

        // Tapped while ongoing earlier, UI open now, dismiss when UI open does NOT clear immediately;
        // closing UI later triggers clear.
        assert!(!planner.set_mobile_ui_open(true));
        assert!(!planner.on_notification_dismissed());
        assert!(planner.set_mobile_ui_open(false));
    }

    #[test]
    fn ongoing_tap_keeps_completed_notification_unconsumed_and_ui_close_preserves_it() {
        let mut planner = NotificationPlanner::new(AppLocale::En);
        let running = snapshot_with(true, 1, 0, 0, vec![active_task(1, 40)]);
        assert!(matches!(
            planner.on_snapshot(&running),
            Some(DesiredNotification::Ongoing { .. })
        ));

        // Tapping an ongoing notification opens UI without marking completed consumed.
        planner.on_notification_tapped(false);
        assert!(planner.consume_pending_open_ui());
        assert!(!planner.set_mobile_ui_open(true));

        // Closing UI while still uploading does not request clearing.
        assert!(!planner.set_mobile_ui_open(false));

        // Completing afterward still posts the completed notification.
        let done = snapshot_with(true, 0, 1, 0, vec![succeeded_task(1)]);
        assert_eq!(
            planner.on_snapshot(&done),
            Some(DesiredNotification::Completed {
                title: "Upload complete".to_owned(),
                body: "1 uploaded · Tap to view results".to_owned(),
            })
        );
    }

    #[test]
    fn closing_ui_after_watching_completion_clears_active_completed_notification() {
        let mut planner = NotificationPlanner::new(AppLocale::En);
        let running = snapshot_with(true, 1, 0, 0, vec![active_task(1, 50)]);
        assert!(matches!(
            planner.on_snapshot(&running),
            Some(DesiredNotification::Ongoing { .. })
        ));

        planner.on_notification_tapped(false);
        assert!(!planner.set_mobile_ui_open(true));

        // Upload finishes while UI is already open -> completed notification is active in shade.
        let done = snapshot_with(true, 0, 1, 0, vec![succeeded_task(1)]);
        assert!(matches!(
            planner.on_snapshot(&done),
            Some(DesiredNotification::Completed { .. })
        ));

        // Closing the mobile UI after viewing completion dismisses notification & clears results.
        assert!(planner.set_mobile_ui_open(false));
        assert_eq!(planner.on_snapshot(&done), None);

        let hidden = snapshot_with(false, 0, 0, 0, vec![]);
        assert_eq!(planner.on_snapshot(&hidden), Some(DesiredNotification::Clear));
    }

    #[test]
    fn hidden_snapshot_resets_mobile_ui_open_so_subsequent_dismiss_clears() {
        let mut planner = NotificationPlanner::new(AppLocale::En);
        let running = snapshot_with(true, 1, 0, 0, vec![active_task(1, 20)]);
        assert!(matches!(
            planner.on_snapshot(&running),
            Some(DesiredNotification::Ongoing { .. })
        ));
        assert!(!planner.set_mobile_ui_open(true));

        // Session aborts or clears while mobile UI was open -> resets mobile_ui_open.
        let hidden = snapshot_with(false, 0, 0, 0, vec![]);
        assert_eq!(planner.on_snapshot(&hidden), Some(DesiredNotification::Clear));

        // Next batch completes with UI closed; swiping completed notification clears immediately.
        let next_running = snapshot_with(true, 1, 0, 0, vec![active_task(2, 10)]);
        assert!(matches!(
            planner.on_snapshot(&next_running),
            Some(DesiredNotification::Ongoing { .. })
        ));
        let next_done = snapshot_with(true, 0, 1, 0, vec![succeeded_task(2)]);
        assert!(matches!(
            planner.on_snapshot(&next_done),
            Some(DesiredNotification::Completed { .. })
        ));
        assert!(planner.on_notification_dismissed());
    }
}
