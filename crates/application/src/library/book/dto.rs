use serde::{Deserialize, Serialize};
use shiro_domain::library::book::{entity::Book, file::entity::BookFileType};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetBookOutput {
    pub id: String,
    pub title: String,
    pub hash: String,
    #[serde(rename = "type")]
    pub file_type: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<CreateBookOutput> for GetBookOutput {
    fn from(value: CreateBookOutput) -> Self {
        Self {
            id: value.id,
            title: value.title,
            hash: value.hash,
            file_type: value.file_type,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrashedBookOutput {
    #[serde(flatten)]
    pub book: GetBookOutput,
    pub deleted_at: String,
}

impl From<(Book, BookFileType)> for CreateBookOutput {
    fn from((book, file_type): (Book, BookFileType)) -> Self {
        let book = book.into_inner();
        Self {
            id: book.id.to_string(),
            title: book.title.into(),
            hash: book.hash.as_hash().to_hex().to_string(),
            file_type: file_type.to_string(),
            created_at: book.created_at.to_string(),
            updated_at: book.updated_at.to_string(),
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
            created_at: book.created_at.to_string(),
            updated_at: book.updated_at.to_string(),
        }
    }
}
