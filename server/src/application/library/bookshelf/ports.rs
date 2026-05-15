use async_trait::async_trait;


use crate::{
    domain::library::bookshelf::{entity::Bookshelf, value_objects::{BookshelfId, BookshelfName}},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait BookshelfRepository: Send + Sync {
    async fn create(&self, bookshelf: &Bookshelf) -> Result<Bookshelf, RepositoryError>;
    async fn rename(&self, id: &BookshelfId, new_name: &BookshelfName)-> Result<(), RepositoryError>;
    async fn delete(&self, id: &BookshelfId) -> Result<(), RepositoryError>; 
    
    async fn get(&self, id: &BookshelfId) -> Result<Bookshelf, RepositoryError>; 
    async fn get_all(&self) -> Result<Vec<Bookshelf>, RepositoryError>; 
}
