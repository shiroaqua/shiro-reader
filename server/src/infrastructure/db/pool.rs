use std::path::Path;

use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn connect(path: &Path, max_connections: u32) -> anyhow::Result<SqlitePool> {
    let url = format!(
        "sqlite://{}?mode=rwc",
        path.to_string_lossy()
            .replace('%', "%25")
            .replace('?', "%3F")
            .replace('#', "%23")
    );

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
        .connect(&url)
        .await?;

    Ok(pool)
}
