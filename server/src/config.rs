use std::{env, net::SocketAddr};

#[derive(Debug, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub telemetry: TelemetryConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
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

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let server = ServerConfig {
            host: env_var("HOST", "127.0.0.1"),
            port: env_var("PORT", "80").parse()?,
        };

        let telemetry = TelemetryConfig {
            log_format: match env_var("LOG_FORMAT", "pretty").as_str() {
                "json" => LogFormat::Json,
                _ => LogFormat::Pretty,
            },
        };

        Ok(Self {
            server,
            telemetry,
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
