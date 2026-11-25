use std::sync::{Arc, Mutex, RwLock};

use shiro_error_catalog::{
    AppLocale, BoundedErrorBuffer, CommandErrorPayload, ErrorCode, ErrorEventPayload,
    PairedClientError,
};
use uuid::Uuid;

use crate::dto::{BootstrapStateDto, UploadPanelSnapshotDto};

const DEFAULT_ERROR_BUFFER_CAPACITY: usize = 64;

/// Transport hook receiving localized client error events and runtime state updates
/// (e.g. Tauri `app.emit`).
pub trait ErrorSink: Send + Sync {
    fn emit_error(&self, event: &ErrorEventPayload);

    /// Called when the host runtime state transitions asynchronously (for example when a cloud
    /// session is confirmed invalidated during a background or library request).
    fn emit_runtime_state(&self, _state: &BootstrapStateDto) {}

    /// Called whenever the concurrent upload queue snapshot updates.
    fn emit_upload_state(&self, _snapshot: &UploadPanelSnapshotDto) {}
}

/// No-op sink useful in tests and headless callers.
#[derive(Debug, Default)]
pub struct NoopErrorSink;

impl ErrorSink for NoopErrorSink {
    fn emit_error(&self, _event: &ErrorEventPayload) {}
}

/// Single localization, buffering, and event dispatch gateway for all Rust client errors.
pub struct ErrorDispatcher {
    locale: RwLock<AppLocale>,
    buffer: Mutex<BoundedErrorBuffer>,
    sink: Arc<dyn ErrorSink>,
}

impl ErrorDispatcher {
    pub fn new(initial_locale: AppLocale, sink: Arc<dyn ErrorSink>) -> Self {
        Self {
            locale: RwLock::new(initial_locale),
            buffer: Mutex::new(BoundedErrorBuffer::new(DEFAULT_ERROR_BUFFER_CAPACITY)),
            sink,
        }
    }

    pub fn locale(&self) -> AppLocale {
        *self.locale.read().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set_locale(&self, locale: AppLocale) {
        *self.locale.write().unwrap_or_else(|e| e.into_inner()) = locale;
    }

    /// Generates a paired command/event error in the current locale, buffers it, and emits it.
    pub fn dispatch(
        &self,
        code: ErrorCode,
        operation_id: impl Into<String>,
        item_id: Option<String>,
    ) -> PairedClientError {
        let paired = PairedClientError::new(code, self.locale(), operation_id, item_id);
        self.record_and_emit(&paired.event);
        paired
    }

    /// Convenience helper creating a fresh `operation_id` and emitting the localized error event.
    pub fn dispatch_operation(&self, code: ErrorCode) -> CommandErrorPayload {
        self.dispatch(code, Uuid::new_v4().to_string(), None)
            .command
    }

    /// Generates a localized command error payload without buffering or emitting a global error event.
    pub fn command_only(&self, code: ErrorCode) -> CommandErrorPayload {
        PairedClientError::new(code, self.locale(), Uuid::new_v4().to_string(), None).command
    }

    /// Dispatches an error event for a startup/lifecycle diagnostic while preserving the paired ID.
    pub fn dispatch_startup(&self, code: ErrorCode) -> PairedClientError {
        self.dispatch(code, Uuid::new_v4().to_string(), None)
    }

    pub fn buffered_snapshot(&self) -> Vec<ErrorEventPayload> {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot()
    }

    pub fn clear_buffer(&self) {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
    }

    pub fn emit_runtime_state(&self, state: &BootstrapStateDto) {
        self.sink.emit_runtime_state(state);
    }

    pub fn emit_upload_state(&self, snapshot: &UploadPanelSnapshotDto) {
        self.sink.emit_upload_state(snapshot);
    }

    fn record_and_emit(&self, event: &ErrorEventPayload) {
        self.buffer
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(event.clone());
        self.sink.emit_error(event);
    }
}
