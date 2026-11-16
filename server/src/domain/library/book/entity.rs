use blake3::Hash;
use entity_derive::Entity;
use jiff::Timestamp;

use crate::domain::{
    RequiresVerificationEntity,
    library::book::{
        errors::BookDomainError,
        value_objects::{BookHash, BookId, BookTitle},
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
    pub fn rename(&mut self, new_title: BookTitle) -> Result<BookChange<'_>, BookDomainError> {
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

impl RequiresVerificationEntity<BookDomainError> for Book {
    fn verify(self) -> Result<Self, BookDomainError> {
        Ok(self)
    }
}
