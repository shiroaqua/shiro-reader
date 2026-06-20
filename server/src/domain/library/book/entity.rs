use blake3::Hash;
use derive_new::new;

use crate::domain::library::{book::value_objects::{BookId, BookTitle}, bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId}};


#[derive(new, Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub id: BookId,
    pub title: BookTitle,
    pub hash: Hash,
    pub bookshelf_id: BookshelfId,
    pub folder_id: Option<FolderId>,
    pub created_at: i64,
    pub updated_at: i64,
}