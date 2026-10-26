use async_trait::async_trait;

use crate::{
    application::library::book::dto::GetBookOutput, domain::library::{book::{
        entity::Book,
        value_objects::BookId,
    }}, infrastructure::repositories::{errors::LibraryRepositoryError, sqlite::VersionedRecord},
};

#[async_trait]
pub trait BookRepository: Send + Sync {
    async fn create(&self, book: &Book) -> Result<(), LibraryRepositoryError>;
    async fn delete(&self, id: &BookId) -> Result<(), LibraryRepositoryError>;
    
    async fn find_by_id(&self, id: &BookId) -> Result<VersionedRecord<Book>, LibraryRepositoryError>;
    async fn commit(&self, book: VersionedRecord<Book>) -> Result<(), LibraryRepositoryError>;

    
    async fn list(&self, book_ids: Vec<BookId>) -> Result<Vec<GetBookOutput>, LibraryRepositoryError>;
}
