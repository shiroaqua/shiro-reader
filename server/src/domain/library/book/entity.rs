use blake3::Hash;
use entity_derive::Entity;
use jiff::Timestamp;

use crate::domain::{
    library::book::{
        errors::BookDomainError,
        value_objects::{BookHash, BookId, BookStatus, BookTitle},
    },
};

#[derive(Debug, Clone, PartialEq, Eq, toasty::Model, Entity)]
#[table = "books"]
pub struct Book {
    #[key]
    id: BookId,
    #[unique]
    title: BookTitle,
    hash: BookHash,
    created_at: Timestamp,
    updated_at: Timestamp,
    deleted_at: Option<Timestamp>,
}

impl Book {
    pub fn new(id: BookId, title: BookTitle, hash: Hash) -> Result<Self, BookDomainError> {
        let created_at = Timestamp::now();
        Ok(Self {
            id,
            title,
            hash: BookHash(hash),
            created_at,
            updated_at: created_at,
            deleted_at: None,
        })
    }

    pub fn title(&self) -> &BookTitle {
        &self.title
    }
    pub fn hash(&self) -> &Hash {
        self.hash.as_hash()
    }
    pub fn book_hash(&self) -> &BookHash {
        &self.hash
    }
    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }
    pub fn status(&self) -> BookStatus {
        match self.deleted_at {
            Some(deleted_at) => BookStatus::Deleted { deleted_at },
            None => BookStatus::Active,
        }
    }
    pub fn mark_deleted(&mut self) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        let now = Timestamp::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        Ok(BookChange {
            entity: self,
            fields: BookFields::DELETED_AT | BookFields::UPDATED_AT,
        })
    }
    pub fn restore(&mut self, title: BookTitle) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() == BookStatus::Active {
            return Err(BookDomainError::NotDeleted);
        }
        let mut fields = BookFields::DELETED_AT | BookFields::UPDATED_AT;
        if self.title != title {
            self.title = title;
            fields |= BookFields::TITLE;
        }
        self.deleted_at = None;
        self.updated_at = Timestamp::now();
        Ok(BookChange {
            entity: self,
            fields,
        })
    }
    pub fn rename(&mut self, new_title: BookTitle) -> Result<BookChange<'_>, BookDomainError> {
        if self.status() != BookStatus::Active {
            return Err(BookDomainError::AlreadyDeleted);
        }
        if self.title.eq(&new_title) {
            return Err(BookDomainError::TitleUnchanged);
        }

        self.title = new_title;
        self.updated_at = Timestamp::now();
        Ok(BookChange {
            entity: self,
            fields: BookFields::TITLE | BookFields::UPDATED_AT,
        })
    }
}

impl crate::domain::EntityId for Book {
    type Id = BookId;

    fn id(&self) -> &BookId {
        &self.id
    }
}