use std::time::{Duration, SystemTime, UNIX_EPOCH};

use blake3::Hash;

use crate::{
    domain::library::{
        book::{
            errors::BookDomainError,
            value_objects::{BookId, BookTitle},
        }
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    id: BookId,
    title: BookTitle,
    hash: Hash,
    created_at: i128,
    updated_at: i128,
}

impl Book {
    pub fn new(
        id: BookId,
        title: BookTitle,
        hash: Hash,
        created_at: i128,
    ) -> Result<Self, BookDomainError> {
        validate_timestamps(created_at, created_at)?;
        Ok(Self {
            id,
            title,
            hash,
            created_at,
            updated_at: created_at,
        })
    }

    pub(crate) fn reconstitute(
        id: BookId,
        title: BookTitle,
        hash: Hash,
        created_at: i128,
        updated_at: i128,
    ) -> Result<Self, BookDomainError> {
        validate_timestamps(created_at, updated_at)?;
        Ok(Self {
            id,
            title,
            hash,
            created_at,
            updated_at,
        })
    }

    pub fn id(&self) -> &BookId {
        &self.id
    }
    pub fn title(&self) -> &BookTitle {
        &self.title
    }
    pub fn hash(&self) -> &Hash {
        &self.hash
    }
    pub fn created_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::new(
            self.created_at.div_euclid(1_000_000_000) as u64,
            self.created_at.rem_euclid(1_000_000_000) as u32,
        )
    }
    pub fn updated_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::new(
            self.updated_at.div_euclid(1_000_000_000) as u64,
            self.updated_at.rem_euclid(1_000_000_000) as u32,
        )
    }
    pub fn rename(&mut self, new_title: BookTitle) -> Result<(), BookDomainError> {
        if self.title.eq(&new_title) {
            return Err(BookDomainError::TitleUnchanged);
        }
        self.title = new_title;
        Ok(())
    }
}

fn validate_timestamps(created_at: i128, updated_at: i128) -> Result<(), BookDomainError> {
    if !is_valid_timestamp(created_at)
        || !is_valid_timestamp(updated_at)
        || updated_at < created_at
    {
        return Err(BookDomainError::InvalidTimestamp);
    }
    Ok(())
}

fn is_valid_timestamp(timestamp: i128) -> bool {
    let seconds = timestamp.div_euclid(1_000_000_000);
    let Ok(seconds) = i64::try_from(seconds) else {
        return false;
    };
    let nanos = timestamp.rem_euclid(1_000_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos).is_some()
}