use std::{collections::HashMap, sync::Arc};

use crate::{
    application::bookshelf::{
        commands::{CreateDirectoryCommand, CreateDirectoryOutput},
        errors::DirectoryApplicationError,
        ports::DirectoryRepository,
    },
    domain::bookshelf::{
        entity::Directory,
        errors::DirectoryDomainError,
        value_objects::{DirectoryId, DirectoryName},
    },
    infrastructure::repositories::errors::RepositoryError,
    shared::time::now_ms,
};

pub struct DirectoryService {
    repository: Arc<dyn DirectoryRepository>,
}

impl DirectoryService {
    pub fn new(repository: Arc<dyn DirectoryRepository>) -> Self {
        Self { repository }
    }

    pub async fn create_directory(
        &self,
        command: CreateDirectoryCommand,
    ) -> Result<CreateDirectoryOutput, DirectoryApplicationError> {
        let parent_id = match command.parent_id {
            Some(parent_id) => DirectoryId::parse_parent_id(parent_id)?,
            None => DirectoryId::root(),
        };

        let name = DirectoryName::parse(command.name)?;
        let now = now_ms();

        let directory = Directory::new(DirectoryId::new(), parent_id, name, now);
        let created = self.repository.create(directory).await?;

        Ok(CreateDirectoryOutput {
            id: created.id,
            created_at: created.created_at,
        })
    }
}

impl From<DirectoryDomainError> for DirectoryApplicationError {
    fn from(value: DirectoryDomainError) -> Self {
        match value {
            DirectoryDomainError::InvalidDirectoryId => Self::InvalidDirectoryId,
            DirectoryDomainError::InvalidParentId => Self::InvalidParentId,
            DirectoryDomainError::DirectoryIdRequired => Self::DirectoryIdRequired,
            DirectoryDomainError::DirectoryNameRequired => Self::DirectoryNameRequired,
            DirectoryDomainError::DirectoryNameReserved => Self::DirectoryNameReserved,
            DirectoryDomainError::DirectoryNameInvalidFormat => Self::DirectoryNameInvalidFormat,
        }
    }
}

impl From<RepositoryError> for DirectoryApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::DirectoryNotFound => Self::DirectoryNotFound,
            RepositoryError::ParentDirectoryNotFound => Self::ParentDirectoryNotFound,
            RepositoryError::DirectoryNameConflict => Self::DirectoryNameConflict,
            RepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}