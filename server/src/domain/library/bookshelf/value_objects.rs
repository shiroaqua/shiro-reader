use std::str::FromStr;

use derive_more::{AsRef, Deref, Display, From};
use uuid::Uuid;

use crate::domain::library::bookshelf::errors::BookshelfDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Deref)]
pub struct BookshelfId(String);

impl BookshelfId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookshelfDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookshelfDomainError::InvalidBookshelfId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookshelfDomainError::InvalidBookshelfId)?;
        Ok(Self(parsed.to_string()))
    }
}

impl Default for BookshelfId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for BookshelfId {
    type Err = BookshelfDomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, From, AsRef, Deref)]
pub struct BookshelfName(String);

impl BookshelfName {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookshelfDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookshelfDomainError::MissingBookshelfName);
        }

        if !raw.chars().all(|c| c.is_alphanumeric()) {
            return Err(BookshelfDomainError::InvalidBookshelfNameFormat);
        }

        Ok(Self(raw.to_string()))
    }
}

impl FromStr for BookshelfName {
    type Err = BookshelfDomainError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}
