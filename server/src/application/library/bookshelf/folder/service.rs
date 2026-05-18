use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        bookshelf::folder::{
            commands::{
                CreateFolderCommand, CreateFolderOutput, DeleteFolderCommand, RenameFolderCommand,
            },
            errors::FolderApplicationError,
            ports::FolderRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::bookshelf::{
        folder::{
            entity::Folder,
            errors::FolderDomainError,
            value_objects::{FolderId, FolderName},
        },
        value_objects::BookshelfId,
    },
    infrastructure::repositories::errors::RepositoryError,
    shared::time::now_ms,
};

#[derive(new)]
pub struct FolderService {
    repository: Arc<dyn FolderRepository>,
}

impl FolderService {
    pub async fn create_folder(
        &self,
        command: CreateFolderCommand,
    ) -> Result<CreateFolderOutput, LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let parent_id = command
            .parent_id
            .map(FolderId::parse_parent_id)
            .transpose()?;
        let name = FolderName::parse(command.name)?;
        let now = now_ms();

        let folder = Folder::new(FolderId::new(), bookshelf_id, parent_id, name, now, now);
        let created = self
            .repository
            .create(folder)
            .await
            .map_err(FolderApplicationError::from)?;

        Ok(CreateFolderOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }

    pub async fn rename_folder(
        &self,
        command: RenameFolderCommand,
    ) -> Result<(), LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(command.bookshelf_id)?;
        let folder_id = FolderId::parse_folder_id(command.folder_id)?;
        let new_name = FolderName::parse(command.name)?;
        self.repository
            .rename(&bookshelf_id, &folder_id, &new_name)
            .await
            .map_err(FolderApplicationError::from)?;

        Ok(())
    }

    pub async fn delete_folder(
        &self,
        commmand: DeleteFolderCommand,
    ) -> Result<(), LibraryApplicationError> {
        let bookshelf_id = BookshelfId::parse(commmand.bookshelf_id)?;
        let folder_id = FolderId::parse_folder_id(commmand.folder_id)?;
        self.repository
            .delete(&bookshelf_id, &folder_id)
            .await
            .map_err(FolderApplicationError::from)?;
        Ok(())
    }
}

impl From<FolderDomainError> for FolderApplicationError {
    fn from(value: FolderDomainError) -> Self {
        match value {
            FolderDomainError::InvalidFolderId => Self::InvalidId,
            FolderDomainError::InvalidParentFolderId => Self::InvalidParentId,
            FolderDomainError::MissingFolderId => Self::MissingFolderId,
            FolderDomainError::MissingFolderName => Self::MissingName,
            FolderDomainError::InvalidFolderNameFormat => Self::InvalidNameFormat,
        }
    }
}

impl From<RepositoryError> for FolderApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::FolderNotFound => Self::NotFound,
            RepositoryError::ParentFolderNotFound => Self::ParentNotFound,
            RepositoryError::FolderNameConflict => Self::NameConflict,
            RepositoryError::Storage(error) => Self::Storage(error),
            _ => Self::Storage(anyhow::anyhow!("unrelated error")),
        }
    }
}
