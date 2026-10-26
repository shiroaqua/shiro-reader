use std::path::Path;
use shiro_reader_server::config::{Config, CryptoConfig, DatabaseConfig, LogFormat, ServerConfig, TelemetryConfig};
use super::crypto;

pub fn config(root: &Path) -> Config {
    Config {
        root_directory: root.into(),
        server: ServerConfig { host: "127.0.0.1".into(), port: 0, max_upload_size: 8 * 1024 * 1024 },
        telemetry: TelemetryConfig { log_format: LogFormat::Pretty },
        database: DatabaseConfig { max_connections: 1 },
        crypto: CryptoConfig {
            argon2_memory_cost: 1024, argon2_time_cost: 1,
            argon2_parallelism: 1, argon2_output_length: 32,
            jwt_secret: String::from_utf8(crypto::SECRET.to_vec()).unwrap(),
            jwt_issuer: crypto::ISSUER.into(), jwt_audience: crypto::AUDIENCE.into(),
            jwt_access_ttl_secs: crypto::ACCESS_TTL, jwt_refresh_ttl_secs: crypto::REFRESH_TTL,
            jwt_validation_leeway_secs: 0,
        },
    }
}
