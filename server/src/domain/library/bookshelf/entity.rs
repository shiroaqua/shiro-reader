use crate::{
    domain::library::bookshelf::{
        errors::BookshelfDomainError,
        value_objects::{BookshelfId, BookshelfName},
    },
    shared::time::now_ms,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookshelf {
    id: BookshelfId,
    name: BookshelfName,
    created_at: i64,
    updated_at: i64,
}

impl Bookshelf {
    pub fn new(id: BookshelfId, name: BookshelfName) -> Self {
        let now = now_ms();
        Self {
            id,
            name,
            created_at: now,
            updated_at: now,
        }
    }

    pub(crate) fn reconstitute(
        id: BookshelfId,
        name: BookshelfName,
        created_at: i64,
        updated_at: i64,
    ) -> Self {
        Self {
            id,
            name,
            created_at,
            updated_at,
        }
    }

    pub fn id(&self) -> &BookshelfId {
        &self.id
    }
    pub fn name(&self) -> &BookshelfName {
        &self.name
    }
    pub fn created_at(&self) -> i64 {
        self.created_at
    }
    pub fn updated_at(&self) -> i64 {
        self.updated_at
    }

    pub fn rename(&mut self, new_name: BookshelfName) -> Result<(), BookshelfDomainError> {
        if self.name.eq(&new_name) {
            // 这是有必要的，可以拦截多余的数据库写入。
            return Err(BookshelfDomainError::UnchangedName);
        }

        self.name = new_name;
        Ok(())
    }
}
