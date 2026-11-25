use std::{fs, io::Write, path::PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use shiro_error_catalog::AppLocale;
use tempfile::NamedTempFile;

pub const CONFIG_SCHEMA_VERSION: u32 = 1;
pub const SETTINGS_FILE_NAME: &str = "settings.json";
pub const DEFAULT_UPLOAD_CONCURRENCY: usize = 4;
pub const MAX_UPLOAD_CONCURRENCY: usize = 16;

pub const fn default_upload_concurrency() -> usize {
    DEFAULT_UPLOAD_CONCURRENCY
}

pub fn clamp_upload_concurrency(value: usize) -> usize {
    value.clamp(1, MAX_UPLOAD_CONCURRENCY)
}

/// Startup mode persisted in client settings and fixed for the lifetime of one app process.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    Local,
    Cloud,
}

/// UI color theme persisted in client settings and applied via `data-theme` in the UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    #[default]
    #[serde(alias = "lavender")]
    Default,
    Sakura,
    Mint,
    Sky,
    Peach,
}

/// Configurable log level for desktop clients (`settings.json`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientLogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl ClientLogLevel {
    /// Compile-time default log level: `debug` in debug builds, `info` in release builds.
    pub const fn compile_default() -> Self {
        if cfg!(debug_assertions) {
            Self::Debug
        } else {
            Self::Info
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// Platform-specific capabilities supplied by the host (Windows/Linux/macOS desktop vs Android).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformCapabilities {
    pub can_pick_library_dir: bool,
    pub can_configure_log_level: bool,
    pub can_import_books: bool,
    pub retain_completed_uploads: bool,
    pub default_library_root: PathBuf,
    pub cover_url_base: String,
    pub epub_cache_max_bytes: usize,
}

impl PlatformCapabilities {
    pub fn current_host(default_library_root: PathBuf) -> Self {
        #[cfg(target_os = "android")]
        let (
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            cover_url_base,
        ) = (
            false,
            false,
            true,
            true,
            "http://shiro-cover.localhost/book/".to_owned(),
        );

        #[cfg(target_os = "ios")]
        let (
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            cover_url_base,
        ) = (
            false,
            false,
            false,
            false,
            "shiro-cover://localhost/book/".to_owned(),
        );

        #[cfg(target_os = "windows")]
        let (
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            cover_url_base,
        ) = (
            true,
            true,
            true,
            false,
            "http://shiro-cover.localhost/book/".to_owned(),
        );

        #[cfg(target_os = "macos")]
        let (
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            cover_url_base,
        ) = (
            true,
            true,
            true,
            false,
            "shiro-cover://localhost/book/".to_owned(),
        );

        #[cfg(not(any(
            target_os = "android",
            target_os = "ios",
            target_os = "windows",
            target_os = "macos"
        )))]
        let (
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            cover_url_base,
        ) = (
            true,
            true,
            true,
            false,
            "shiro-cover://localhost/book/".to_owned(),
        );

        let epub_cache_max_bytes = if cfg!(target_os = "android") {
            24 * 1024 * 1024
        } else {
            64 * 1024 * 1024
        };

        Self {
            can_pick_library_dir,
            can_configure_log_level,
            can_import_books,
            retain_completed_uploads,
            default_library_root,
            cover_url_base,
            epub_cache_max_bytes,
        }
    }
}

/// Persistent client configuration kept strictly separate from library data.
///
/// Switching between local and cloud modes retains both `local_library_root` and `cloud_origin`
/// so neither mode's settings or data are wiped.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConfig {
    pub schema_version: u32,
    pub mode: Option<LaunchMode>,
    pub locale: AppLocale,
    #[serde(default)]
    pub theme: AppTheme,
    pub local_library_root: Option<PathBuf>,
    pub cloud_origin: Option<String>,
    #[serde(default)]
    pub log_level: Option<ClientLogLevel>,
    #[serde(default = "default_upload_concurrency")]
    pub upload_concurrency: usize,
}

