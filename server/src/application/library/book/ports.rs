use async_trait::async_trait;

use crate::{
    application::library::book::dto::GetBookOutput, domain::library::{book::{
        entity::Book,
        value_objects::BookId,
    }, bookshelf::value_objects::BookshelfId}, infrastructure::repositories::{errors::RepositoryError, sqlite::VersionedRecord},
};

#[async_trait]
pub trait BookRepository: Send + Sync {
    async fn create(&self, book: &Book) -> Result<(), RepositoryError>;
    async fn delete(&self, id: &BookId) -> Result<(), RepositoryError>;
    
    async fn find_by_id(&self, id: &BookId) -> Result<VersionedRecord<Book>, RepositoryError>;
    async fn commit(&self, book: VersionedRecord<Book>) -> Result<(), RepositoryError>;

    
    async fn list(&self, bookshelf_id: &BookshelfId, book_ids: Vec<BookId>) -> Result<Vec<GetBookOutput>, RepositoryError>;    
}
