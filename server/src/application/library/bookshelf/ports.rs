use async_trait::async_trait;


use crate::{
    domain::library::bookshelf::{entity::Bookshelf, value_objects::BookshelfId},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait BookshelfRepository: Send + Sync {
    async fn create(&self, bookshelf: Bookshelf) -> Result<Bookshelf, RepositoryError>; 
    async fn delete(&self, bookshelf_id: &BookshelfId) -> Result<(), RepositoryError>; 
}
