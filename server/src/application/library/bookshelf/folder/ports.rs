use async_trait::async_trait;


use crate::{
    domain::library::bookshelf::folder::{entity::Folder, value_objects::{FolderId, FolderName}},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait FolderRepository: Send + Sync {
    async fn create(&self, directory: Folder) -> Result<Folder, RepositoryError>; 
}
