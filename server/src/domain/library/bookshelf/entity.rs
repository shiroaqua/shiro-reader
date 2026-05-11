use derive_new::new;

use crate::domain::library::bookshelf::value_objects::{BookshelfId, BookshelfName};

#[derive(new, Debug, Clone, PartialEq, Eq)]
pub struct Bookshelf {
    pub id: BookshelfId,
    pub name: BookshelfName,
    pub created_at: i64,
    pub updated_at: i64,
}
