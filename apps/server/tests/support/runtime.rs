use super::crypto;
use shiro_reader_server::{
    config::{Config, CryptoConfig, LogFormat, ServerConfig, TelemetryConfig},
    infrastructure::dependencies,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
};
use tokio::sync::OnceCell;

static SHARED_LIBS_DIR: OnceCell<PathBuf> = OnceCell::const_new();

pub async fn prepare_libs(root: &Path) {
    let shared_libs = SHARED_LIBS_DIR
        .get_or_init(|| async {
            let shared_root = shared_root_directory();
            dependencies::ensure_runtime_dependencies(&shared_root)
                .await
                .expect("ensure test runtime dependencies");
            let libs = dependencies::libs_dir(&shared_root);
            std::path::absolute(&libs).unwrap_or(libs)
        })
        .await;

    let target_libs = dependencies::libs_dir(root);
    if fs::symlink_metadata(&target_libs).is_ok() {
        return;
    }

    fs::create_dir_all(root).expect("create test root directory");
    link_libs_dir(shared_libs, &target_libs).expect("prepare test libs directory");
}

fn shared_root_directory() -> PathBuf {
    dotenvy::dotenv().ok();

    if let Ok(value) = env::var("ROOT_DIRECTORY") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            let configured = PathBuf::from(trimmed);
            if configured.is_absolute() {
                return configured;
            }
            let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .and_then(Path::parent)
                .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")));
            return workspace_root.join(configured);
        }
    }

    env::temp_dir().join("ShiroReader")
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

pub fn config(root: &Path) -> Config {
    Config {
        root_directory: root.into(),
        server: ServerConfig {
            host: "127.0.0.1".into(),
            port: 0,
            max_upload_size: 8 * 1024 * 1024,
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
            jwt_secret: String::from_utf8(crypto::SECRET.to_vec()).unwrap(),
            jwt_issuer: crypto::ISSUER.into(),
            jwt_audience: crypto::AUDIENCE.into(),
            jwt_access_ttl_secs: crypto::ACCESS_TTL,
            jwt_refresh_ttl_secs: crypto::REFRESH_TTL,
            jwt_validation_leeway_secs: 0,
        },
    }
}
