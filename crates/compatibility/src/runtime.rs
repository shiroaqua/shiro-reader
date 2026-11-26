use std::{
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock, RwLock, Weak},
};

use bytes::Bytes;
use pdfium_render::prelude::Pdfium;
use shiro_application::library::{
    book::{
        commands::{
            DeleteBooksCommand, GetBookCoverCommand, GetBookFileMetaCommand,
            PermanentlyDeleteBooksCommand, RenameBookCommand, RestoreBookIdCommand,
        },
        dto::{GetBookOutput, TrashedBookOutput},
        queries::GetBooksQuery,
    },
    ports::CoverByteStream,
    tag::{
        commands::{
            BindBookTagCommand, ChangeTagColorCommand, CreateTagDefinitionCommand, MoveTagCommand,
            RenameTagCommand, UnbindBookTagCommand, UndefineTagCommand,
        },
        dto::BookTagOutput,
    },
};
use shiro_domain::library::book::file::entity::BookFileType;
use shiro_error_catalog::{AppLocale, CommandErrorPayload, ErrorCode, PairedClientError};
use shiro_http_contract::ServerMetaResponse;
use shiro_infrastructure::{db::pool::checkpoint_library_wal, library_runtime::LocalLibrary};
use tokio::{io::AsyncRead, sync::Mutex as AsyncMutex};
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    cloud_client::{CloudClient, CloudProbeOutcome, SessionVerifyOutcome, is_session_invalid_code},
    config::{
        AppTheme, ClientConfig, ConfigStore, LaunchMode, PlatformCapabilities,
        clamp_upload_concurrency, normalize_cloud_origin,
    },
    credentials::{CredentialStore, EncryptedFileCredentialStore},
    dto::{
        BootstrapStateDto, CloudOriginCheckResult, CloudStatusDto, DeleteTagStrategyDto,
        InitialSetupRequest, RestoreBooksItemResult, RestoreBooksResult, StartupDiagnostic,
        StartupPhase, TagDefinitionDto, TagTreeNodeDto, UpdateSettingsRequest,
        UploadPanelSnapshotDto,
    },
    epub_reader::{
        EpubSessionId, OpenEpubResponseDto,
        byte_source::BookByteSource,
        session::{EpubSessionManager, InsertSessionError, SessionGuardToken},
        zip_index::ZipIndex,
    },
    error_sink::{ErrorDispatcher, ErrorSink},
    local_import::{ImportProgressCallback, ImportTarget, import_reader_into_local},
    tag_value::TagValueType,
    upload_manager::{
        EnqueueUploadItem, StdFsSourceOpener, UploadManager, UploadSourceOpener,
    },
};

struct CloudRuntimeContext {
    client: Arc<CloudClient>,
    phase: StartupPhase,
    server_meta: Option<ServerMetaResponse>,
    diagnostic: Option<StartupDiagnostic>,
}

enum ActiveRuntimeState {
    SetupRequired,
    LocalReady(Arc<LocalLibrary>),
    Cloud(CloudRuntimeContext),
    StartupFailed {
        diagnostic: StartupDiagnostic,
        config_corrupted: bool,
    },
}

enum LibraryBackend {
    Local(Arc<LocalLibrary>),
    Cloud(Arc<CloudClient>),
}

struct RuntimeInner {
    config: ClientConfig,
    active_mode: Option<LaunchMode>,
    state: ActiveRuntimeState,
}

/// Transport-neutral client runtime orchestrating startup lifecycle, settings persistence,
/// fixed per-process launch mode, localized error dispatch, and unified local/cloud library operations.
pub struct ClientRuntime {
    config_store: ConfigStore,
    credential_store: Arc<dyn CredentialStore>,
    cloud_temp_dir: PathBuf,
    capabilities: PlatformCapabilities,
    pdfium: Option<Arc<Mutex<Pdfium>>>,
    error_dispatcher: Arc<ErrorDispatcher>,
    lifecycle_lock: AsyncMutex<()>,
    inner: RwLock<RuntimeInner>,
    self_weak: OnceLock<Weak<Self>>,
    upload_manager: OnceLock<UploadManager>,
    epub_sessions: EpubSessionManager,
}

impl ClientRuntime {
    /// Initializes the client runtime from disk without ever panicking or silently wiping bad data.
    pub async fn initialize(
        config_dir: impl Into<PathBuf>,
        capabilities: PlatformCapabilities,
        detected_locale: AppLocale,
        pdfium: Option<Arc<Mutex<Pdfium>>>,
        error_sink: Arc<dyn ErrorSink>,
    ) -> Arc<Self> {
        Self::initialize_with_source_opener(
            config_dir,
            capabilities,
            detected_locale,
            pdfium,
            error_sink,
            Arc::new(StdFsSourceOpener),
        )
        .await
    }

    /// Initializes the client runtime with a host-provided [`UploadSourceOpener`] (e.g. Tauri
    /// file/URI opener on desktop and mobile).
    pub async fn initialize_with_source_opener(
        config_dir: impl Into<PathBuf>,
        capabilities: PlatformCapabilities,
        detected_locale: AppLocale,
        pdfium: Option<Arc<Mutex<Pdfium>>>,
        error_sink: Arc<dyn ErrorSink>,
        source_opener: Arc<dyn UploadSourceOpener>,
    ) -> Arc<Self> {
        let config_dir = config_dir.into();
        let config_store = ConfigStore::new(&config_dir);
        let data_base = capabilities
            .default_library_root
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| config_dir.clone());
        // Separate encryption key directory from per-origin ciphertext directory.
        let credential_store: Arc<dyn CredentialStore> =
            Arc::new(EncryptedFileCredentialStore::new(
                config_dir.join("keys"),
                data_base.join("cloud-credentials"),
            ));
        let cloud_temp_dir = data_base.join("cloud-temp");
        let error_dispatcher = Arc::new(ErrorDispatcher::new(detected_locale, error_sink));

        let mut startup_cloud_origin: Option<String> = None;

