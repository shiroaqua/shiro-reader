use std::sync::Arc;

use derive_new::new;

use crate::{
    application::library::{
        bookshelf::{
            commands::{CreateBookshelfCommand, DeleteBookshelfCommand, RenameBookshelfCommand}, dto::{CreateBookshelfOutput, GetBookshelfOutput}, errors::BookshelfApplicationError, ports::BookshelfRepository, queries::GetBookshelvesQuery,
        }, errors::LibraryApplicationError,
    }, domain::library::bookshelf::{
        entity::Bookshelf,
        errors::BookshelfDomainError,
        value_objects::{BookshelfId, BookshelfName},
    },
};

#[derive(new)]
pub struct BookshelfService {
    repositories: Arc<dyn BookshelfRepository>,
}

impl BookshelfService {
    pub async fn create_bookshelf(
        &self,
        command: CreateBookshelfCommand,
    ) -> Result<CreateBookshelfOutput, LibraryApplicationError> {
        let id = BookshelfId::new();
        let name = BookshelfName::parse(command.name)?;
        let bookshelf = Bookshelf::new(id, name);
        self.repositories.create(&bookshelf).await?;

        Ok(CreateBookshelfOutput {
            id: bookshelf.id().to_string(),
            created_at: bookshelf.created_at(),
        })
    }

    pub async fn rename_bookshelf(
        &self,
        command: RenameBookshelfCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookshelfId::parse(command.id)?;
        let new_name = BookshelfName::parse(command.name)?;
        
        let mut bookshelf = self.repositories.find_by_id(&id).await?;
        bookshelf.rename(new_name)?;
        self.repositories.commit(bookshelf).await?;
        Ok(())
    }

    pub async fn delete_bookshelf(
        &self,
        command: DeleteBookshelfCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = BookshelfId::parse(command.id)?;

        self.repositories.delete(&id).await?;

        Ok(())
    }

    pub async fn get_bookshelves(
        &self,
        query: GetBookshelvesQuery,
    ) -> Result<Vec<GetBookshelfOutput>, LibraryApplicationError> {
        let ids = query
            .ids
            .iter()
            .map(|id| BookshelfId::parse(id))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.repositories.list(ids).await?)
    }
}

impl From<BookshelfDomainError> for BookshelfApplicationError {
    fn from(value: BookshelfDomainError) -> Self {
        match value {
            BookshelfDomainError::InvalidId => Self::InvalidId,
            BookshelfDomainError::MissingName => Self::MissingName,
            BookshelfDomainError::InvalidNameFormat => Self::InvalidNameFormat,
            BookshelfDomainError::UnchangedName => Self::UnchangedName,
        }
    }
}
