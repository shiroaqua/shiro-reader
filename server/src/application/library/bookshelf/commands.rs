
use crate::domain::library::bookshelf::{entity::Bookshelf, value_objects::{BookshelfId, BookshelfName}};
use o2o::o2o;

#[derive(Debug)]
pub struct CreateBookshelfCommand {
    pub name: String,
}


#[derive(Debug)]
pub struct GetBookshelfCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct GetAllBookshelfOutput(pub Vec<GetBookshelfOutput>);


#[derive(Debug, o2o)]
#[from_owned(Bookshelf)]
pub struct GetBookshelfOutput {
    pub id: BookshelfId,
    pub name: BookshelfName,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug)]
pub struct DeleteBookshelfCommand {
    pub id: String,
}

#[derive(Debug)]
pub struct CreateBookshelfOutput {
    pub id: BookshelfId,
    pub created_at: i64,
}