        let inner = match config_store.load() {
            Ok(None) => RuntimeInner {
                config: ClientConfig::initial(detected_locale),
                active_mode: None,
                state: ActiveRuntimeState::SetupRequired,
            },
            Err(err) => {
                error!(error = %err, "client config load failed");
                let paired = error_dispatcher.dispatch_startup(ErrorCode::ConfigInvalid);
                let diagnostic = diagnostic_from_paired(
                    &paired,
                    Some(err.reason),
                    Some(err.path.display().to_string()),
                );
                RuntimeInner {
                    config: ClientConfig::initial(detected_locale),
                    active_mode: None,
                    state: ActiveRuntimeState::StartupFailed {
                        diagnostic,
                        config_corrupted: true,
                    },
                }
            }
            Ok(Some(config)) => {
                error_dispatcher.set_locale(config.locale);
                match config.mode {
                    None => RuntimeInner {
                        config,
                        active_mode: None,
                        state: ActiveRuntimeState::SetupRequired,
                    },
                    Some(LaunchMode::Local) => {
                        let effective_root = config.effective_local_root(&capabilities);
                        match LocalLibrary::open(effective_root.clone(), pdfium.clone()).await {
                            Ok(library) => {
                                info!(root = %effective_root.display(), "local library initialized");
                                RuntimeInner {
                                    config,
                                    active_mode: Some(LaunchMode::Local),
                                    state: ActiveRuntimeState::LocalReady(Arc::new(library)),
                                }
                            }
                            Err(err) => {
                                let code = classify_local_startup_error(&err);
                                error!(root = %effective_root.display(), error = %err, "local library startup failed");
                                let paired = error_dispatcher.dispatch_startup(code);
                                let diagnostic = diagnostic_from_paired(
                                    &paired,
                                    Some(err.to_string()),
                                    Some(effective_root.display().to_string()),
                                );
                                RuntimeInner {
                                    config,
                                    active_mode: Some(LaunchMode::Local),
                                    state: ActiveRuntimeState::StartupFailed {
                                        diagnostic,
                                        config_corrupted: false,
                                    },
                                }
                            }
                        }
                    }
                    Some(LaunchMode::Cloud) => match config.cloud_origin.clone() {
                        Some(raw_origin) => match normalize_cloud_origin(&raw_origin) {
                            Ok(origin) => {
                                startup_cloud_origin = Some(origin);
                                RuntimeInner {
                                    config,
                                    active_mode: Some(LaunchMode::Cloud),
                                    state: ActiveRuntimeState::SetupRequired,
                                }
                            }
                            Err(err) => {
                                let paired = error_dispatcher
                                    .dispatch_startup(ErrorCode::CloudInvalidOrigin);
                                let diagnostic = diagnostic_from_paired(
                                    &paired,
                                    Some(err.to_string()),
                                    Some(raw_origin),
                                );
                                RuntimeInner {
                                    config,
                                    active_mode: Some(LaunchMode::Cloud),
                                    state: ActiveRuntimeState::StartupFailed {
                                        diagnostic,
                                        config_corrupted: false,
                                    },
                                }
                            }
                        },
                        None => {
                            let paired =
                                error_dispatcher.dispatch_startup(ErrorCode::CloudInvalidOrigin);
                            let diagnostic = diagnostic_from_paired(
                                &paired,
                                Some("missing cloud_origin in cloud mode config".to_owned()),
                                None,
                            );
                            RuntimeInner {
                                config,
                                active_mode: Some(LaunchMode::Cloud),
                                state: ActiveRuntimeState::StartupFailed {
                                    diagnostic,
                                    config_corrupted: false,
                                },
                            }
                        }
                    },
                }
            }
        };

        let initial_concurrency = inner.config.upload_concurrency;
        let initial_locale = error_dispatcher.locale();
        let can_import_books = capabilities.can_import_books;
        let retain_completed_uploads = capabilities.retain_completed_uploads;
        let epub_cache_max_bytes = capabilities.epub_cache_max_bytes;

        let runtime = Arc::new(Self {
            config_store,
            credential_store,
            cloud_temp_dir,
            capabilities,
            pdfium,
            error_dispatcher,
            lifecycle_lock: AsyncMutex::new(()),
            inner: RwLock::new(inner),
            self_weak: OnceLock::new(),
            upload_manager: OnceLock::new(),
            epub_sessions: EpubSessionManager::new(epub_cache_max_bytes),
        });
        let _ = runtime.self_weak.set(Arc::downgrade(&runtime));

        let weak_for_uploads = Arc::downgrade(&runtime);
        let upload_manager = UploadManager::new(
            initial_concurrency,
            initial_locale,
            can_import_books,
            retain_completed_uploads,
            Arc::clone(&runtime.error_dispatcher),
            source_opener,
            Arc::new(move || {
                weak_for_uploads
                    .upgrade()
                    .ok_or(ErrorCode::RuntimeNotReady)
                    .and_then(|rt| rt.ready_import_target())
            }),
        );
        let _ = runtime.upload_manager.set(upload_manager);

        if let Some(origin) = startup_cloud_origin {
            runtime.activate_cloud_origin(origin, true).await;
        }

