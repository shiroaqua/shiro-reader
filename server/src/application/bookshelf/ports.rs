use async_trait::async_trait;


use crate::{
    domain::bookshelf::{entity::Directory, value_objects::{DirectoryId, DirectoryName}},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait DirectoryRepository: Send + Sync {
    async fn create(&self, directory: Directory) -> Result<Directory, RepositoryError>; 
}
