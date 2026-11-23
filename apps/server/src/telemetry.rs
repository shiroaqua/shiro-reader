use anyhow::Context;
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

use crate::config::{LogFormat, TelemetryConfig, default_log_level};

fn is_bare_log_level(raw: &str) -> bool {
    matches!(
        raw.to_ascii_lowercase().as_str(),
        "off" | "error" | "warn" | "info" | "debug" | "trace"
    )
}

pub fn build_env_filter(
    config: &TelemetryConfig,
    rust_log: Option<&str>,
) -> anyhow::Result<EnvFilter> {
    if let Some(raw) = rust_log.map(str::trim).filter(|raw| !raw.is_empty()) {
        return EnvFilter::try_new(raw).context("invalid RUST_LOG filter directive");
    }

    let raw = config.log_level.trim();
    let raw = if raw.is_empty() {
        default_log_level()
    } else {
        raw
    };

    if is_bare_log_level(raw) {
        let lvl = raw.to_ascii_lowercase();
        let directive = format!("shiro_reader_server={lvl},axum={lvl},tower_http={lvl}");
        EnvFilter::try_new(directive)
            .with_context(|| format!("invalid log_level filter directive: {raw}"))
    } else {
        EnvFilter::try_new(raw)
            .with_context(|| format!("invalid log_level filter directive: {raw}"))
    }
}

pub fn init(config: &TelemetryConfig) -> anyhow::Result<()> {
    let rust_log = std::env::var(EnvFilter::DEFAULT_ENV).ok();
    let env_filter = build_env_filter(config, rust_log.as_deref())?;

    match config.log_format {
        LogFormat::Json => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer().json())
                .init();
        }
        LogFormat::Pretty => {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .pretty()
                        .with_file(false)
                        .with_line_number(false),
                )
                .init();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::build_env_filter;
    use crate::config::{LogFormat, TelemetryConfig};

    fn telemetry(log_level: &str) -> TelemetryConfig {
        TelemetryConfig {
            log_format: LogFormat::Pretty,
            log_level: log_level.to_owned(),
        }
    }

    fn assert_directives(filter: &super::EnvFilter, expected: &[&str]) {
        let mut actual: Vec<String> = filter
            .to_string()
            .split(',')
            .map(|part| part.trim().to_owned())
            .collect();
        let mut expected: Vec<String> = expected.iter().map(|part| (*part).to_owned()).collect();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn rust_log_overrides_config_log_level() {
        let filter = build_env_filter(&telemetry("warn"), Some("shiro_reader_server=trace"))
            .expect("valid RUST_LOG");
        assert_directives(&filter, &["shiro_reader_server=trace"]);

        let blank_rust_log =
            build_env_filter(&telemetry("warn"), Some("   ")).expect("fallback to config");
        assert_directives(
            &blank_rust_log,
            &[
                "shiro_reader_server=warn",
                "axum=warn",
                "tower_http=warn",
            ],
        );
    }

    #[test]
    fn bare_levels_expand_to_server_crates_and_full_directives_pass_through() {
        for level in ["debug", "info", "warn", "off"] {
            let input = if level == "info" { "INFO" } else { level };
            let filter = build_env_filter(&telemetry(input), None).expect("bare level");
            assert_directives(
                &filter,
                &[
                    &format!("shiro_reader_server={level}"),
                    &format!("axum={level}"),
                    &format!("tower_http={level}"),
                ],
            );
        }

        let custom = "shiro_reader_server=trace,sqlx=warn";
        let filter = build_env_filter(&telemetry(custom), None).expect("custom directive");
        assert_directives(&filter, &["shiro_reader_server=trace", "sqlx=warn"]);
    }

    #[test]
    fn invalid_filter_expressions_return_error() {
        assert!(build_env_filter(&telemetry("invalid[directive"), None).is_err());
        assert!(build_env_filter(&telemetry("info"), Some("invalid[directive")).is_err());
    }
}
