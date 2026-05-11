use std::sync::Arc;

use crate::{
    application::library::bookshelf::folder::{
        commands::{CreateFolderCommand, CreateFolderOutput},
        errors::FolderApplicationError,
        ports::FolderRepository,
    },
    domain::library::bookshelf::{entity::Bookshelf, folder::{
        entity::Folder,
        errors::FolderDomainError,
        value_objects::{FolderId, FolderName},
    }, value_objects::BookshelfId},
    infrastructure::repositories::errors::RepositoryError,
    shared::time::now_ms,
};

pub struct FolderService {
    repository: Arc<dyn FolderRepository>,
}

impl FolderService {
    pub fn new(repository: Arc<dyn FolderRepository>) -> Self {
        Self { repository }
    }

    pub async fn create_directory(
        &self,
        command: CreateFolderCommand,
    ) -> Result<CreateFolderOutput, FolderApplicationError> {
        let bookshelf_id = BookshelfId::new();
        let parent_id = command.parent_id.map(FolderId::parse_parent_id).transpose()?;
        let name = FolderName::parse(command.name)?;
        let now = now_ms();

        let folder = Folder::new(FolderId::new(), bookshelf_id, parent_id, name, now, now);
        let created = self.repository.create(folder).await?;

        Ok(CreateFolderOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }
}

impl From<FolderDomainError> for FolderApplicationError {
    fn from(value: FolderDomainError) -> Self {
        match value {
            FolderDomainError::InvalidFolderId => Self::InvalidFolderId,
            FolderDomainError::InvalidParentId => Self::InvalidParentId,
            FolderDomainError::FolderIdRequired => Self::FolderIdRequired,
            FolderDomainError::FolderNameRequired => Self::FolderNameRequired,
            FolderDomainError::FolderNameInvalidFormat => Self::FolderNameInvalidFormat,
        }
    }
}

impl From<RepositoryError> for FolderApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::FolderNotFound => Self::FolderNotFound,
            RepositoryError::ParentFolderNotFound => Self::ParentFolderNotFound,
            RepositoryError::FolderNameConflict => Self::FolderNameConflict,
            RepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}