impl ClientConfig {
    pub fn initial(detected_locale: AppLocale) -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            mode: None,
            locale: detected_locale,
            theme: AppTheme::default(),
            local_library_root: None,
            cloud_origin: None,
            log_level: None,
            upload_concurrency: DEFAULT_UPLOAD_CONCURRENCY,
        }
    }

    /// Resolves the effective local library root respecting platform rules:
    /// Android ALWAYS uses the fixed private app directory.
    pub fn effective_local_root(&self, capabilities: &PlatformCapabilities) -> PathBuf {
        if !capabilities.can_pick_library_dir {
            return capabilities.default_library_root.clone();
        }
        self.local_library_root
            .as_ref()
            .filter(|path| !path.as_os_str().is_empty())
            .cloned()
            .unwrap_or_else(|| capabilities.default_library_root.clone())
    }

    /// Resolves the effective log level respecting platform rules:
    /// mobile targets ALWAYS use the compile-time default log level.
    pub fn effective_log_level(&self, capabilities: &PlatformCapabilities) -> ClientLogLevel {
        if !capabilities.can_configure_log_level {
            return ClientLogLevel::compile_default();
        }
        self.log_level
            .unwrap_or_else(ClientLogLevel::compile_default)
    }
}

#[derive(Debug)]
pub struct ConfigLoadError {
    pub path: PathBuf,
    pub reason: String,
}

impl std::fmt::Display for ConfigLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "failed to load client config {}: {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for ConfigLoadError {}

/// Atomic file-backed persistence for [`ClientConfig`].
#[derive(Debug, Clone)]
pub struct ConfigStore {
    config_dir: PathBuf,
    settings_path: PathBuf,
}

impl ConfigStore {
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        let config_dir = config_dir.into();
        let settings_path = config_dir.join(SETTINGS_FILE_NAME);
        Self {
            config_dir,
            settings_path,
        }
    }

    pub fn exists(&self) -> bool {
        self.settings_path.exists()
    }

    /// Loads existing settings if present; returns `Ok(None)` on first launch.
    ///
    /// Never overwrites or clears a corrupted config file.
    pub fn load(&self) -> Result<Option<ClientConfig>, ConfigLoadError> {
        if !self.settings_path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&self.settings_path).map_err(|err| ConfigLoadError {
            path: self.settings_path.clone(),
            reason: err.to_string(),
        })?;
        let mut config: ClientConfig = serde_json::from_str(&raw).map_err(|err| ConfigLoadError {
            path: self.settings_path.clone(),
            reason: err.to_string(),
        })?;
        if config.schema_version != CONFIG_SCHEMA_VERSION {
            return Err(ConfigLoadError {
                path: self.settings_path.clone(),
                reason: format!("unsupported schema version {}", config.schema_version),
            });
        }
        config.upload_concurrency = clamp_upload_concurrency(config.upload_concurrency);
        Ok(Some(config))
    }

    /// Atomically writes settings via a temporary file in the same directory + rename.
    pub fn save_atomic(&self, config: &ClientConfig) -> anyhow::Result<()> {
        fs::create_dir_all(&self.config_dir).with_context(|| {
            format!(
                "failed to create config directory {}",
                self.config_dir.display()
            )
        })?;

        let mut normalized = config.clone();
        normalized.upload_concurrency = clamp_upload_concurrency(normalized.upload_concurrency);
        let serialized = serde_json::to_vec_pretty(&normalized)
            .context("failed to serialize client configuration")?;

        let mut temp = NamedTempFile::new_in(&self.config_dir).with_context(|| {
            format!(
                "failed to create temp config file in {}",
                self.config_dir.display()
            )
        })?;
        temp.write_all(&serialized)
            .context("failed to write temporary config file")?;
        temp.as_file()
            .sync_all()
            .context("failed to sync temporary config file")?;
        temp.persist(&self.settings_path).map_err(|err| {
            anyhow::anyhow!(
                "failed to persist config file {}: {}",
                self.settings_path.display(),
                err.error
            )
        })?;
        Ok(())
    }
}

