use jiff::Timestamp;

use crate::domain::library::book::{entity::Book, file::entity::BookFileType};

#[derive(Debug)]
pub struct CreateBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub file_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug)]
pub struct GetBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    pub file_type: String,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

#[derive(Debug)]
pub struct TrashedBookOutput {
    pub book: GetBookOutput,
    pub deleted_at: Timestamp,
}

impl From<(Book, BookFileType)> for CreateBookOutput {
    fn from((book, file_type): (Book, BookFileType)) -> Self {
        let book = book.into_inner();
        Self {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: file_type.to_string(),
            created_at: book.created_at,
            updated_at: book.updated_at,
        }
    }
}

impl From<(Book, BookFileType)> for GetBookOutput {
    fn from((book, file_type): (Book, BookFileType)) -> Self {
        let book = book.into_inner();
        Self {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: file_type.to_string(),
            created_at: book.created_at,
            updated_at: book.updated_at,
        }
    }
}