        runtime
    }

    pub fn upload_manager(&self) -> &UploadManager {
        self.upload_manager
            .get()
            .expect("upload_manager initialized during ClientRuntime::initialize")
    }

    pub fn capabilities(&self) -> &PlatformCapabilities {
        &self.capabilities
    }

    pub fn error_dispatcher(&self) -> &Arc<ErrorDispatcher> {
        &self.error_dispatcher
    }

    pub fn locale(&self) -> AppLocale {
        self.error_dispatcher.locale()
    }

    pub fn active_mode(&self) -> Option<LaunchMode> {
        self.inner
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .active_mode
    }

    pub fn bootstrap_state(&self) -> BootstrapStateDto {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let configured_root = guard.config.effective_local_root(&self.capabilities);
        let (phase, active_library_root, startup_error, cloud) = match &guard.state {
            ActiveRuntimeState::SetupRequired => (StartupPhase::SetupRequired, None, None, None),
            ActiveRuntimeState::LocalReady(library) => (
                StartupPhase::Ready,
                Some(library.root_dir().display().to_string()),
                None,
                None,
            ),
            ActiveRuntimeState::StartupFailed { diagnostic, .. } => (
                StartupPhase::StartupFailed,
                None,
                Some(diagnostic.clone()),
                None,
            ),
            ActiveRuntimeState::Cloud(ctx) => (
                ctx.phase,
                None,
                None,
                Some(CloudStatusDto {
                    origin: ctx.client.origin().to_owned(),
                    user: ctx.client.current_user(),
                    has_saved_session: ctx.client.has_saved_session(),
                    protocol_version: ctx.server_meta.as_ref().map(|m| m.api_version),
                    diagnostic: ctx.diagnostic.clone(),
                }),
            ),
        };

        let mode_changed = guard.active_mode.is_some() && guard.config.mode != guard.active_mode;
        let local_root_changed = matches!(guard.state, ActiveRuntimeState::LocalReady(_))
            && active_library_root.as_deref() != Some(configured_root.to_string_lossy().as_ref());
        let cloud_origin_changed = matches!(&guard.state, ActiveRuntimeState::Cloud(ctx) if guard.config.cloud_origin.as_deref() != Some(ctx.client.origin()));

        BootstrapStateDto {
            phase,
            active_mode: guard.active_mode,
            configured_mode: guard.config.mode,
            pending_restart: mode_changed || local_root_changed || cloud_origin_changed,
            locale: guard.config.locale,
            theme: guard.config.theme,
            configured_library_root: configured_root.display().to_string(),
            default_library_root: self.capabilities.default_library_root.display().to_string(),
            cloud_origin: guard.config.cloud_origin.clone(),
            can_pick_library_dir: self.capabilities.can_pick_library_dir,
            can_import_books: self.capabilities.can_import_books,
            cover_url_base: self.capabilities.cover_url_base.clone(),
            startup_error,
            cloud,
            buffered_errors: self.error_dispatcher.buffered_snapshot(),
        }
    }

    /// Completes first-launch setup, persists settings atomically, fixes launch mode for this
    /// process, and initializes the selected runtime.
    pub async fn complete_initial_setup(
        &self,
        request: InitialSetupRequest,
    ) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;

        let base_config = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            let allowed = match &guard.state {
                ActiveRuntimeState::SetupRequired => true,
                ActiveRuntimeState::StartupFailed {
                    config_corrupted: true,
                    ..
                } => true,
                ActiveRuntimeState::StartupFailed { diagnostic, .. } => {
                    diagnostic.code == ErrorCode::CloudInvalidOrigin.code()
                }
                _ => false,
            };
            if !allowed {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::LaunchModeLocked));
            }
            guard.config.clone()
        };

        let mut next_config = base_config;
        next_config.locale = request.locale;
        next_config.mode = Some(request.mode);

        if let Some(root_raw) = request.local_library_root {
            let trimmed = root_raw.trim();
            if !trimmed.is_empty() {
                if !self.capabilities.can_pick_library_dir {
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::FolderPickerNotImplemented));
                }
                next_config.local_library_root = Some(PathBuf::from(trimmed));
            }
        }

        if let Some(origin_raw) = request.cloud_origin {
            let trimmed = origin_raw.trim();
            if !trimmed.is_empty() {
                match normalize_cloud_origin(trimmed) {
                    Ok(normalized) => next_config.cloud_origin = Some(normalized),
                    Err(_) => {
                        return Err(self
                            .error_dispatcher
                            .dispatch_operation(ErrorCode::CloudInvalidOrigin));
                    }
                }
            }
        }

        if request.mode == LaunchMode::Cloud && next_config.cloud_origin.is_none() {
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::CloudInvalidOrigin));
        }

        match request.mode {
            LaunchMode::Local => {
                let effective_root = next_config.effective_local_root(&self.capabilities);
                let library = match LocalLibrary::open(effective_root.clone(), self.pdfium.clone())
                    .await
                {
                    Ok(lib) => Arc::new(lib),
                    Err(err) => {
                        let code = classify_local_startup_error(&err);
                        warn!(root = %effective_root.display(), error = %err, "initial local setup failed");
                        return Err(self.error_dispatcher.dispatch_operation(code));
                    }
                };

                if let Err(err) = self.config_store.save_atomic(&next_config) {
                    error!(error = %err, "failed to save initial client config");
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::ConfigSaveFailed));
                }

                let applied_locale = next_config.locale;
                {
                    let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                    guard.config = next_config;
                    guard.active_mode = Some(LaunchMode::Local);
                    guard.state = ActiveRuntimeState::LocalReady(library);
                    self.epub_sessions.clear_all();
                }
                self.error_dispatcher.set_locale(applied_locale);
                self.upload_manager().relocalize_errors(applied_locale);
                self.error_dispatcher.clear_buffer();
                Ok(self.bootstrap_state())
            }
            LaunchMode::Cloud => {
                let origin = next_config.cloud_origin.clone().ok_or_else(|| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::CloudInvalidOrigin)
                })?;

                self.probe_candidate_origin(&origin)
                    .await
                    .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;

                if let Err(err) = self.config_store.save_atomic(&next_config) {
                    error!(error = %err, "failed to save initial cloud client config");
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::ConfigSaveFailed));
                }

                let applied_locale = next_config.locale;
                {
                    let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                    guard.config = next_config;
                    guard.active_mode = Some(LaunchMode::Cloud);
                }
                self.error_dispatcher.set_locale(applied_locale);
                self.upload_manager().relocalize_errors(applied_locale);
                self.error_dispatcher.clear_buffer();
                self.activate_cloud_origin(origin, false).await;
                Ok(self.bootstrap_state())
            }
        }
    }

    /// Immediately updates the active UI & Rust error locale and persists it if configured.
    pub fn set_locale(&self, locale: AppLocale) -> Result<BootstrapStateDto, CommandErrorPayload> {
        self.error_dispatcher.set_locale(locale);
        self.upload_manager().relocalize_errors(locale);

        let config_to_save = {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config.locale = locale;
            let config_corrupted = matches!(
                guard.state,
                ActiveRuntimeState::StartupFailed {
                    config_corrupted: true,
                    ..
                }
            );
            match &mut guard.state {
                ActiveRuntimeState::StartupFailed { diagnostic, .. } => {
                    relocalize_diagnostic(diagnostic, locale);
                }
                ActiveRuntimeState::Cloud(ctx) => {
                    if let Some(diagnostic) = &mut ctx.diagnostic {
                        relocalize_diagnostic(diagnostic, locale);
                    }
                }
                _ => {}
            }
            if !config_corrupted && (self.config_store.exists() || guard.config.mode.is_some()) {
                Some(guard.config.clone())
            } else {
                None
            }
        };

        if let Some(config) = config_to_save
            && let Err(err) = self.config_store.save_atomic(&config)
        {
            error!(error = %err, "failed to persist locale change");
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed));
        }

        Ok(self.bootstrap_state())
    }

    /// Immediately updates the UI color theme and persists it if configured.
    pub fn set_theme(&self, theme: AppTheme) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let config_to_save = {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config.theme = theme;
            let config_corrupted = matches!(
                guard.state,
                ActiveRuntimeState::StartupFailed {
                    config_corrupted: true,
                    ..
                }
            );
            (!config_corrupted && (self.config_store.exists() || guard.config.mode.is_some()))
                .then(|| guard.config.clone())
        };

        if let Some(config) = config_to_save
            && let Err(err) = self.config_store.save_atomic(&config)
        {
            error!(error = %err, "failed to persist theme change");
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed));
        }

        Ok(self.bootstrap_state())
    }

    /// Updates persisted settings without hot-switching the current process launch mode.
    pub async fn update_settings(
        &self,
        request: UpdateSettingsRequest,
    ) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;

        let (prev_mode, prev_origin, mut next_config) = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            if matches!(
                guard.state,
                ActiveRuntimeState::StartupFailed {
                    config_corrupted: true,
                    ..
                }
            ) {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::ConfigInvalid));
            }
            (
                guard.config.mode,
                guard.config.cloud_origin.clone(),
                guard.config.clone(),
            )
        };

        if let Some(locale) = request.locale {
            next_config.locale = locale;
        }

        if let Some(mode) = request.mode {
            next_config.mode = Some(mode);
        }

        if let Some(root_raw) = request.local_library_root {
            if !self.capabilities.can_pick_library_dir {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::FolderPickerNotImplemented));
            }
            let trimmed = root_raw.trim();
            if trimmed.is_empty() {
                next_config.local_library_root = None;
            } else {
                next_config.local_library_root = Some(PathBuf::from(trimmed));
            }
        }

        let cloud_origin_requested = request.cloud_origin.is_some();
        if let Some(origin_raw) = request.cloud_origin {
            let trimmed = origin_raw.trim();
            if trimmed.is_empty() {
                next_config.cloud_origin = None;
            } else {
                match normalize_cloud_origin(trimmed) {
                    Ok(normalized) => next_config.cloud_origin = Some(normalized),
                    Err(_) => {
                        return Err(self
                            .error_dispatcher
                            .dispatch_operation(ErrorCode::CloudInvalidOrigin));
                    }
                }
            }
        }

        if next_config.mode == Some(LaunchMode::Cloud) {
            let Some(origin) = next_config.cloud_origin.as_deref() else {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::CloudInvalidOrigin));
            };
            let switching_or_changing_cloud = prev_mode != Some(LaunchMode::Cloud)
                || cloud_origin_requested
                || next_config.cloud_origin != prev_origin;
            if switching_or_changing_cloud {
                self.probe_candidate_origin(origin)
                    .await
                    .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;
            }
        }

        if let Err(err) = self.config_store.save_atomic(&next_config) {
            error!(error = %err, "failed to persist updated settings");
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed));
        }

        let applied_locale = request.locale;
        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config = next_config;
            if let Some(locale) = applied_locale {
                match &mut guard.state {
                    ActiveRuntimeState::StartupFailed { diagnostic, .. } => {
                        relocalize_diagnostic(diagnostic, locale);
                    }
                    ActiveRuntimeState::Cloud(ctx) => {
                        if let Some(diagnostic) = &mut ctx.diagnostic {
                            relocalize_diagnostic(diagnostic, locale);
                        }
                    }
                    _ => {}
                }
            }
        }
        if let Some(locale) = applied_locale {
            self.error_dispatcher.set_locale(locale);
            self.upload_manager().relocalize_errors(locale);
        }

        Ok(self.bootstrap_state())
    }

    /// Recovers from a broken/corrupted local library root on desktop by validating and opening
    /// a user-chosen replacement directory before persisting the repaired configuration.
    pub async fn repair_local_library_root(
        &self,
        new_root: PathBuf,
    ) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;

        if !self.capabilities.can_pick_library_dir {
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::FolderPickerNotImplemented));
        }

        let mut next_config = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            if !matches!(guard.state, ActiveRuntimeState::StartupFailed { .. }) {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::LaunchModeLocked));
            }
            guard.config.clone()
        };

        let library = match LocalLibrary::open(new_root.clone(), self.pdfium.clone()).await {
            Ok(lib) => Arc::new(lib),
            Err(err) => {
                let code = classify_local_startup_error(&err);
                warn!(root = %new_root.display(), error = %err, "repair local library root failed");
                return Err(self.error_dispatcher.dispatch_operation(code));
            }
        };

        next_config.mode = Some(LaunchMode::Local);
        next_config.local_library_root = Some(new_root);

        if let Err(err) = self.config_store.save_atomic(&next_config) {
            error!(error = %err, "failed to save repaired library root");
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed));
        }

        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config = next_config;
            guard.active_mode = Some(LaunchMode::Local);
            guard.state = ActiveRuntimeState::LocalReady(library);
            self.epub_sessions.clear_all();
        }
        self.error_dispatcher.clear_buffer();

        Ok(self.bootstrap_state())
    }

    /// Retries opening the configured local library root or probing cloud mode from failure states.
    pub async fn retry_startup(&self) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;

        enum RetryAction {
            CloudProbe,
            ReturnCurrent,
            Proceed { config_corrupted: bool },
        }

        let action = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            match &guard.state {
                ActiveRuntimeState::StartupFailed {
                    config_corrupted, ..
                } => RetryAction::Proceed {
                    config_corrupted: *config_corrupted,
                },
                ActiveRuntimeState::Cloud(_) => RetryAction::CloudProbe,
                _ => RetryAction::ReturnCurrent,
            }
        };

        let config_corrupted = match action {
            RetryAction::CloudProbe => return self.cloud_probe_locked().await,
            RetryAction::ReturnCurrent => return Ok(self.bootstrap_state()),
            RetryAction::Proceed { config_corrupted } => config_corrupted,
        };

        if config_corrupted {
            match self.config_store.load() {
                Ok(Some(cfg)) => {
                    self.error_dispatcher.set_locale(cfg.locale);
                    self.upload_manager().relocalize_errors(cfg.locale);
                    let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                    guard.config = cfg;
                }
                Ok(None) => {
                    {
                        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                        guard.active_mode = None;
                        guard.state = ActiveRuntimeState::SetupRequired;
                        self.epub_sessions.clear_all();
                    }
                    self.error_dispatcher.clear_buffer();
                    return Ok(self.bootstrap_state());
                }
                Err(_) => {
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::ConfigInvalid));
                }
            }
        }

        let config_snapshot = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            guard.config.clone()
        };

        match config_snapshot.mode {
            None => {
                {
                    let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                    guard.active_mode = None;
                    guard.state = ActiveRuntimeState::SetupRequired;
                    self.epub_sessions.clear_all();
                }
                self.error_dispatcher.clear_buffer();
                Ok(self.bootstrap_state())
            }
            Some(LaunchMode::Cloud) => {
                let normalized = match config_snapshot.cloud_origin.as_deref() {
                    Some(raw_origin) => match normalize_cloud_origin(raw_origin) {
                        Ok(origin) => origin,
                        Err(err) => {
                            let paired = self
                                .error_dispatcher
                                .dispatch_startup(ErrorCode::CloudInvalidOrigin);
                            let diagnostic = diagnostic_from_paired(
                                &paired,
                                Some(err.to_string()),
                                Some(raw_origin.to_owned()),
                            );
                            let mut guard =
                                self.inner.write().unwrap_or_else(|e| e.into_inner());
                            guard.active_mode = Some(LaunchMode::Cloud);
                            guard.state = ActiveRuntimeState::StartupFailed {
                                diagnostic,
                                config_corrupted: false,
                            };
                            return Err(paired.command);
                        }
                    },
                    None => {
                        let paired = self
                            .error_dispatcher
                            .dispatch_startup(ErrorCode::CloudInvalidOrigin);
                        let diagnostic = diagnostic_from_paired(
                            &paired,
                            Some("missing cloud_origin in cloud mode config".to_owned()),
                            None,
                        );
                        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                        guard.active_mode = Some(LaunchMode::Cloud);
                        guard.state = ActiveRuntimeState::StartupFailed {
                            diagnostic,
                            config_corrupted: false,
                        };
                        return Err(paired.command);
                    }
                };
                {
                    let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                    guard.active_mode = Some(LaunchMode::Cloud);
                }
                self.error_dispatcher.clear_buffer();
                self.activate_cloud_origin(normalized, false).await;
                Ok(self.bootstrap_state())
            }
            Some(LaunchMode::Local) => {
                let target_root = config_snapshot.effective_local_root(&self.capabilities);

                match LocalLibrary::open(target_root.clone(), self.pdfium.clone()).await {
                    Ok(library) => {
                        {
                            let mut guard =
                                self.inner.write().unwrap_or_else(|e| e.into_inner());
                            guard.active_mode = Some(LaunchMode::Local);
                            guard.state = ActiveRuntimeState::LocalReady(Arc::new(library));
                            self.epub_sessions.clear_all();
                        }
                        self.error_dispatcher.clear_buffer();
                        Ok(self.bootstrap_state())
                    }
                    Err(err) => {
                        let code = classify_local_startup_error(&err);
                        let paired = self.error_dispatcher.dispatch_startup(code);
                        let diagnostic = diagnostic_from_paired(
                            &paired,
                            Some(err.to_string()),
                            Some(target_root.display().to_string()),
                        );
                        {
                            let mut guard =
                                self.inner.write().unwrap_or_else(|e| e.into_inner());
                            guard.state = ActiveRuntimeState::StartupFailed {
                                diagnostic,
                                config_corrupted: false,
                            };
                            self.epub_sessions.clear_all();
                        }
                        Err(paired.command)
                    }
                }
            }
        }
    }

    // -------------------------------------------------------------------------
    // Cloud Lifecycle & Authentication
    // -------------------------------------------------------------------------

    async fn activate_cloud_origin(&self, origin: String, emit_startup_events: bool) {
        self.epub_sessions.clear_all();
        let on_invalidated = self.self_weak.get().cloned().map(|weak| {
            Arc::new(move || {
                if let Some(rt) = weak.upgrade() {
                    rt.handle_cloud_session_invalidated();
                }
            }) as Arc<dyn Fn() + Send + Sync>
        });

        let client = match CloudClient::new(
            origin.clone(),
            self.cloud_temp_dir.clone(),
            Arc::clone(&self.credential_store),
            on_invalidated,
        ) {
            Ok(c) => Arc::new(c),
            Err(err) => {
                let paired = self
                    .error_dispatcher
                    .dispatch_startup(ErrorCode::CloudInvalidOrigin);
                let diagnostic =
                    diagnostic_from_paired(&paired, Some(err.to_string()), Some(origin));
                let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                guard.state = ActiveRuntimeState::StartupFailed {
                    diagnostic,
                    config_corrupted: false,
                };
                self.epub_sessions.clear_all();
                return;
            }
        };

        let mut credential_diagnostic: Option<StartupDiagnostic> = None;
        if let Err(code) = client.load_saved_credentials() {
            warn!(origin = %origin, code = code.code(), "failed to load encrypted cloud credentials");
            let paired = if emit_startup_events {
                self.error_dispatcher.dispatch_startup(code)
            } else {
                PairedClientError::new(code, self.locale(), Uuid::new_v4().to_string(), None)
            };
            credential_diagnostic =
                Some(diagnostic_from_paired(&paired, None, Some(origin.clone())));
        }

        let ctx = self
            .probe_cloud_client(client, credential_diagnostic, emit_startup_events)
            .await;

        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.active_mode = Some(LaunchMode::Cloud);
            guard.state = ActiveRuntimeState::Cloud(ctx);
            self.epub_sessions.clear_all();
        }
    }

    async fn probe_cloud_client(
        &self,
        client: Arc<CloudClient>,
        initial_diagnostic: Option<StartupDiagnostic>,
        emit_errors: bool,
    ) -> CloudRuntimeContext {
        let make_diag = |code: ErrorCode, detail: Option<String>, path: Option<String>| {
            let paired = if emit_errors {
                self.error_dispatcher.dispatch_startup(code)
            } else {
                PairedClientError::new(code, self.locale(), Uuid::new_v4().to_string(), None)
            };
            diagnostic_from_paired(&paired, detail, path)
        };

        match client.probe().await {
            CloudProbeOutcome::ProtocolIncompatible { detail } => {
                let diagnostic = make_diag(
                    ErrorCode::CloudProtocolIncompatible,
                    Some(detail),
                    Some(client.origin().to_owned()),
                );
                CloudRuntimeContext {
                    client,
                    phase: StartupPhase::CloudProtocolIncompatible,
                    server_meta: None,
                    diagnostic: Some(diagnostic),
                }
            }
            CloudProbeOutcome::Unreachable { code, detail } => {
                let diagnostic = make_diag(code, Some(detail), Some(client.origin().to_owned()));
                CloudRuntimeContext {
                    client,
                    phase: StartupPhase::CloudUnreachable,
                    server_meta: None,
                    diagnostic: Some(diagnostic),
                }
            }
            CloudProbeOutcome::Compatible(meta) => {
                if !client.has_saved_session() {
                    return CloudRuntimeContext {
                        client,
                        phase: StartupPhase::CloudAuthRequired,
                        server_meta: Some(meta),
                        diagnostic: initial_diagnostic,
                    };
                }

                match client.verify_session().await {
                    SessionVerifyOutcome::Valid(_) => CloudRuntimeContext {
                        client,
                        phase: StartupPhase::Ready,
                        server_meta: Some(meta),
                        diagnostic: None,
                    },
                    SessionVerifyOutcome::Invalidated(code) => {
                        let diagnostic = make_diag(code, None, Some(client.origin().to_owned()));
                        CloudRuntimeContext {
                            client,
                            phase: StartupPhase::CloudAuthRequired,
                            server_meta: Some(meta),
                            diagnostic: Some(diagnostic),
                        }
                    }
                    SessionVerifyOutcome::TransientFailure { code, detail } => {
                        let diagnostic =
                            make_diag(code, Some(detail), Some(client.origin().to_owned()));
                        CloudRuntimeContext {
                            client,
                            phase: StartupPhase::CloudUnreachable,
                            server_meta: Some(meta),
                            diagnostic: Some(diagnostic),
                        }
                    }
                }
            }
        }
    }

    fn handle_cloud_session_invalidated(&self) {
        self.upload_manager().abort_all();
        let updated = {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            let updated = if let ActiveRuntimeState::Cloud(ctx) = &mut guard.state {
                let paired = PairedClientError::new(
                    ErrorCode::AuthenticationInvalidToken,
                    self.locale(),
                    Uuid::new_v4().to_string(),
                    None,
                );
                ctx.phase = StartupPhase::CloudAuthRequired;
                ctx.diagnostic = Some(diagnostic_from_paired(
                    &paired,
                    None,
                    Some(ctx.client.origin().to_owned()),
                ));
                true
            } else {
                false
            };
            self.epub_sessions.clear_all();
            updated
        };
        if updated {
            self.error_dispatcher
                .emit_runtime_state(&self.bootstrap_state());
        }
    }

    /// Validates URL format and probes a candidate cloud server's `/api/v1/meta` endpoint
    /// without mutating settings or emitting error toast events.
    pub async fn check_cloud_origin(&self, raw_origin: String) -> CloudOriginCheckResult {
        let locale = self.locale();
        let make_err = |code: ErrorCode| CloudOriginCheckResult {
            ok: false,
            message: Some(code.localize(locale).to_owned()),
        };

        let normalized = match normalize_cloud_origin(&raw_origin) {
            Ok(origin) => origin,
            Err(_) => return make_err(ErrorCode::CloudInvalidOrigin),
        };

        match self.probe_candidate_origin(&normalized).await {
            Ok(_) => CloudOriginCheckResult {
                ok: true,
                message: None,
            },
            Err(code) => make_err(code),
        }
    }

    async fn probe_candidate_origin(&self, origin: &str) -> Result<ServerMetaResponse, ErrorCode> {
        let client = CloudClient::new(
            origin.to_owned(),
            self.cloud_temp_dir.clone(),
            Arc::clone(&self.credential_store),
            None,
        )
        .map_err(|_| ErrorCode::CloudInvalidOrigin)?;

        match client.probe().await {
            CloudProbeOutcome::Compatible(meta) => Ok(meta),
            CloudProbeOutcome::ProtocolIncompatible { .. } => {
                Err(ErrorCode::CloudProtocolIncompatible)
            }
            CloudProbeOutcome::Unreachable { code, .. } => Err(code),
        }
    }

    /// Re-probes the active cloud server and verifies any saved session.
    pub async fn cloud_probe(&self) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.cloud_probe_locked().await
    }

    async fn cloud_probe_locked(&self) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let client = self.require_cloud_client()?;
        let ctx = self.probe_cloud_client(client, None, false).await;
        {
            let clear_sessions = ctx.phase != StartupPhase::Ready;
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.state = ActiveRuntimeState::Cloud(ctx);
            if clear_sessions {
                self.epub_sessions.clear_all();
            }
        }
        let state = self.bootstrap_state();
        self.error_dispatcher.emit_runtime_state(&state);
        Ok(state)
    }

    /// Updates the cloud server origin while staying in Cloud Mode (before authenticated ready
    /// state) and immediately re-probes the new origin. Launch mode remains fixed to Cloud.
    pub async fn cloud_update_origin(
        &self,
        new_origin: String,
    ) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.upload_manager().abort_all_and_drain().await;
        self.epub_sessions.clear_all();
        let normalized = normalize_cloud_origin(&new_origin).map_err(|_| {
            self.error_dispatcher
                .dispatch_operation(ErrorCode::CloudInvalidOrigin)
        })?;

        let mut next_config = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            if guard.active_mode != Some(LaunchMode::Cloud) {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::LaunchModeLocked));
            }
            if matches!(&guard.state, ActiveRuntimeState::Cloud(ctx) if ctx.phase == StartupPhase::Ready)
            {
                return Err(self
                    .error_dispatcher
                    .dispatch_operation(ErrorCode::LaunchModeLocked));
            }
            guard.config.clone()
        };

        self.probe_candidate_origin(&normalized)
            .await
            .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;

        next_config.cloud_origin = Some(normalized.clone());
        self.config_store.save_atomic(&next_config).map_err(|_| {
            self.error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed)
        })?;

        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config = next_config;
        }
        self.activate_cloud_origin(normalized, false).await;
        let state = self.bootstrap_state();
        self.error_dispatcher.emit_runtime_state(&state);
        Ok(state)
    }

    /// Signs into the active cloud server via Rust HTTP client; tokens stay strictly in Rust.
    pub async fn cloud_login(
        &self,
        username: String,
        password: String,
    ) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        let (client, server_meta) = {
            let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
            match &guard.state {
                ActiveRuntimeState::Cloud(ctx) => {
                    (Arc::clone(&ctx.client), ctx.server_meta.clone())
                }
                _ => {
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::RuntimeNotReady));
                }
            }
        };

        // Ensure protocol compatibility before sending credentials.
        let verified_meta = match server_meta {
            Some(meta) if meta.is_compatible() => meta,
            _ => match client.probe().await {
                CloudProbeOutcome::Compatible(meta) => meta,
                CloudProbeOutcome::ProtocolIncompatible { detail } => {
                    let paired = self
                        .error_dispatcher
                        .dispatch_startup(ErrorCode::CloudProtocolIncompatible);
                    {
                        let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
                        guard.state = ActiveRuntimeState::Cloud(CloudRuntimeContext {
                            client: Arc::clone(&client),
                            phase: StartupPhase::CloudProtocolIncompatible,
                            server_meta: None,
                            diagnostic: Some(diagnostic_from_paired(
                                &paired,
                                Some(detail),
                                Some(client.origin().to_owned()),
                            )),
                        });
                    }
                    self.error_dispatcher
                        .emit_runtime_state(&self.bootstrap_state());
                    return Err(paired.command);
                }
                CloudProbeOutcome::Unreachable { code, .. } => {
                    return Err(self.error_dispatcher.dispatch_operation(code));
                }
            },
        };

        client
            .login(username, password)
            .await
            .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;

        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.state = ActiveRuntimeState::Cloud(CloudRuntimeContext {
                client,
                phase: StartupPhase::Ready,
                server_meta: Some(verified_meta),
                diagnostic: None,
            });
            self.epub_sessions.clear_all();
        }

        let state = self.bootstrap_state();
        self.error_dispatcher.emit_runtime_state(&state);
        Ok(state)
    }

    /// Signs out of the active cloud server and wipes local encrypted credentials.
    pub async fn cloud_logout(&self) -> Result<BootstrapStateDto, CommandErrorPayload> {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.upload_manager().abort_all_and_drain().await;
        self.epub_sessions.clear_all();
        let client = self.require_cloud_client()?;
        client
            .logout()
            .await
            .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;

        {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            if let ActiveRuntimeState::Cloud(ctx) = &mut guard.state {
                ctx.phase = StartupPhase::CloudAuthRequired;
                ctx.diagnostic = None;
            }
            self.epub_sessions.clear_all();
        }
        let state = self.bootstrap_state();
        self.error_dispatcher.emit_runtime_state(&state);
        Ok(state)
    }

    /// Releases the active local SQLite pool and then flushes/truncates the WAL before shutdown.
    pub async fn shutdown(&self) {
        let _lifecycle = self.lifecycle_lock.lock().await;
        self.upload_manager().abort_all_and_drain().await;
        let db_path = {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            let prev = std::mem::replace(&mut guard.state, ActiveRuntimeState::SetupRequired);
            self.epub_sessions.clear_all();
            match prev {
                ActiveRuntimeState::LocalReady(lib) => {
                    let path = lib.db_path().to_path_buf();
                    drop(lib);
                    Some(path)
                }
                other => {
                    guard.state = other;
                    None
                }
            }
        };
        if let Some(path) = db_path
            && let Err(err) = checkpoint_library_wal(&path).await
        {
            warn!(error = %err, "WAL checkpoint on shutdown failed");
        }
    }

    fn require_cloud_client(&self) -> Result<Arc<CloudClient>, CommandErrorPayload> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        match &guard.state {
            ActiveRuntimeState::Cloud(ctx) => Ok(Arc::clone(&ctx.client)),
            ActiveRuntimeState::SetupRequired => Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::SetupRequired)),
            _ => Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::RuntimeNotReady)),
        }
    }

    fn require_ready_backend(&self) -> Result<LibraryBackend, CommandErrorPayload> {
        self.ready_backend()
            .map_err(|code| self.error_dispatcher.dispatch_operation(code))
    }

    fn require_ready_backend_with_session_token(
        &self,
    ) -> Result<(LibraryBackend, SessionGuardToken), CommandErrorPayload> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let backend = Self::ready_backend_from_state(&guard.state)
            .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;
        let token = self.epub_sessions.snapshot_token();
        Ok((backend, token))
    }

    pub fn ensure_library_ready(&self, operation_id: &str) -> Result<(), CommandErrorPayload> {
        self.ready_backend().map(|_| ()).map_err(|code| {
            self.error_dispatcher
                .dispatch(code, operation_id, None)
                .command
        })
    }

    fn ready_backend(&self) -> Result<LibraryBackend, ErrorCode> {
        let guard = self.inner.read().unwrap_or_else(|e| e.into_inner());
        Self::ready_backend_from_state(&guard.state)
    }

    fn ready_backend_from_state(state: &ActiveRuntimeState) -> Result<LibraryBackend, ErrorCode> {
        match state {
            ActiveRuntimeState::LocalReady(lib) => Ok(LibraryBackend::Local(Arc::clone(lib))),
            ActiveRuntimeState::Cloud(ctx) if ctx.phase == StartupPhase::Ready => {
                Ok(LibraryBackend::Cloud(Arc::clone(&ctx.client)))
            }
            ActiveRuntimeState::Cloud(ctx) => {
                let code = match ctx.phase {
                    StartupPhase::CloudProtocolIncompatible => ErrorCode::CloudProtocolIncompatible,
                    StartupPhase::CloudUnreachable => ErrorCode::CloudNetworkUnreachable,
                    _ => ErrorCode::CloudNotAuthenticated,
                };
                Err(code)
            }
            ActiveRuntimeState::SetupRequired => Err(ErrorCode::SetupRequired),
            ActiveRuntimeState::StartupFailed { .. } => Err(ErrorCode::RuntimeNotReady),
        }
    }

    // -------------------------------------------------------------------------
    // Unified Bookshelf & Trash Operations (Local & Cloud)
    // -------------------------------------------------------------------------

    pub async fn list_books(&self) -> Result<Vec<GetBookOutput>, CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .list_books(GetBooksQuery { ids: vec![] })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .list_books()
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn rename_book(&self, id: String, title: String) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .rename_book(RenameBookCommand { id, title })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .rename_book(id, title)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn delete_books(&self, ids: Vec<String>) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .delete_books(DeleteBooksCommand { ids: ids.clone() })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                })?,
            LibraryBackend::Cloud(cloud) => cloud
                .delete_books(ids.clone())
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
        }
        self.epub_sessions.evict_books(&ids);
        Ok(())
    }

    pub async fn list_trash(&self) -> Result<Vec<TrashedBookOutput>, CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .list_trashed_book()
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .list_trash()
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    /// Restores trashed books one by one, returning per-item outcomes and emitting at most
    /// one localized error event per failed item with matching `operation_id`/`item_id`.
    pub async fn restore_books(
        &self,
        ids: Vec<String>,
    ) -> Result<RestoreBooksResult, CommandErrorPayload> {
        let backend = self.require_ready_backend()?;
        let op = Uuid::new_v4().to_string();
        match backend {
            LibraryBackend::Local(lib) => {
                let mut service = lib.book_service();
                let mut restored_ids = Vec::with_capacity(ids.len());
                let mut failed = Vec::new();

                for id in ids {
                    match service.restore_book(RestoreBookIdCommand { id: id.clone() }).await {
                        Ok(()) => restored_ids.push(id),
                        Err(err) => {
                            let paired = self.error_dispatcher.dispatch(
                                ErrorCode::from(&err),
                                op.clone(),
                                Some(id.clone()),
                            );
                            failed.push(RestoreBooksItemResult {
                                book_id: id,
                                restored: false,
                                error: Some(paired.command),
                            });
                        }
                    }
                }

                Ok(RestoreBooksResult {
                    operation_id: op,
                    restored_ids,
                    failed,
                    skipped_ids: Vec::new(),
                })
            }
            LibraryBackend::Cloud(cloud) => {
                let mut restored_ids = Vec::with_capacity(ids.len());
                let mut failed = Vec::new();
                let mut pending = ids.into_iter();

                for id in pending.by_ref() {
                    match cloud.restore_book(&id).await {
                        Ok(()) => restored_ids.push(id),
                        Err(code) => {
                            let paired =
                                self.error_dispatcher
                                    .dispatch(code, op.clone(), Some(id.clone()));
                            failed.push(RestoreBooksItemResult {
                                book_id: id,
                                restored: false,
                                error: Some(paired.command),
                            });
                            if is_session_invalid_code(code) {
                                break;
                            }
                        }
                    }
                }

                Ok(RestoreBooksResult {
                    operation_id: op,
                    restored_ids,
                    failed,
                    skipped_ids: pending.collect(),
                })
            }
        }
    }

    pub async fn permanently_delete_books(
        &self,
        ids: Vec<String>,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .permanently_delete_books(PermanentlyDeleteBooksCommand { ids: ids.clone() })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                })?,
            LibraryBackend::Cloud(cloud) => cloud
                .permanently_delete_books(ids.clone())
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
        }
        self.epub_sessions.evict_books(&ids);
        Ok(())
    }

    // -------------------------------------------------------------------------
    // EPUB Reader Operations (Local & Cloud)
    // -------------------------------------------------------------------------

    async fn resolve_epub_byte_source(
        &self,
        backend: &LibraryBackend,
        book_id: &str,
    ) -> Result<(BookByteSource, blake3::Hash, u64), CommandErrorPayload> {
        match backend {
            LibraryBackend::Local(lib) => {
                let descriptor = lib
                    .book_service()
                    .get_book_file_meta(GetBookFileMetaCommand {
                        id: book_id.to_owned(),
                    })
                    .await
                    .map_err(|err| {
                        self.error_dispatcher
                            .dispatch_operation(ErrorCode::from(&err))
                    })?;
                if descriptor.file_type != BookFileType::EPUB {
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::ReaderUnsupportedFormat));
                }
                let source = BookByteSource::Local {
                    book_files: Arc::clone(lib.book_file_service()),
                    hash: descriptor.hash,
                    total_size: descriptor.size,
                };
                Ok((source, descriptor.hash, descriptor.size))
            }
            LibraryBackend::Cloud(cloud) => {
                let meta = cloud
                    .head_book_file(book_id)
                    .await
                    .map_err(|code| self.error_dispatcher.dispatch_operation(code))?;
                if meta.file_type != BookFileType::EPUB {
                    return Err(self
                        .error_dispatcher
                        .dispatch_operation(ErrorCode::ReaderUnsupportedFormat));
                }
                let source = BookByteSource::Cloud {
                    client: Arc::clone(cloud),
                    cache: Arc::clone(self.epub_sessions.chunk_cache()),
                    book_id: Arc::from(book_id),
                    hash: meta.hash,
                    total_size: meta.size,
                };
                Ok((source, meta.hash, meta.size))
            }
        }
    }

    pub async fn open_epub(
        &self,
        book_id: String,
    ) -> Result<OpenEpubResponseDto, CommandErrorPayload> {
        const MAX_EVICTION_RETRIES: usize = 2;

        let (backend, mut session_token) = self.require_ready_backend_with_session_token()?;
        let (mut source, mut hash, mut file_size) =
            self.resolve_epub_byte_source(&backend, &book_id).await?;
        let mut index = Arc::new(
            ZipIndex::parse(&source)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
        );
        let mut entries = index.export_entry_sizes();

        for attempt in 0..=MAX_EVICTION_RETRIES {
            match self.epub_sessions.try_insert(
                session_token,
                book_id.clone(),
                source.clone(),
                Arc::clone(&index),
            ) {
                Ok(session) => {
                    return Ok(OpenEpubResponseDto {
                        session_id: session.session_id,
                        book_id,
                        file_size,
                        entries,
                    });
                }
                Err(InsertSessionError::LifecycleChanged) => {
                    let code = self.ready_backend().err().unwrap_or(ErrorCode::RuntimeNotReady);
                    return Err(self.error_dispatcher.dispatch_operation(code));
                }
                Err(InsertSessionError::EvictionOccurred) => {
                    if attempt == MAX_EVICTION_RETRIES {
                        return Err(self
                            .error_dispatcher
                            .dispatch_operation(ErrorCode::RuntimeNotReady));
                    }
                    let (next_backend, next_token) =
                        self.require_ready_backend_with_session_token()?;
                    let (next_source, next_hash, next_size) =
                        self.resolve_epub_byte_source(&next_backend, &book_id).await?;
                    if next_hash != hash || next_size != file_size {
                        source = next_source;
                        hash = next_hash;
                        file_size = next_size;
                        index = Arc::new(
                            ZipIndex::parse(&source)
                                .await
                                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
                        );
                        entries = index.export_entry_sizes();
                    }
                    session_token = next_token;
                }
            }
        }
        unreachable!("loop always returns within MAX_EVICTION_RETRIES")
    }

    pub async fn read_epub_entry(
        &self,
        session_id: EpubSessionId,
        path: &str,
    ) -> Result<Bytes, CommandErrorPayload> {
        let Some(session) = self.epub_sessions.get(session_id) else {
            return Err(self
                .error_dispatcher
                .command_only(ErrorCode::ReaderSessionNotFound));
        };
        session
            .index
            .read_entry(&session.source, path)
            .await
            .map_err(|code| self.error_dispatcher.command_only(code))
    }

    pub async fn close_epub(&self, session_id: EpubSessionId) -> Result<(), CommandErrorPayload> {
        self.epub_sessions.remove(session_id);
        Ok(())
    }

    pub async fn get_cover_stream(
        &self,
        book_id: String,
    ) -> Result<(CoverByteStream, String), ErrorCode> {
        match self.ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .book_service()
                .get_cover(GetBookCoverCommand { id: book_id })
                .await
                .map_err(|err| ErrorCode::from(&err)),
            LibraryBackend::Cloud(cloud) => cloud.get_cover_stream(&book_id).await,
        }
    }

    /// Updates upload concurrency (`1..=16`), persists it in `settings.json`, and dynamically
    /// resizes the active upload worker pool.
    pub async fn set_upload_concurrency(
        &self,
        concurrency: usize,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        let clamped = clamp_upload_concurrency(concurrency);
        let config_to_save = {
            let mut guard = self.inner.write().unwrap_or_else(|e| e.into_inner());
            guard.config.upload_concurrency = clamped;
            let config_corrupted = matches!(
                guard.state,
                ActiveRuntimeState::StartupFailed {
                    config_corrupted: true,
                    ..
                }
            );
            (!config_corrupted && (self.config_store.exists() || guard.config.mode.is_some()))
                .then(|| guard.config.clone())
        };

        if let Some(config) = config_to_save
            && let Err(err) = self.config_store.save_atomic(&config)
        {
            error!(error = %err, "failed to persist upload concurrency change");
            return Err(self
                .error_dispatcher
                .dispatch_operation(ErrorCode::ConfigSaveFailed));
        }

        Ok(self.upload_manager().set_concurrency(clamped).await)
    }

    pub fn upload_snapshot(&self) -> UploadPanelSnapshotDto {
        self.upload_manager().snapshot()
    }

    pub async fn upload_enqueue(
        &self,
        items: Vec<EnqueueUploadItem>,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.upload_manager().enqueue(items).await
    }

    pub async fn upload_enqueue_paths(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.upload_manager().enqueue_paths(paths).await
    }

    pub async fn upload_retry_task(
        &self,
        task_id: u64,
    ) -> Result<UploadPanelSnapshotDto, CommandErrorPayload> {
        self.upload_manager().retry_task(task_id).await
    }

    pub async fn upload_cancel_task(&self, task_id: u64) -> UploadPanelSnapshotDto {
        self.upload_manager().cancel_task(task_id).await
    }

    pub async fn upload_cancel_all(&self) -> UploadPanelSnapshotDto {
        self.upload_manager().cancel_all_remaining().await
    }

    pub async fn upload_dismiss_task(&self, task_id: u64) -> UploadPanelSnapshotDto {
        self.upload_manager().dismiss_task(task_id).await
    }

    pub async fn upload_dismiss_failed(&self) -> UploadPanelSnapshotDto {
        self.upload_manager().dismiss_failed().await
    }

    pub async fn upload_clear_completed(&self) -> UploadPanelSnapshotDto {
        self.upload_manager().clear_completed().await
    }

    fn ready_import_target(&self) -> Result<ImportTarget, ErrorCode> {
        match self.ready_backend()? {
            LibraryBackend::Local(lib) => Ok(ImportTarget::Local(lib)),
            LibraryBackend::Cloud(client) => Ok(ImportTarget::Cloud {
                client,
                temp_dir: self.cloud_temp_dir.clone(),
            }),
        }
    }

    /// Streams a single book source into the active library (Local or Cloud) under `operation_id`.
    ///
    /// Emits a localized per-item error event on failure (except on expected cancellation).
    pub async fn import_stream<R>(
        &self,
        operation_id: &str,
        file_name: &str,
        reader: R,
        cancel_flag: Option<&CancellationToken>,
        on_progress: Option<ImportProgressCallback<'_>>,
    ) -> Result<GetBookOutput, CommandErrorPayload>
    where
        R: AsyncRead + Unpin + Send,
    {
        let result = match self.ready_backend() {
            Ok(LibraryBackend::Local(lib)) => {
                import_reader_into_local(&lib, file_name, reader, cancel_flag, on_progress).await
            }
            Ok(LibraryBackend::Cloud(cloud)) => {
                cloud
                    .import_reader_into_cloud(file_name, reader, cancel_flag, on_progress)
                    .await
            }
            Err(code) => Err(code),
        };

        match result {
            Ok(book) => Ok(book),
            Err(ErrorCode::ImportCancelled) => {
                let paired = PairedClientError::new(
                    ErrorCode::ImportCancelled,
                    self.locale(),
                    operation_id,
                    Some(file_name.to_owned()),
                );
                Err(paired.command)
            }
            Err(code) => Err(self
                .error_dispatcher
                .dispatch(code, operation_id, Some(file_name.to_owned()))
                .command),
        }
    }

    // -------------------------------------------------------------------------
    // Unified Tag Tree & Book-Tag Binding Operations (Local & Cloud)
    // -------------------------------------------------------------------------

    pub async fn get_tag_tree(&self) -> Result<Vec<TagTreeNodeDto>, CommandErrorPayload> {
        let nodes = match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib.tag_service().get_tree().await.map_err(|err| {
                self.error_dispatcher
                    .dispatch_operation(ErrorCode::from(&err))
            })?,
            LibraryBackend::Cloud(cloud) => cloud
                .get_tag_tree()
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
        };
        Ok(nodes.into_iter().map(TagTreeNodeDto::from).collect())
    }

    pub async fn define_tag(
        &self,
        name: String,
        parent_id: Option<String>,
        color: String,
        value_type: TagValueType,
    ) -> Result<TagDefinitionDto, CommandErrorPayload> {
        let (allows_value, value_regex) = value_type.to_value_rule();
        let output = match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .define_tag(CreateTagDefinitionCommand {
                    name,
                    parent_id,
                    color,
                    allows_value,
                    value_regex,
                })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                })?,
            LibraryBackend::Cloud(cloud) => cloud
                .define_tag(name, parent_id, color, allows_value, value_regex)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code))?,
        };
        Ok(TagDefinitionDto::from(output))
    }

    pub async fn rename_tag(&self, id: String, name: String) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .rename_tag(RenameTagCommand { id, name })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .rename_tag(id, name)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn change_tag_color(
        &self,
        id: String,
        color: String,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .change_tag_color(ChangeTagColorCommand { id, color })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .change_tag_color(id, color)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn move_tag(
        &self,
        id: String,
        parent_id: Option<String>,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .move_tag(MoveTagCommand { id, parent_id })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .move_tag(id, parent_id)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn undefine_tag(
        &self,
        id: String,
        strategy: DeleteTagStrategyDto,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .undefine_tag(UndefineTagCommand {
                    id,
                    strategy: strategy.into(),
                })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .undefine_tag(id, strategy)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn get_book_tags(
        &self,
        book_id: String,
    ) -> Result<Vec<BookTagOutput>, CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .get_book_tags(book_id)
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .get_book_tags(&book_id)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn bind_book_tag(
        &self,
        book_id: String,
        tag_id: String,
        value: Option<String>,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .bind_book_tag(BindBookTagCommand {
                    book_id,
                    tag_id,
                    value,
                })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .bind_book_tag(&book_id, tag_id, value)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }

    pub async fn unbind_book_tag(
        &self,
        book_id: String,
        tag_id: String,
    ) -> Result<(), CommandErrorPayload> {
        match self.require_ready_backend()? {
            LibraryBackend::Local(lib) => lib
                .tag_service()
                .unbind_book_tag(UnbindBookTagCommand { book_id, tag_id })
                .await
                .map_err(|err| {
                    self.error_dispatcher
                        .dispatch_operation(ErrorCode::from(&err))
                }),
            LibraryBackend::Cloud(cloud) => cloud
                .unbind_book_tag(&book_id, tag_id)
                .await
                .map_err(|code| self.error_dispatcher.dispatch_operation(code)),
        }
    }
}

fn diagnostic_from_paired(
    paired: &PairedClientError,
    detail: Option<String>,
    path: Option<String>,
) -> StartupDiagnostic {
    StartupDiagnostic {
        error_id: paired.event.error_id.clone(),
        code: paired.event.code,
        key: paired.event.key.clone(),
        message: paired.event.message.clone(),
        detail,
        path,
    }
}

fn relocalize_diagnostic(diagnostic: &mut StartupDiagnostic, locale: AppLocale) {
    if let Some(code) = ErrorCode::from_code(diagnostic.code) {
        diagnostic.message = code.localize(locale).to_owned();
    }
}

fn classify_local_startup_error(err: &anyhow::Error) -> ErrorCode {
    let text = format!("{err:#}").to_ascii_lowercase();
    if text.contains("corrupted")
        || text.contains("not a valid sqlite")
        || text.contains("database disk image is malformed")
        || text.contains("file is not a database")
        || text.contains("malformed database schema")
    {
        ErrorCode::LocalDatabaseCorrupted
    } else if text.contains("not a directory")
        || text.contains("not writable")
        || text.contains("cannot be empty")
        || text.contains("permission denied")
        || text.contains("read-only")
    {
        ErrorCode::LocalRootInvalid
    } else {
        ErrorCode::LocalStartupFailed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_local_startup_error_detects_native_sqlite_corruption() {
        for msg in [
            "sqlite error: database disk image is malformed",
            "failed to run migrations: file is not a database",
            "malformed database schema (books) - near syntax error",
            "library.db is not a valid SQLite database",
        ] {
            let err = anyhow::anyhow!(msg);
            assert_eq!(
                classify_local_startup_error(&err),
                ErrorCode::LocalDatabaseCorrupted,
                "expected corruption classification for: {msg}"
            );
        }

        assert_eq!(
            classify_local_startup_error(&anyhow::anyhow!("permission denied")),
            ErrorCode::LocalRootInvalid
        );
        assert_eq!(
            classify_local_startup_error(&anyhow::anyhow!("unexpected startup error")),
            ErrorCode::LocalStartupFailed
        );
    }
}
