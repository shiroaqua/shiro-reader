use blake3::Hash;

use crate::{
    domain::library::{
        book::{
            errors::BookDomainError,
            value_objects::{BookId, BookTitle},
            file::entity::BookFileType,
        },
        bookshelf::value_objects::BookshelfId,
    },
    shared::time::now_ms,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    id: BookId,
    title: BookTitle,
    hash: Hash,
    file_type: BookFileType,
    bookshelf_id: BookshelfId,
    created_at: i64,
    updated_at: i64,
    version: i64,
}

impl Book {
    pub fn new(id: BookId, title: BookTitle, hash: Hash, file_type: BookFileType, bookshelf_id: BookshelfId) -> Self {
        let now = now_ms();
        Self {
            id,
            title,
            hash,
            file_type,
            bookshelf_id,
            created_at: now,
            updated_at: now,
            version: 0,
        }
    }

    pub(crate) fn reconstitute(
        id: BookId,
        title: BookTitle,
        hash: Hash,
        file_type: BookFileType,
        bookshelf_id: BookshelfId,
        created_at: i64,
        updated_at: i64,
        version: i64,
    ) -> Self {
        Self {
            id,
            title,
            hash,
            file_type,
            bookshelf_id,
            created_at,
            updated_at,
            version,
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
    pub fn file_type(&self) -> &BookFileType { &self.file_type }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }
    pub(crate) fn version(&self) -> i64 {
        self.version
    }

    pub fn rename(&mut self, new_title: BookTitle) -> Result<(), BookDomainError> {
        if self.title.eq(&new_title) {
            return Err(BookDomainError::TitleUnchanged);
        }
        self.title = new_title;
        Ok(())
    }
}
