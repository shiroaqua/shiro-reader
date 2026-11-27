use derive_more::{AsRef, Display, Into};
use entity_derive::Entity;
use jiff::Timestamp;

use crate::identity::user::value_objects::UserId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionDomainError {
    #[error("Token hash cannot be empty")]
    MissingTokenHash,

    #[error("Token hash must be a 64-character hex string")]
    InvalidTokenHash,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct TokenHash(String);

impl TokenHash {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, SessionDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(SessionDomainError::MissingTokenHash);
        }
        if raw.len() != 64 || !raw.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(SessionDomainError::InvalidTokenHash);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Entity)]
pub struct AuthSession {
    #[key]
    refresh_hash: TokenHash,
    user_id: UserId,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl AuthSession {
    pub fn new(refresh_hash: TokenHash, user_id: UserId) -> Self {
        let now = Timestamp::now();
        Self {
            refresh_hash,
            user_id,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn refresh_hash(&self) -> &TokenHash {
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
    access_hash: TokenHash,
    refresh_hash: TokenHash,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl AuthAccessToken {
    pub fn new(access_hash: TokenHash, refresh_hash: TokenHash) -> Self {
        let now = Timestamp::now();
        Self {
            access_hash,
            refresh_hash,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn access_hash(&self) -> &TokenHash {
        &self.access_hash
    }

    pub fn refresh_hash(&self) -> &TokenHash {
        &self.refresh_hash
    }

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }
}
