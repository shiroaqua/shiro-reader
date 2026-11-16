use std::{env, net::SocketAddr, path::PathBuf};

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

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let root_directory = PathBuf::from(env_var("ROOT_DIRECTORY", "."));

        let server = ServerConfig {
            host: env_var("HOST", "127.0.0.1"),
            port: env_var("PORT", "80").parse()?,
            max_upload_size: env_var(
                "MAX_UPLOAD_SIZE",
                &(256 * 1024 * 1024/* 256MB */).to_string(),
            )
            .parse::<usize>()?,
        };

        let telemetry = TelemetryConfig {
            log_format: match env_var("LOG_FORMAT", "pretty").as_str() {
                "json" => LogFormat::Json,
                _ => LogFormat::Pretty,
            },
        };

        let crypto = CryptoConfig {
            argon2_memory_cost: env_var("ARGON2_MEMORY_COST_KIB", "19456").parse()?,
            argon2_time_cost: env_var("ARGON2_TIME_COST", "2").parse()?,
            argon2_parallelism: env_var("ARGON2_PARALLELISM", "1").parse()?,
            argon2_output_length: env_var("ARGON2_OUTPUT_LENGTH", "32").parse()?,
            jwt_secret: env_var("JWT_SECRET", "shiro-reader-development-secret-change-me"),
            jwt_issuer: env_var("JWT_ISSUER", "shiro-reader"),
            jwt_audience: env_var("JWT_AUDIENCE", "shiro-reader"),
            jwt_access_ttl_secs: env_var("JWT_ACCESS_TTL_SECS", "900").parse()?,
            jwt_refresh_ttl_secs: env_var("JWT_REFRESH_TTL_SECS", "2592000").parse()?,
            jwt_validation_leeway_secs: env_var("JWT_VALIDATION_LEEWAY_SECS", "60").parse()?,
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

fn env_var(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}
