use async_trait::async_trait;

use crate::{
    domain::library::bookshelf::{
        folder::{
            entity::Folder,
            value_objects::{FolderId, FolderName},
        },
        value_objects::BookshelfId,
    },
    infrastructure::repositories::errors::RepositoryError,
};

#[async_trait]
pub trait FolderRepository: Send + Sync {
    async fn create(&self, folder: Folder) -> Result<Folder, RepositoryError>;
    async fn delete(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId) -> Result<(), RepositoryError>;
   
    async fn rename(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId, new_name: &FolderName) -> Result<(), RepositoryError>;
    async fn move_to(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId, new_parent_folder_id: &FolderId) -> Result<(), RepositoryError>;
   
    async fn find_by_id(&self, bookshelf_id: &BookshelfId, folder_id: &FolderId) -> Result<Folder, RepositoryError>;  
    async fn list_root(&self, bookshelf_id: &BookshelfId) -> Result<Vec<Folder>, RepositoryError>;
    async fn list(&self, bookshelf_id: &BookshelfId) -> Result<Vec<Folder>, RepositoryError>;
}
