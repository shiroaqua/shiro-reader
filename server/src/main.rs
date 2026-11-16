use axum::extract::DefaultBodyLimit;
use shiro_reader_server::{
    app,
    application::identity::user::{commands::UserRegisterCommand, queries::GetUsersQuery},
    config::Config,
    state::AppState,
    telemetry,
};
use tokio::net::TcpListener;

use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::load()?;
    telemetry::init(&config.telemetry)?;

    let state = AppState::build(&config).await?;

    if state
        .user_service
        .get_users(GetUsersQuery { user_ids: vec![] })
        .await?
        .len()
        == 0
    {
        if cfg!(debug_assertions) {
            state
                .user_service
                .register(UserRegisterCommand {
                    name: "admin".to_owned(),
                    password: "admin".to_owned(),
                })
                .await?;
        } else {
            let option = securepass::PasswordOptions {
                length: 10,
                include_special_chars: false,
                include_uppercase: true,
                include_numbers: true,
                with_balancing: true,
                phrase: None,
            };

            let password = option.generate_password().unwrap();
            state
                .user_service
                .register(UserRegisterCommand {
                    name: "admin".to_owned(),
                    password: password.to_owned(),
                })
                .await?;
            info!("admin: {password}")
        }
    }

    let router =
        app::build_router(state).layer(DefaultBodyLimit::max(config.server.max_upload_size));

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
