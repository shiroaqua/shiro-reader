use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        bookshelf::{
            commands::{
                CreateBookshelfCommand, CreateBookshelfOutput, DeleteBookshelfCommand,
                GetAllBookshelfOutput, GetBookshelfCommand, GetBookshelfOutput,
                RenameBookshelfCommand,
            },
            errors::BookshelfApplicationError,
            ports::BookshelfRepository,
        },
        errors::LibraryApplicationError,
    },
    domain::library::bookshelf::{
        entity::Bookshelf,
        errors::BookshelfDomainError,
        value_objects::{BookshelfId, BookshelfName},
    },
    infrastructure::repositories::errors::RepositoryError,
    shared::time::now_ms,
};

#[derive(new)]
pub struct BookshelfService {
    pub repositories: Arc<dyn BookshelfRepository>,
}

impl BookshelfService {
    pub async fn create_bookshelf(
        &self,
        command: CreateBookshelfCommand,
    ) -> Result<CreateBookshelfOutput, LibraryApplicationError> {
        let id = BookshelfId::new();
        let name = BookshelfName::parse(command.name)?;
        let now = now_ms();

        let result = self
            .repositories
            .create(&Bookshelf::new(id, name, now, now))
            .await
            .map_err(BookshelfApplicationError::from)?;

        Ok(CreateBookshelfOutput {
            id: result.id,
            created_at: result.created_at,
        })
    }
    pub async fn rename_bookshelf(
        &self,
        command: RenameBookshelfCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookshelfId::parse(command.id)?;
        let new_name = BookshelfName::parse(command.name)?;
        self.repositories
            .rename(&id, &new_name)
            .await
            .map_err(BookshelfApplicationError::from)?;
        Ok(())
    }

    pub async fn delete_bookshelf(
        &self,
        command: DeleteBookshelfCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookshelfId::parse(command.id)?;

        self.repositories
            .delete(&id)
            .await
            .map_err(BookshelfApplicationError::from)?;

        Ok(())
    }

    pub async fn get_bookshelf(
        &self,
        command: GetBookshelfCommand,
    ) -> Result<GetBookshelfOutput, LibraryApplicationError> {
        let id = BookshelfId::parse(command.id)?;
        Ok(self
            .repositories
            .get(&id)
            .await
            .map_err(BookshelfApplicationError::from)?
            .into())
    }

    pub async fn get_all_bookshelf(
        &self,
    ) -> Result<GetAllBookshelfOutput, LibraryApplicationError> {
        Ok(GetAllBookshelfOutput(
            self.repositories
                .get_all()
                .await
                .map_err(BookshelfApplicationError::from)?
                .into_iter()
                .map(Into::into)
                .collect(),
        ))
    }
}

impl From<BookshelfDomainError> for BookshelfApplicationError {
    fn from(value: BookshelfDomainError) -> Self {
        match value {
            BookshelfDomainError::InvalidBookshelfId => Self::InvalidBookshelfId,
            BookshelfDomainError::MissingBookshelfName => Self::MissingBookshelfName,
            BookshelfDomainError::InvalidBookshelfNameFormat => Self::InvalidBookshelfNameFormat,
        }
    }
}

impl From<RepositoryError> for BookshelfApplicationError {
    fn from(value: RepositoryError) -> Self {
        match value {
            RepositoryError::BookshelfNotFound => Self::BookshelfNotFound,
            RepositoryError::BookshelfNameConflict => Self::BookshelfNameConflict,
            RepositoryError::Storage(error) => Self::Storage(error),
            _ => Self::Storage(anyhow::anyhow!("unrelated error")),
        }
    }
}
