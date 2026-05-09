use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};

use crate::config::DatabaseConfig;

pub async fn connect(config: &DatabaseConfig) -> anyhow::Result<SqlitePool> {
    connect_sqlite_url(&config.url, config.max_connections).await
}

pub async fn connect_sqlite_url(url: &str, max_connections: u32) -> anyhow::Result<SqlitePool> {
    let pool = SqlitePoolOptions::new()
        .max_connections(max_connections)
        .after_connect(|connection, _meta| {
            Box::pin(async move {
                // SQLite 默认可能没有启用外键约束。
                sqlx::query("PRAGMA foreign_keys = ON;")
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect(url)
        .await?;

    Ok(pool)
}
