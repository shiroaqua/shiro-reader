use std::{fs, io::Read, path::Path};

use anyhow::{Context, bail};
use toasty::{Db, sql};
use toasty_driver_sqlite::Sqlite;

use super::models::{BookRecord, BookTagRecord, ChapterRecord, TagDefinitionRecord};

#[cfg(feature = "identity")]
use super::models::{AuthAccessTokenRecord, AuthSessionRecord, UserRecord};

const SQLITE_HEADER_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Validates that an existing SQLite database path is a readable, well-formed SQLite file.
///
/// Rejects directories and non-SQLite files up front so startup never silently overwrites
/// or masks corrupted data.
fn validate_existing_sqlite_path(path: &Path) -> anyhow::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect database path {}", path.display()))?;
    if !metadata.is_file() {
        bail!("database path is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 {
        return Ok(());
    }

    let mut file = fs::File::open(path)
        .with_context(|| format!("cannot open database file {}", path.display()))?;
    let mut header = [0u8; 16];
    if file.read_exact(&mut header).is_err() || &header != SQLITE_HEADER_MAGIC {
        bail!(
            "database file is corrupted or not a valid SQLite 3 database: {}",
            path.display()
        );
    }

    Ok(())
}

#[cfg(feature = "identity")]
pub async fn connect_identity(path: &Path) -> anyhow::Result<Db> {
    validate_existing_sqlite_path(path)?;

    // SQLite pragmas (busy_timeout, foreign_keys) are connection-local; apply them directly
    // on this single-connection Toasty pool so every query inherits them.
    let mut db = Db::builder()
        .models(toasty::models!(
            UserRecord,
            AuthSessionRecord,
            AuthAccessTokenRecord
        ))
        .max_pool_size(1)
        .build(Sqlite::open(path))
        .await
        .with_context(|| format!("failed to open identity database {}", path.display()))?;
    exec_pragma_queries(
        &mut db,
        &[include_str!("../../migrations/identity/00_sqlite_pragmas.sql")],
    )
    .await?;
    migrate(
        &mut db,
        &[
            include_str!("../../migrations/identity/01_create_users.sql"),
            include_str!("../../migrations/identity/02_create_auth_sessions.sql"),
        ],
    )
    .await?;
    Ok(db)
}

pub async fn connect_library(path: &Path) -> anyhow::Result<Db> {
    validate_existing_sqlite_path(path)?;

    // Keep the library on the single pooled connection where busy_timeout, WAL, and foreign_keys
    // are configured directly.
    let mut db = Db::builder()
        .models(toasty::models!(
            BookRecord,
            ChapterRecord,
            TagDefinitionRecord,
            BookTagRecord
        ))
        .max_pool_size(1)
        .build(Sqlite::open(path))
        .await
        .with_context(|| format!("failed to open library database {}", path.display()))?;
    exec_pragma_queries(
        &mut db,
        &[include_str!("../../migrations/library/00_sqlite_wal.sql")],
    )
    .await?;
    migrate(
        &mut db,
        &[
            include_str!("../../migrations/library/01_create_books.sql"),
            include_str!("../../migrations/library/02_create_tags.sql"),
            include_str!("../../migrations/library/03_create_chapters.sql"),
        ],
    )
    .await?;
    Ok(db)
}

async fn exec_pragma_queries(db: &mut Db, files: &[&str]) -> anyhow::Result<()> {
    for file in files {
        for statement in file.split("-- #[toasty::breakpoint]") {
            let statement = statement.trim();
            if !statement.is_empty() {
                sql::query(statement).exec(db).await?;
            }
        }
    }
    Ok(())
}

/// Flushes and truncates the SQLite WAL file into the main database file before closing.
///
/// Keeps the library directory self-consistent for clean shutdown and future offline copies.
pub async fn checkpoint_library_wal(path: &Path) -> anyhow::Result<()> {
    let db_path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> anyhow::Result<()> {
        let conn = rusqlite::Connection::open(&db_path)
            .with_context(|| format!("failed to open sqlite database {}", db_path.display()))?;
        conn.execute_batch(include_str!("../../migrations/library/wal_checkpoint.sql"))
            .with_context(|| format!("failed to checkpoint WAL for {}", db_path.display()))?;
        Ok(())
    })
    .await
    .context("SQLite WAL checkpoint task panicked")?
}

async fn migrate(db: &mut Db, files: &[&str]) -> anyhow::Result<()> {
    // Each breakpoint separates complete SQLite statements.
    for file in files {
        for statement in file.split("-- #[toasty::breakpoint]") {
            let statement = statement.trim();
            if !statement.is_empty() {
                sql::statement(statement).exec(db).await?;
            }
        }
    }
    Ok(())
}
