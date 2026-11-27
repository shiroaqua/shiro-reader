use shiro_domain::library::{
    book::{
        entity::Book,
        value_objects::{BookId, BookTitle},
    },
    tag::{
        entity::{BookTag, TagDefinition},
        value_objects::{TagColor, TagId, TagName},
    },
};
use uuid::Uuid;

#[cfg(feature = "identity")]
use shiro_domain::identity::{
    session::{AuthAccessToken, AuthSession},
    user::{
        entity::User,
        value_objects::{UserId, UserName, UserPasswordHash},
    },
};

use super::scalars::{DbBookHash, DbTagValueRule, DbTimestamp};

#[derive(Debug, Clone, toasty::Model)]
#[table = "books"]
pub struct BookRecord {
    #[key]
    pub id: Uuid,
    #[unique]
    pub title: String,
    pub hash: DbBookHash,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
    pub deleted_at: Option<DbTimestamp>,
}

impl BookRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .id(self.id)
            .title(self.title)
            .hash(self.hash)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
            .deleted_at(self.deleted_at)
    }
}

impl From<Book> for BookRecord {
    fn from(book: Book) -> Self {
        let inner = book.into_inner();
        Self {
            id: inner.id.as_uuid(),
            title: inner.title.into_inner(),
            hash: DbBookHash(inner.hash),
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
            deleted_at: inner.deleted_at.map(DbTimestamp),
        }
    }
}

impl TryFrom<BookRecord> for Book {
    type Error = anyhow::Error;

    fn try_from(record: BookRecord) -> Result<Self, Self::Error> {
        let title = BookTitle::parse(record.title)?;
        Ok(Book::reconstruct(
            BookId::from_uuid(record.id),
            title,
            record.hash.into(),
            record.created_at.into(),
            record.updated_at.into(),
            record.deleted_at.map(Into::into),
        ))
    }
}

#[derive(Debug, Clone, toasty::Model)]
#[table = "tag_definitions"]
pub struct TagDefinitionRecord {
    #[key]
    pub id: Uuid,
    pub name: String,
    #[index]
    pub parent_id: Option<Uuid>,
    pub color: String,
    pub value_rule: DbTagValueRule,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
}

impl TagDefinitionRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .id(self.id)
            .name(self.name)
            .parent_id(self.parent_id)
            .color(self.color)
            .value_rule(self.value_rule)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
    }
}

impl From<TagDefinition> for TagDefinitionRecord {
    fn from(definition: TagDefinition) -> Self {
        let inner = definition.into_inner();
        Self {
            id: inner.id.as_uuid(),
            name: inner.name.into_inner(),
            parent_id: inner.parent_id.map(|id| id.as_uuid()),
            color: inner.color.into_inner(),
            value_rule: DbTagValueRule(inner.value_rule),
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
        }
    }
}

impl TryFrom<TagDefinitionRecord> for TagDefinition {
    type Error = anyhow::Error;

    fn try_from(record: TagDefinitionRecord) -> Result<Self, Self::Error> {
        Ok(TagDefinition::reconstruct(
            TagId::from_uuid(record.id),
            TagName::parse(record.name)?,
            record.parent_id.map(TagId::from_uuid),
            TagColor::parse(record.color)?,
            record.value_rule.into(),
            record.created_at.into(),
            record.updated_at.into(),
        )?)
    }
}

#[derive(Debug, Clone, toasty::Model)]
#[table = "book_tags"]
pub struct BookTagRecord {
    #[key]
    pub tag_id: Uuid,
    #[key]
    pub book_id: Uuid,
    pub value: Option<String>,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
    pub deleted_at: Option<DbTimestamp>,
}

impl BookTagRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .tag_id(self.tag_id)
            .book_id(self.book_id)
            .value(self.value)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
            .deleted_at(self.deleted_at)
    }
}

impl From<BookTag> for BookTagRecord {
    fn from(binding: BookTag) -> Self {
        let inner = binding.into_inner();
        Self {
            tag_id: inner.tag_id.as_uuid(),
            book_id: inner.book_id.as_uuid(),
            value: inner.value,
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
            deleted_at: inner.deleted_at.map(DbTimestamp),
        }
    }
}

impl From<BookTagRecord> for BookTag {
    fn from(record: BookTagRecord) -> Self {
        BookTag::reconstruct(
            TagId::from_uuid(record.tag_id),
            BookId::from_uuid(record.book_id),
            record.value,
            record.created_at.into(),
            record.updated_at.into(),
            record.deleted_at.map(Into::into),
        )
    }
}

#[cfg(feature = "identity")]
#[derive(Debug, Clone, toasty::Model)]
#[table = "users"]
pub struct UserRecord {
    #[key]
    pub id: Uuid,
    #[unique]
    pub name: String,
    pub password_hash: String,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
}

#[cfg(feature = "identity")]
impl UserRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .id(self.id)
            .name(self.name)
            .password_hash(self.password_hash)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
    }
}

#[cfg(feature = "identity")]
impl From<User> for UserRecord {
    fn from(user: User) -> Self {
        let inner = user.into_inner();
        Self {
            id: inner.id.as_uuid(),
            name: inner.name.into_inner(),
            password_hash: inner.password_hash.into_inner(),
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
        }
    }
}

#[cfg(feature = "identity")]
impl TryFrom<UserRecord> for User {
    type Error = anyhow::Error;

    fn try_from(record: UserRecord) -> Result<Self, Self::Error> {
        Ok(User::reconstruct(
            UserId::from_uuid(record.id),
            UserName::parse(record.name)?,
            UserPasswordHash::parse(record.password_hash)?,
            record.created_at.into(),
            record.updated_at.into(),
        ))
    }
}

#[cfg(feature = "identity")]
#[derive(Debug, Clone, toasty::Model)]
#[table = "auth_sessions"]
pub struct AuthSessionRecord {
    #[key]
    pub refresh_hash: String,
    #[index]
    pub user_id: Uuid,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
}

#[cfg(feature = "identity")]
impl AuthSessionRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .refresh_hash(self.refresh_hash)
            .user_id(self.user_id)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
    }
}

#[cfg(feature = "identity")]
impl From<AuthSession> for AuthSessionRecord {
    fn from(session: AuthSession) -> Self {
        let inner = session.into_inner();
        Self {
            refresh_hash: inner.refresh_hash.into_inner(),
            user_id: inner.user_id.as_uuid(),
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
        }
    }
}

#[cfg(feature = "identity")]
#[derive(Debug, Clone, toasty::Model)]
#[table = "auth_access_tokens"]
pub struct AuthAccessTokenRecord {
    #[key]
    pub access_hash: String,
    #[index]
    pub refresh_hash: String,
    pub created_at: DbTimestamp,
    pub updated_at: DbTimestamp,
}

#[cfg(feature = "identity")]
impl AuthAccessTokenRecord {
    pub fn into_create(self) -> <Self as toasty::codegen_support::Model>::Create {
        Self::create()
            .access_hash(self.access_hash)
            .refresh_hash(self.refresh_hash)
            .created_at(self.created_at)
            .updated_at(self.updated_at)
    }
}

#[cfg(feature = "identity")]
impl From<AuthAccessToken> for AuthAccessTokenRecord {
    fn from(token: AuthAccessToken) -> Self {
        let inner = token.into_inner();
        Self {
            access_hash: inner.access_hash.into_inner(),
            refresh_hash: inner.refresh_hash.into_inner(),
            created_at: DbTimestamp(inner.created_at),
            updated_at: DbTimestamp(inner.updated_at),
        }
    }
}
