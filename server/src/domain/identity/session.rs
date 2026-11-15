use crate::domain::identity::user::value_objects::UserId;
use entity_derive::Entity;
use jiff::Timestamp;

#[derive(Debug, toasty::Model, Entity)]
#[table = "auth_sessions"]
pub struct AuthSession {
    #[key]
    pub(crate) refresh_hash: String,
    #[index]
    pub(crate) user_id: UserId,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
}

impl AuthSession {
    pub fn new(refresh_hash: String, user_id: UserId) -> Self {
        let now = Timestamp::now();
        Self {
            refresh_hash,
            user_id,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, toasty::Model, Entity)]
#[table = "auth_access_tokens"]
pub struct AuthAccessToken {
    #[key]
    pub(crate) access_hash: String,
    #[index]
    pub(crate) refresh_hash: String,
    pub(crate) created_at: Timestamp,
    pub(crate) updated_at: Timestamp,
}

impl AuthAccessToken {
    pub fn new(access_hash: String, refresh_hash: String) -> Self {
        let now = Timestamp::now();
        Self {
            access_hash,
            refresh_hash,
            created_at: now,
            updated_at: now,
        }
    }
}
