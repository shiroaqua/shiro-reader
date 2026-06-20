use crate::domain::library::{
    book::{file::entity::BookFileType, value_objects::{BookId, BookTitle}},
    bookshelf::{folder::value_objects::FolderId, value_objects::BookshelfId},
};


#[derive(Debug)]
pub struct CreateBookCommand {
    pub title: String,
    pub hash: String,
    pub bookshelf_id: String,
    pub folder_id: Option<String>,
}

#[derive(Debug)]
pub struct DeleteBookCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct RenameBookCommand {
    pub id: String,
    pub title: String,
}

#[derive(Debug)]

pub struct GetBooksCommand {
    pub bookshelf_id: String,
    pub folder_id: Option<String>
}

#[derive(Debug)]
pub struct GetBookCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct CreateBookOutput {
    pub id: BookId,
    pub created_at: i64,
}

#[derive(Debug)]
pub struct GetBooksOutput(pub Vec<GetBookOutput>);

#[derive(Debug)]
pub struct GetBookOutput {
    pub id: BookId,
    pub title: BookTitle,
    pub hash: blake3::Hash,
    pub file_type: BookFileType,
    pub bookshelf_id: BookshelfId,
    pub folder_id: Option<FolderId>,
    pub created_at: i64,
    pub updated_at: i64,
}

