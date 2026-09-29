use async_trait::async_trait;


use crate::{
    application::library::{bookshelf::dto::GetBookshelfOutput}, domain::library::bookshelf::{entity::Bookshelf, value_objects::{BookshelfId}}, infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait BookshelfRepository: Send + Sync {
    async fn create(&self, bookshelf: &Bookshelf) -> Result<(), RepositoryError>;
    async fn delete(&self, id: &BookshelfId) -> Result<(), RepositoryError>;  
    async fn find_by_id(&self, id: &BookshelfId) -> Result<Bookshelf, RepositoryError>; 
    async fn update(&self, bookshelf: Bookshelf) -> Result<(), RepositoryError>;

    async fn list(&self, ids: Vec<BookshelfId>) -> Result<Vec<GetBookshelfOutput>, RepositoryError>; 

}
