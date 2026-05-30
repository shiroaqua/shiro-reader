use derive_new::new;

use crate::domain::library::bookshelf::folder::value_objects::{FolderId, FolderName};
use crate::domain::library::bookshelf::value_objects::BookshelfId;

#[derive(new, Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub id: FolderId,
    pub bookshelf_id: BookshelfId,
    pub parent_id: FolderId,
    pub name: FolderName,
    pub created_at: i64,
    pub updated_at: i64,
}