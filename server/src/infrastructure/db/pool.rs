use std::path::Path;

use toasty::{Db, sql};
use toasty_driver_sqlite::Sqlite;

use crate::domain::{
    EntityCreate,
    identity::{
        session::{AuthAccessToken, AuthSession},
        user::entity::User,
    },
    library::{
        book::entity::Book,
        tag::{
            entity::{BookTag, TagDefinition},
            value_objects::{TagColor, TagName, TagValueRule},
        },
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
    let first_initialization = !path.exists();
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
    if first_initialization {
        seed_default_tags(&mut db).await?;
    }
    Ok(db)
}

const DEFAULT_TAG_COLOR: &str = "#000000";
const LANGUAGE_REGEX: &str = "^(?:zh-Hans|zh-Hant|en|ja|ab|aa|af|ak|sq|am|ar|an|hy|as|av|ae|ay|az|bm|bn|ba|eu|be|bh|bi|bs|br|bg|my|ca|ch|ce|cu|cv|kw|co|cr|hr|cs|da|dv|nl|dz|eo|et|ee|fo|fj|tl|fi|fr|ff|gl|lg|ka|de|el|gn|gu|ht|ha|he|hz|hi|ho|hu|is|io|ig|id|ia|ie|iu|ik|ga|it|jv|kl|kn|kr|ks|kk|km|ki|rw|kv|kg|ko|kj|ku|ky|lo|la|lv|li|ln|lt|lu|lb|mk|mg|ms|ml|mt|gv|mi|mr|mh|mn|na|nv|ng|ne|nd|se|no|nb|nn|ny|oc|or|oj|om|os|pi|ps|fa|pl|pt|pa|qu|ro|rm|rn|ru|sm|sg|sa|sc|gd|sr|sn|ii|sd|si|sk|sl|so|nr|st|es|su|sw|ss|sv|ty|tg|ta|tt|te|th|bo|ti|to|ts|tn|tr|tk|tw|uk|ur|ug|uz|ve|vi|vo|wa|cy|fy|wo|xh|yi|yo|za|zu)$";
const ISSN_ISBN_REGEX: &str = "^(?:97[89](-?[0-9]){10}|([0-9]-?){9}[0-9Xx]|[0-9]{4}-?[0-9]{3}[0-9Xx])$";

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
    for spec in DEFAULT_TAGS {
        let name = TagName::parse(spec.name)?;
        let color = TagColor::parse(DEFAULT_TAG_COLOR)?;
        let value_rule = TagValueRule::new(spec.allows_value, spec.value_regex.map(str::to_owned))?;
        TagDefinition::new(name, None, color, value_rule)
            .create()
            .exec(db)
            .await?;
    }
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
