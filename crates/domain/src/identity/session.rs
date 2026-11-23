use entity_derive::Entity;
use jiff::Timestamp;

use crate::identity::user::value_objects::UserId;

#[derive(Debug, Clone, PartialEq, Eq, Entity)]
pub struct AuthSession {
    #[key]
    refresh_hash: String,
    user_id: UserId,
    created_at: Timestamp,
    updated_at: Timestamp,
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

    pub fn refresh_hash(&self) -> &str {
        &self.refresh_hash
    }

    pub fn user_id(&self) -> &UserId {
        &self.user_id
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Entity)]
pub struct AuthAccessToken {
    #[key]
    access_hash: String,
    refresh_hash: String,
    created_at: Timestamp,
    updated_at: Timestamp,
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

    pub fn access_hash(&self) -> &str {
        &self.access_hash
    }

    pub fn refresh_hash(&self) -> &str {
        &self.refresh_hash
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }
}
