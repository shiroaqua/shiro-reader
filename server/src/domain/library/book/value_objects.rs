use derive_more::{AsRef, Deref, Display, From};
use uuid::Uuid;

use crate::domain::library::book::errors::BookDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct BookId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct BookTitle(String);

impl BookId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::InvalidBookId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookDomainError::InvalidBookId)?;
        Ok(Self(parsed))
    }
}

impl BookTitle {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::MissingBookTitle);
        }

        if raw.trim() != raw {
            return Err(BookDomainError::InvalidBookTitleFormat);
        }

        Ok(Self(raw.to_string()))
    }
}
