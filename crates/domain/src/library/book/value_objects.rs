use bitflags::bitflags;
use derive_more::{AsRef, Display, From, Into};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::book::errors::BookDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Into, Serialize, Deserialize)]
pub struct BookId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookTitle(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Into)]
pub struct BookHash(blake3::Hash);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookStatus {
    Active,
    Deleted { deleted_at: Timestamp },
}

impl BookHash {
    pub const fn new(hash: blake3::Hash) -> Self {
        Self(hash)
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(blake3::Hash::from_bytes(bytes))
    }

    pub const fn as_hash(&self) -> &blake3::Hash {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    pub const fn into_inner(self) -> blake3::Hash {
        self.0
    }
}

impl From<blake3::Hash> for BookHash {
    fn from(hash: blake3::Hash) -> Self {
        Self(hash)
    }
}

impl BookId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub const fn into_inner(self) -> Uuid {
        self.0
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::InvalidId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookDomainError::InvalidId)?;
        Ok(Self(parsed))
    }
}

impl Default for BookId {
    fn default() -> Self {
        Self::new()
    }
}

impl BookTitle {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::MissingTitle);
        }

        if raw.trim() != raw {
            return Err(BookDomainError::InvalidTitleFormat);
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

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TocEntryFlags: u16 {
        /// Entry is collapsed by default (e.g. PDF outline `/Count < 0`).
        const DEFAULT_COLLAPSED = 1 << 0;
        /// Entry points to an external link/URI rather than an in-book location.
        const EXTERNAL_LINK = 1 << 1;
    }
}