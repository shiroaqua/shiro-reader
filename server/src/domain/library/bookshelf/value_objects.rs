use std::str::FromStr;

use derive_more::{AsRef, Deref, Display, From};
use uuid::Uuid;

use crate::domain::library::bookshelf::errors::BookshelfDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct BookshelfId(Uuid);

impl BookshelfId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookshelfDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookshelfDomainError::InvalidId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookshelfDomainError::InvalidId)?;
        Ok(Self(parsed))
    }
}

impl Default for BookshelfId {
    fn default() -> Self {
        Self::new()
    }
}


#[derive(Debug, Clone, PartialEq, Eq, Display, From, AsRef, Deref)]
pub struct BookshelfName(String);

impl BookshelfName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookshelfDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookshelfDomainError::MissingName);
        }

        if !raw.chars().all(|c| c.is_alphanumeric()) {
            return Err(BookshelfDomainError::InvalidNameFormat);
        }

        Ok(Self(raw.to_string()))
    }
}
