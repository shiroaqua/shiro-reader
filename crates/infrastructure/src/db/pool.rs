use std::{fs, io::Read, path::Path};

use anyhow::{Context, bail};
use shiro_domain::library::tag::{
    entity::TagDefinition,
    value_objects::{TagColor, TagName, TagValueRule},
};
use toasty::{Db, sql};
use toasty_driver_sqlite::Sqlite;

use super::models::{BookRecord, BookTagRecord, TagDefinitionRecord};

#[cfg(feature = "identity")]
use super::models::{AuthAccessTokenRecord, AuthSessionRecord, UserRecord};

const SQLITE_HEADER_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Validates that an existing SQLite database path is a readable, well-formed SQLite file.
///
/// Rejects directories and non-SQLite files up front so startup never silently overwrites
/// or masks corrupted data.
fn validate_existing_sqlite_path(path: &Path) -> anyhow::Result<bool> {
    if !path.exists() {
        return Ok(true);
    }

    let metadata = fs::metadata(path)
        .with_context(|| format!("cannot inspect database path {}", path.display()))?;
    if !metadata.is_file() {
        bail!("database path is not a regular file: {}", path.display());
    }
    if metadata.len() == 0 {
        return Ok(true);
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

    Ok(false)
}

#[cfg(feature = "identity")]
pub async fn connect_identity(path: &Path) -> anyhow::Result<Db> {
    let _ = validate_existing_sqlite_path(path)?;

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
    let first_initialization = validate_existing_sqlite_path(path)?;

    // Keep the library on the single pooled connection where busy_timeout, WAL, and foreign_keys
    // are configured directly.
    let mut db = Db::builder()
        .models(toasty::models!(
            BookRecord,
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
        ],
    )
    .await?;
    if first_initialization {
        seed_default_tags(&mut db).await?;
    }
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

const DEFAULT_TAG_COLOR: &str = "#000000";
const LANGUAGE_REGEX: &str = "^(?:zh-Hans|zh-Hant|en|ja|ab|aa|af|ak|sq|am|ar|an|hy|as|av|ae|ay|az|bm|bn|ba|eu|be|bh|bi|bs|br|bg|my|ca|ch|ce|cu|cv|kw|co|cr|hr|cs|da|dv|nl|dz|eo|et|ee|fo|fj|tl|fi|fr|ff|gl|lg|ka|de|el|gn|gu|ht|ha|he|hz|hi|ho|hu|is|io|ig|id|ia|ie|iu|ik|ga|it|jv|kl|kn|kr|ks|kk|km|ki|rw|kv|kg|ko|kj|ku|ky|lo|la|lv|li|ln|lt|lu|lb|mk|mg|ms|ml|mt|gv|mi|mr|mh|mn|na|nv|ng|ne|nd|se|no|nb|nn|ny|oc|or|oj|om|os|pi|ps|fa|pl|pt|pa|qu|ro|rm|rn|ru|sm|sg|sa|sc|gd|sr|sn|ii|sd|si|sk|sl|so|nr|st|es|su|sw|ss|sv|ty|tg|ta|tt|te|th|bo|ti|to|ts|tn|tr|tk|tw|uk|ur|ug|uz|ve|vi|vo|wa|cy|fy|wo|xh|yi|yo|za|zu)$";
const ISSN_ISBN_REGEX: &str =
    "^(?:97[89](-?[0-9]){10}|([0-9]-?){9}[0-9Xx]|[0-9]{4}-?[0-9]{3}[0-9Xx])$";

struct DefaultTag {
    name: &'static str,
    allows_value: bool,
    value_regex: Option<&'static str>,
}

const DEFAULT_TAGS: &[DefaultTag] = &[
    DefaultTag {
        name: "tag.language",
        allows_value: true,
        value_regex: Some(LANGUAGE_REGEX),
    },
    DefaultTag {
        name: "tag.author",
        allows_value: true,
        value_regex: None,
    },
    DefaultTag {
        name: "tag.issnIsbn",
        allows_value: true,
        value_regex: Some(ISSN_ISBN_REGEX),
    },
    DefaultTag {
        name: "tag.publisher",
        allows_value: true,
        value_regex: None,
    },
    DefaultTag {
        name: "tag.edition",
        allows_value: true,
        value_regex: None,
    },
];

async fn seed_default_tags(db: &mut Db) -> anyhow::Result<()> {
    let mut tx = db.transaction().await?;
    for spec in DEFAULT_TAGS {
        let name = TagName::parse(spec.name)?;
        let color = TagColor::parse(DEFAULT_TAG_COLOR)?;
        let value_rule = TagValueRule::new(spec.allows_value, spec.value_regex.map(str::to_owned))?;
        let record = TagDefinitionRecord::from(TagDefinition::new(name, None, color, value_rule));
        record.into_create().exec(&mut tx).await?;
    }
    tx.commit().await?;
    Ok(())
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
