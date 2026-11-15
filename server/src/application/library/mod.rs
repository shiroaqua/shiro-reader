use crate::{
    application::{MapDbResult, errors::MapToastyError, library::errors::LibraryApplicationError},
    domain::{
        RequiresVerificationEntity,
        library::{book::errors::BookDomainError, tag::entity::TagDefinition},
    },
};

pub mod book;
pub mod errors;
pub mod tag;


impl<T> MapDbResult<T, LibraryApplicationError> for Result<T, toasty::Error>
where
    T: RequiresVerificationEntity<BookDomainError> + MapToastyError<LibraryApplicationError>,
{
    fn map_result(self) -> Result<T, LibraryApplicationError> {
        let value = self.map_err(T::map_db_err)?;
        value.verify().map_err(Into::into)
    }
}

impl MapDbResult<TagDefinition, LibraryApplicationError> for Result<TagDefinition, toasty::Error> {
    fn map_result(self) -> Result<TagDefinition, LibraryApplicationError> {
        let value = self.map_err(TagDefinition::map_db_err)?;
        value.verify().map_err(Into::into)
    }
}
