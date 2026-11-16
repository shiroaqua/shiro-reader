use std::path::Path;

use toasty::{Db, sql};
use toasty_driver_sqlite::Sqlite;

use crate::domain::{
    identity::{
        session::{AuthAccessToken, AuthSession},
        user::entity::User,
    },
    library::{
        book::entity::Book,
        tag::entity::{BookTag, TagDefinition},
    },
};

pub async fn connect_identity(path: &Path) -> anyhow::Result<Db> {
    // SQLite foreign_keys is connection-local; migrations enable it on this pooled connection.
    let mut db = Db::builder()
        .models(toasty::models!(User, AuthSession, AuthAccessToken))
        .max_pool_size(1)
        .build(Sqlite::open(path))
        .await?;
    migrate(
        &mut db,
        &[
            include_str!("../../../migrations/identity/01_create_users.sql"),
            include_str!("../../../migrations/identity/02_create_auth_sessions.sql"),
        ],
    )
    .await?;
    Ok(db)
}

pub async fn connect_library(path: &Path) -> anyhow::Result<Db> {
    // Keep the library on the connection where foreign_keys is enabled.
    let mut db = Db::builder()
        .models(toasty::models!(Book, TagDefinition, BookTag))
        .max_pool_size(1)
        .build(Sqlite::open(path))
        .await?;
    migrate(
        &mut db,
        &[
            include_str!("../../../migrations/library/01_create_books.sql"),
            include_str!("../../../migrations/library/02_create_tags.sql"),
        ],
    )
    .await?;
    Ok(db)
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
