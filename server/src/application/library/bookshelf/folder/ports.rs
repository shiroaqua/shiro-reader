use async_trait::async_trait;


use crate::{
    domain::library::bookshelf::{folder::{entity::Folder, value_objects::{FolderId}}, value_objects::BookshelfId},
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait FolderRepository: Send + Sync {
    async fn create(&self, folder: Folder) -> Result<Folder, RepositoryError>; 
    async fn delete(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId) -> Result<(), RepositoryError>; 
}
