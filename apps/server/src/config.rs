use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::{
    env,
    fs::{self, OpenOptions},
    io::Write,
    net::SocketAddr,
    path::{Path, PathBuf},
    str::FromStr,
};

const DEFAULT_CONFIG_FILE: &str = "config.toml";
const DEFAULT_HOST: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 80;
const DEFAULT_ROOT_DIRECTORY: &str = ".";
const DEFAULT_MAX_UPLOAD_SIZE: usize = shiro_http_contract::DEFAULT_MAX_UPLOAD_BYTES as usize;

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
    pub crypto: CryptoConfig,
    pub root_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_upload_size: usize,
}

#[derive(Debug, Clone)]
pub struct TelemetryConfig {
    pub log_format: LogFormat,
    pub log_level: String,
}

pub const fn default_log_level() -> &'static str {
    if cfg!(debug_assertions) {
        "debug"
    } else {
        "info"
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    Pretty,
    Json,
}

#[derive(Debug, Clone)]
pub struct CryptoConfig {
    /// Argon2 内存成本，单位为 KiB。
    pub argon2_memory_cost: u32,
    pub argon2_time_cost: u32,
    pub argon2_parallelism: u32,
    pub argon2_output_length: usize,
    pub jwt_secret: String,
    pub jwt_issuer: String,
    pub jwt_audience: String,
    /// 访问令牌有效期，单位为秒。
    pub jwt_access_ttl_secs: u64,
    /// 刷新令牌有效期，单位为秒。
    pub jwt_refresh_ttl_secs: u64,
    /// JWT 时间声明校验的宽限，单位为秒。
    pub jwt_validation_leeway_secs: u64,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileConfig {
    server: FileServerConfig,
    telemetry: FileTelemetryConfig,
    crypto: FileCryptoConfig,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileServerConfig {
    host: Option<String>,
    port: Option<u16>,
    max_upload_size: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileTelemetryConfig {
    log_format: Option<String>,
    log_level: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct FileCryptoConfig {
    argon2_memory_cost: Option<u32>,
    argon2_time_cost: Option<u32>,
    argon2_parallelism: Option<u32>,
    argon2_output_length: Option<usize>,
    jwt_secret: Option<String>,
    jwt_issuer: Option<String>,
    jwt_audience: Option<String>,
    jwt_access_ttl_secs: Option<u64>,
    jwt_refresh_ttl_secs: Option<u64>,
    jwt_validation_leeway_secs: Option<u64>,
}

#[derive(Serialize)]
struct GeneratedConfig {
    server: GeneratedServerConfig,
    crypto: GeneratedCryptoConfig,
}

#[derive(Serialize)]
struct GeneratedServerConfig {
    host: &'static str,
    port: u16,
}

#[derive(Serialize)]
struct GeneratedCryptoConfig {
    jwt_secret: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let root_directory = PathBuf::from(
            env::var("ROOT_DIRECTORY").unwrap_or_else(|_| DEFAULT_ROOT_DIRECTORY.into()),
        );
        Self::load_from_root_directory(root_directory, |key| env::var(key).ok())
    }

    fn load_from_root_directory(
        root_directory: PathBuf,
        env_value: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Self> {
        fs::create_dir_all(&root_directory).with_context(|| {
            format!(
                "failed to create root directory {}",
                root_directory.display()
            )
        })?;
        let config_path = root_directory.join(DEFAULT_CONFIG_FILE);
        let file_config = load_file(&config_path)?;
        Self::from_file(file_config, root_directory, env_value)
    }

    fn from_file(
        file: FileConfig,
        root_directory: PathBuf,
        env_value: impl Fn(&str) -> Option<String>,
    ) -> anyhow::Result<Self> {
        let jwt_secret = file
            .crypto
            .jwt_secret
            .context("config file did not initialize the JWT secret")?;
        let jwt_secret = env_value("JWT_SECRET").unwrap_or(jwt_secret);

        let server = ServerConfig {
            host: string_setting(&env_value, "HOST", file.server.host, DEFAULT_HOST),
            port: setting(&env_value, "PORT", file.server.port, DEFAULT_PORT)?,
            max_upload_size: setting(
                &env_value,
                "MAX_UPLOAD_SIZE",
                file.server.max_upload_size,
                DEFAULT_MAX_UPLOAD_SIZE,
            )?,
        };

        let log_format = string_setting(
            &env_value,
            "LOG_FORMAT",
            file.telemetry.log_format,
            "pretty",
        );
        let log_level = non_empty_setting(
            &env_value,
            "LOG_LEVEL",
            file.telemetry.log_level,
            default_log_level(),
        );
        let telemetry = TelemetryConfig {
            log_format: match log_format.as_str() {
                "json" => LogFormat::Json,
                _ => LogFormat::Pretty,
            },
            log_level,
        };

        let crypto = CryptoConfig {
            argon2_memory_cost: setting(
                &env_value,
                "ARGON2_MEMORY_COST_KIB",
                file.crypto.argon2_memory_cost,
                19456,
            )?,
            argon2_time_cost: setting(
                &env_value,
                "ARGON2_TIME_COST",
                file.crypto.argon2_time_cost,
                2,
            )?,
            argon2_parallelism: setting(
                &env_value,
                "ARGON2_PARALLELISM",
                file.crypto.argon2_parallelism,
                1,
            )?,
            argon2_output_length: setting(
                &env_value,
                "ARGON2_OUTPUT_LENGTH",
                file.crypto.argon2_output_length,
                32,
            )?,
            jwt_secret,
            jwt_issuer: string_setting(
                &env_value,
                "JWT_ISSUER",
                file.crypto.jwt_issuer,
                "shiro-reader",
            ),
            jwt_audience: string_setting(
                &env_value,
                "JWT_AUDIENCE",
                file.crypto.jwt_audience,
                "shiro-reader",
            ),
            jwt_access_ttl_secs: setting(
                &env_value,
                "JWT_ACCESS_TTL_SECS",
                file.crypto.jwt_access_ttl_secs,
                900,
            )?,
            jwt_refresh_ttl_secs: setting(
                &env_value,
                "JWT_REFRESH_TTL_SECS",
                file.crypto.jwt_refresh_ttl_secs,
                2592000,
            )?,
            jwt_validation_leeway_secs: setting(
                &env_value,
                "JWT_VALIDATION_LEEWAY_SECS",
                file.crypto.jwt_validation_leeway_secs,
                60,
            )?,
        };

        Ok(Self {
            server,
            telemetry,
            crypto,
            root_directory,
        })
    }
}

impl ServerConfig {
    pub fn socket_addr(&self) -> SocketAddr {
        format!("{}:{}", self.host, self.port)
            .parse()
            .expect("HOST and PORT must form a valid socket address")
    }
}

fn load_file(path: &Path) -> anyhow::Result<FileConfig> {
    match fs::read_to_string(path) {
        Ok(contents) => parse_file(path, &contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => create_default_file(path),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn create_default_file(path: &Path) -> anyhow::Result<FileConfig> {
    let contents = toml::to_string_pretty(&GeneratedConfig {
        server: GeneratedServerConfig {
            host: DEFAULT_HOST,
            port: DEFAULT_PORT,
        },
        crypto: GeneratedCryptoConfig {
            jwt_secret: generate_jwt_secret()?,
        },
    })?;

    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config directory {}", parent.display()))?;
    }

    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    match options.open(path) {
        Ok(mut file) => {
            file.write_all(contents.as_bytes())
                .with_context(|| format!("failed to write config file {}", path.display()))?;
            parse_file(path, &contents)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let contents = fs::read_to_string(path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            parse_file(path, &contents)
        }
        Err(error) => Err(error).with_context(|| format!("failed to create {}", path.display())),
    }
}

fn parse_file(path: &Path, contents: &str) -> anyhow::Result<FileConfig> {
    let mut config: FileConfig = toml::from_str(contents)
        .with_context(|| format!("failed to parse config file {}", path.display()))?;

    if config.crypto.jwt_secret.is_none() {
        let secret = generate_jwt_secret()?;
        persist_jwt_secret(path, contents, &secret)?;
        config.crypto.jwt_secret = Some(secret);
    }

    Ok(config)
}

fn persist_jwt_secret(path: &Path, contents: &str, secret: &str) -> anyhow::Result<()> {
    let mut document = contents
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("failed to update config file {}", path.display()))?;
    let root = document.as_table_mut();
    if let Some(crypto) = root.get_mut("crypto") {
        crypto
            .as_table_mut()
            .context("crypto section must be a table")?
            .insert("jwt_secret", toml_edit::value(secret));
    } else {
        let mut crypto = toml_edit::Table::new();
        crypto.insert("jwt_secret", toml_edit::value(secret));
        root.insert("crypto", crypto.into());
    }

    fs::write(path, document.to_string()).with_context(|| {
        format!(
            "failed to persist generated JWT secret in {}",
            path.display()
        )
    })
}

fn generate_jwt_secret() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).context("failed to generate JWT secret")?;

    let mut secret = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write;
        write!(&mut secret, "{byte:02x}").expect("writing to a String cannot fail");
    }
    Ok(secret)
}

fn string_setting(
    env_value: &impl Fn(&str) -> Option<String>,
    key: &str,
    file_value: Option<String>,
    default: &str,
) -> String {
    env_value(key)
        .or(file_value)
        .unwrap_or_else(|| default.to_owned())
}

fn non_empty_setting(
    env_value: &impl Fn(&str) -> Option<String>,
    key: &str,
    file_value: Option<String>,
    default: &str,
) -> String {
    env_value(key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            file_value
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| default.to_owned())
}

fn setting<T>(
    env_value: &impl Fn(&str) -> Option<String>,
    key: &str,
    file_value: Option<T>,
    default: T,
) -> anyhow::Result<T>
where
    T: FromStr,
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env_value(key) {
        Some(value) => value
            .parse()
            .with_context(|| format!("invalid value for environment variable {key}")),
        None => Ok(file_value.unwrap_or(default)),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{Config, LogFormat, ServerConfig};

    #[test]
    fn default_max_upload_size_is_512_mib_and_respects_env_override() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load_from_root_directory(dir.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(cfg.server.max_upload_size, 512 * 1024 * 1024);

        let custom_cfg = Config::load_from_root_directory(dir.path().to_path_buf(), |key| {
            (key == "MAX_UPLOAD_SIZE").then(|| (16 * 1024 * 1024).to_string())
        })
        .unwrap();
        assert_eq!(custom_cfg.server.max_upload_size, 16 * 1024 * 1024);
    }

    #[test]
    fn first_load_generates_config_file_with_hex_secret_and_restricted_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load_from_root_directory(dir.path().to_path_buf(), |_| None).unwrap();

        assert_eq!(cfg.crypto.jwt_secret.len(), 64);
        assert!(cfg.crypto.jwt_secret.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_eq!(cfg.telemetry.log_format, LogFormat::Pretty);

        let config_path = dir.path().join("config.toml");
        assert!(config_path.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(&config_path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let reloaded =
            Config::load_from_root_directory(dir.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(reloaded.crypto.jwt_secret, cfg.crypto.jwt_secret);
    }

    #[test]
    fn missing_jwt_secret_is_persisted_for_both_absent_and_existing_crypto_tables() {
        // Case 1: no [crypto] section in config.toml
        let dir_no_crypto = tempfile::tempdir().unwrap();
        let path_no_crypto = dir_no_crypto.path().join("config.toml");
        fs::write(&path_no_crypto, "[server]\nport = 8080\n").unwrap();

        let cfg1 =
            Config::load_from_root_directory(dir_no_crypto.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(cfg1.server.port, 8080);
        assert_eq!(cfg1.crypto.jwt_secret.len(), 64);
        let cfg1_again =
            Config::load_from_root_directory(dir_no_crypto.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(cfg1_again.crypto.jwt_secret, cfg1.crypto.jwt_secret);

        // Case 2: [crypto] section exists without jwt_secret
        let dir_with_crypto = tempfile::tempdir().unwrap();
        let path_with_crypto = dir_with_crypto.path().join("config.toml");
        fs::write(
            &path_with_crypto,
            "[server]\nhost = \"0.0.0.0\"\n[crypto]\njwt_issuer = \"custom-iss\"\n",
        )
        .unwrap();

        let cfg2 = Config::load_from_root_directory(dir_with_crypto.path().to_path_buf(), |_| None)
            .unwrap();
        assert_eq!(cfg2.server.host, "0.0.0.0");
        assert_eq!(cfg2.crypto.jwt_issuer, "custom-iss");
        assert_eq!(cfg2.crypto.jwt_secret.len(), 64);
        let cfg2_again =
            Config::load_from_root_directory(dir_with_crypto.path().to_path_buf(), |_| None)
                .unwrap();
        assert_eq!(cfg2_again.crypto.jwt_secret, cfg2.crypto.jwt_secret);
    }

    #[test]
    fn precedence_is_env_over_file_over_default_and_invalid_inputs_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let default_cfg =
            Config::load_from_root_directory(dir.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(default_cfg.telemetry.log_level, super::default_log_level());

        fs::write(
            dir.path().join("config.toml"),
            "[server]\nhost = \"10.0.0.1\"\nport = 9000\n[telemetry]\nlog_format = \"pretty\"\nlog_level = \"warn\"\n[crypto]\njwt_secret = \"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"\n",
        )
        .unwrap();

        let file_cfg =
            Config::load_from_root_directory(dir.path().to_path_buf(), |_| None).unwrap();
        assert_eq!(file_cfg.telemetry.log_level, "warn");

        let cfg = Config::load_from_root_directory(dir.path().to_path_buf(), |key| match key {
            "PORT" => Some("7070".to_owned()),
            "LOG_FORMAT" => Some("json".to_owned()),
            "LOG_LEVEL" => Some("trace".to_owned()),
            _ => None,
        })
        .unwrap();
        assert_eq!(cfg.server.host, "10.0.0.1");
        assert_eq!(cfg.server.port, 7070);
        assert_eq!(cfg.telemetry.log_format, LogFormat::Json);
        assert_eq!(cfg.telemetry.log_level, "trace");
        assert_eq!(cfg.crypto.jwt_issuer, "shiro-reader");

        let invalid_port = Config::load_from_root_directory(dir.path().to_path_buf(), |key| {
            (key == "PORT").then(|| "not-a-port".to_owned())
        });
        assert!(invalid_port.is_err());

        let unknown_key_dir = tempfile::tempdir().unwrap();
        fs::write(
            unknown_key_dir.path().join("config.toml"),
            "[server]\nunknown_key = true\n",
        )
        .unwrap();
        assert!(
            Config::load_from_root_directory(unknown_key_dir.path().to_path_buf(), |_| None)
                .is_err()
        );
    }

    #[test]
    fn socket_addr_parses_ipv4_and_bracketed_ipv6() {
        let ipv4 = ServerConfig {
            host: "127.0.0.1".to_owned(),
            port: 8080,
            max_upload_size: 1024,
        };
        assert_eq!(ipv4.socket_addr(), "127.0.0.1:8080".parse().unwrap());

        let ipv6 = ServerConfig {
            host: "[::1]".to_owned(),
            port: 8443,
            max_upload_size: 1024,
        };
        assert_eq!(ipv6.socket_addr(), "[::1]:8443".parse().unwrap());
    }
}
