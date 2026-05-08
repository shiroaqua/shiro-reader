use shiro_reader_server::{app, config::Config, state::AppState, telemetry};
use tokio::net::TcpListener;

use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    telemetry::init(&config.telemetry)?;

    let state = AppState::build(&config).await?;
    let router = app::build_router(state);

    let listener = TcpListener::bind(config.server.socket_addr()).await?;
    info!(addr = %config.server.socket_addr(), "server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