/// Normalizes and validates a cloud server origin (`http://` or `https://`) via `url::Url::parse`.
pub fn normalize_cloud_origin(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        bail!("cloud server origin cannot be empty");
    }

    let parsed = url::Url::parse(trimmed)
        .context("cloud server origin must be a valid http(s)://host[:port] URL")?;

    if !matches!(parsed.scheme(), "http" | "https") {
        bail!("cloud server origin must use http:// or https://");
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        bail!("cloud server origin must not contain credentials");
    }
    if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
        bail!("cloud server origin must be a clean http(s)://host[:port] origin");
    }
    if parsed.host().is_none() {
        bail!("cloud server host cannot be empty");
    }
    if parsed.port() == Some(0) {
        bail!("cloud server port cannot be 0");
    }

    Ok(parsed.origin().ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_store_rejects_unsupported_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            r#"{"schemaVersion":2,"mode":"local","locale":"en","localLibraryRoot":null,"cloudOrigin":null}"#,
        )
        .unwrap();
        let err = store.load().unwrap_err();
        assert!(err.reason.contains("unsupported schema version 2"));
    }

    #[test]
    fn effective_local_root_respects_platform_capabilities() {
        let default_root = PathBuf::from("/default/library");
        let custom_root = PathBuf::from("/custom/library");

        let desktop = PlatformCapabilities {
            can_pick_library_dir: true,
            can_configure_log_level: true,
            can_import_books: true,
            retain_completed_uploads: false,
            default_library_root: default_root.clone(),
            cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
            epub_cache_max_bytes: 64 * 1024 * 1024,
        };
        let mut cfg = ClientConfig::initial(AppLocale::En);
        assert_eq!(cfg.effective_local_root(&desktop), default_root);
        assert_eq!(
            cfg.effective_log_level(&desktop),
            ClientLogLevel::compile_default()
        );
        assert_eq!(cfg.upload_concurrency, DEFAULT_UPLOAD_CONCURRENCY);

        cfg.local_library_root = Some(PathBuf::new());
        assert_eq!(cfg.effective_local_root(&desktop), default_root);

        cfg.local_library_root = Some(custom_root.clone());
        assert_eq!(cfg.effective_local_root(&desktop), custom_root);

        cfg.log_level = Some(ClientLogLevel::Trace);
        assert_eq!(cfg.effective_log_level(&desktop), ClientLogLevel::Trace);

        let android = PlatformCapabilities {
            can_pick_library_dir: false,
            can_configure_log_level: false,
            can_import_books: true,
            retain_completed_uploads: true,
            default_library_root: default_root.clone(),
            cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
            epub_cache_max_bytes: 24 * 1024 * 1024,
        };
        assert_eq!(cfg.effective_local_root(&android), default_root);
        assert_eq!(
            cfg.effective_log_level(&android),
            ClientLogLevel::compile_default()
        );
    }

    #[test]
    fn compile_default_matches_build_profile_and_deserializes_legacy_settings_without_log_level() {
        let expected_default = if cfg!(debug_assertions) {
            ClientLogLevel::Debug
        } else {
            ClientLogLevel::Info
        };
        assert_eq!(ClientLogLevel::compile_default(), expected_default);

        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path());
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            r#"{"schemaVersion":1,"mode":"local","locale":"en","localLibraryRoot":null,"cloudOrigin":null}"#,
        )
        .unwrap();
        let loaded = store.load().unwrap().expect("loaded config");
        assert_eq!(loaded.log_level, None);
        assert_eq!(loaded.theme, AppTheme::Default);
        assert_eq!(loaded.upload_concurrency, DEFAULT_UPLOAD_CONCURRENCY);
        assert_eq!(
            serde_json::from_str::<AppTheme>("\"lavender\"").unwrap(),
            AppTheme::Default
        );

        let desktop = PlatformCapabilities {
            can_pick_library_dir: true,
            can_configure_log_level: true,
            can_import_books: true,
            retain_completed_uploads: false,
            default_library_root: PathBuf::from("/default/library"),
            cover_url_base: "http://shiro-cover.localhost/book/".to_owned(),
            epub_cache_max_bytes: 64 * 1024 * 1024,
        };
        assert_eq!(loaded.effective_log_level(&desktop), expected_default);

        // Out-of-range upload_concurrency values are clamped on both load and save.
        fs::write(
            dir.path().join(SETTINGS_FILE_NAME),
            r#"{"schemaVersion":1,"mode":"local","locale":"en","localLibraryRoot":null,"cloudOrigin":null,"uploadConcurrency":0}"#,
        )
        .unwrap();
        assert_eq!(store.load().unwrap().unwrap().upload_concurrency, 1);

        let mut oversized = loaded;
        oversized.upload_concurrency = 99;
        store.save_atomic(&oversized).unwrap();
        assert_eq!(
            store.load().unwrap().unwrap().upload_concurrency,
            MAX_UPLOAD_CONCURRENCY
        );
    }
}
