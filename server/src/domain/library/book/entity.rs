use std::time::{Duration, SystemTime, UNIX_EPOCH};

use blake3::Hash;

use crate::{
    domain::library::{
        book::{
            errors::BookDomainError,
            value_objects::{BookId, BookTitle},
        },
        bookshelf::value_objects::BookshelfId,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    id: BookId,
    title: BookTitle,
    hash: Hash,
    bookshelf_id: BookshelfId,
    created_at: i128,
    updated_at: i128,
}

impl Book {
    pub fn new(id: BookId, title: BookTitle, hash: Hash, bookshelf_id: BookshelfId, created_at: i128) -> Self {
        Self {
            id,
            title,
            hash,
            bookshelf_id,
            created_at: created_at,
            updated_at: created_at,
        }
    }

    pub(crate) fn reconstitute(
        id: BookId,
        title: BookTitle,
        hash: Hash,
        bookshelf_id: BookshelfId,
        created_at: i128,
        updated_at: i128,
    ) -> Self {
        Self {
            id,
            title,
            hash,
            bookshelf_id,
            created_at,
            updated_at,
        }
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
    pub fn bookshelf_id(&self) -> &BookshelfId {
        &self.bookshelf_id
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
