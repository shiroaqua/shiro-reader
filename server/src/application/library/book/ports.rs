use async_trait::async_trait;

use crate::{
    domain::library::{book::{
        entity::Book,
        value_objects::{BookId, BookTitle},
    }, bookshelf::value_objects::BookshelfId},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait BookRepository: Send + Sync {
    async fn create(&self, book: Book) -> Result<Book, RepositoryError>;
    async fn delete(&self, id: &BookId) -> Result<(), RepositoryError>;
    async fn rename(&self, id: &BookId, new_title: &BookTitle) -> Result<(), RepositoryError>;
    async fn find_by_id(&self, id: &BookId) -> Result<Book, RepositoryError>;
    async fn list(&self, bookshelf_id: &BookshelfId) -> Result<Vec<Book>, RepositoryError>;
}
