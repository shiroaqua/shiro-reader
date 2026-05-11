use crate::domain::bookshelf::value_objects::{DirectoryId, DirectoryName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Directory {
    pub id: DirectoryId,
    pub parent_id: Option<DirectoryId>,
    pub name: DirectoryName,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Directory {
    pub fn new(id: DirectoryId, parent_id: DirectoryId, name: DirectoryName, now_ms: i64) -> Self {
        Self {
            id: id,
            name: name,
            parent_id: Some(parent_id),
            created_at: now_ms,
            updated_at: now_ms,
        }
    }

    pub fn is_root(&self) -> bool {
        self.id.is_root()
    }
}
