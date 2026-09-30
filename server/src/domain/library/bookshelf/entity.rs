use std::time::{Duration, SystemTime, UNIX_EPOCH};
use crate::{
    domain::library::bookshelf::{
        errors::BookshelfDomainError,
        value_objects::{BookshelfId, BookshelfName},
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bookshelf {
    id: BookshelfId,
    name: BookshelfName,
    created_at: i128,
    updated_at: i128,
}

impl Bookshelf {
    pub fn new(id: BookshelfId, name: BookshelfName, created_at: i128) -> Self {
        Self {
            id,
            name,
            created_at: created_at,
            updated_at: created_at,
        }
    }

    pub(crate) fn reconstitute(
        id: BookshelfId,
        name: BookshelfName,
        created_at: i128,
        updated_at: i128,
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
    pub fn created_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::new(self.created_at.div_euclid(1_000_000_000) as u64, self.created_at.rem_euclid(1_000_000_000) as u32)
    }
    pub fn updated_at(&self) -> SystemTime {
        UNIX_EPOCH + Duration::new(self.updated_at.div_euclid(1_000_000_000) as u64, self.updated_at.rem_euclid(1_000_000_000) as u32)
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
