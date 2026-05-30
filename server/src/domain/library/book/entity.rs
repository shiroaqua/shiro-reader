use blake3::Hash;
use derive_new::new;
use strum::{Display, EnumString};

use crate::domain::library::{book::value_objects::{BookId, BookTitle}, bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId}};


#[derive(new, Debug, Clone, PartialEq, Eq)]
pub struct Book {
    pub id: BookId,
    pub title: BookTitle,
    pub hash: Hash,
    pub file_type: BookFileType,
    pub bookshelf_id: BookshelfId,
    pub folder_id: Option<FolderId>,
    pub created_at: i64,
    pub updated_at: i64,
}


#[derive(Debug, Clone, PartialEq, Eq, EnumString, Display)]
pub enum BookFileType {
    TXT,
    PDF,
    EPUB
}