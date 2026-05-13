use crate::domain::library::bookshelf::value_objects::BookshelfId;


#[derive(Debug)]
pub struct CreateBookshelfCommand {
    pub name: String,
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
