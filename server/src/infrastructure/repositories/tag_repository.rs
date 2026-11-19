use toasty::{Db, Executor, Transaction};

use crate::{
    domain::{
        ChangeTarget, EntityCreate,
        library::{
            book::{
                entity::Book,
                value_objects::{BookId, BookStatus},
            },
            tag::{
                entity::{BookTag, TagDefinition},
                value_objects::{BookTagStatus, TagId},
            },
        },
    },
    infrastructure::db::sqlite::sqlite_error,
};

#[derive(Debug, thiserror::Error)]
pub enum TagRepositoryError {
    #[error("tag not found")]
    NotFound,

    #[error("parent tag not found")]
    ParentNotFound,

    #[error("a sibling tag with this name already exists")]
    SiblingNameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

#[derive(Clone)]
pub struct TagRepository {
    db: Db,
}

impl TagRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    pub async fn create(
        &mut self,
        definition: TagDefinition,
    ) -> Result<TagDefinition, TagRepositoryError> {
        definition
            .create()
            .exec(&mut self.db)
            .await
            .map_err(map_tag_db_error)
    }

    pub async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        list_tag_definitions(&mut self.db).await
    }

    pub async fn active_book_exists(
        &mut self,
        book_id: &BookId,
    ) -> Result<bool, TagRepositoryError> {
        active_book_exists(&mut self.db, book_id).await
    }

    pub async fn list_active_book_tags(
        &mut self,
        book_id: &BookId,
    ) -> Result<Vec<BookTag>, TagRepositoryError> {
        let bindings = BookTag::filter(BookTag::fields().book_id().eq(book_id.clone()))
            .order_by(BookTag::fields().tag_id().asc())
            .exec(&mut self.db)
            .await
            .map_err(map_tag_db_error)?;
        Ok(bindings
            .into_iter()
            .filter(|binding| binding.status() == BookTagStatus::Active)
            .collect())
    }

    pub async fn begin_transaction(&mut self) -> Result<TagTransaction<'_>, TagRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_tag_db_error)?;
        Ok(TagTransaction { tx })
    }
}

pub struct TagTransaction<'a> {
    tx: Transaction<'a>,
}

impl TagTransaction<'_> {
    pub async fn find_definition_by_id(
        &mut self,
        id: &TagId,
    ) -> Result<TagDefinition, TagRepositoryError> {
        TagDefinition::get_by_id(&mut self.tx, id)
            .await
            .map_err(map_tag_db_error)
    }

    pub async fn definition_exists(&mut self, id: &TagId) -> Result<bool, TagRepositoryError> {
        Ok(
            TagDefinition::filter(TagDefinition::fields().id().eq(id.clone()))
                .first()
                .exec(&mut self.tx)
                .await
                .map_err(map_tag_db_error)?
                .is_some(),
        )
    }

    pub async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        list_tag_definitions(&mut self.tx).await
    }

    pub async fn delete_definitions(
        &mut self,
        definitions: Vec<TagDefinition>,
    ) -> Result<(), TagRepositoryError> {
        for definition in definitions {
            definition
                .delete()
                .exec(&mut self.tx)
                .await
                .map_err(map_tag_db_error)?;
        }
        Ok(())
    }

    pub async fn active_book_exists(
        &mut self,
        book_id: &BookId,
    ) -> Result<bool, TagRepositoryError> {
        active_book_exists(&mut self.tx, book_id).await
    }

    pub async fn find_binding(
        &mut self,
        book_id: &BookId,
        tag_id: &TagId,
    ) -> Result<Option<BookTag>, TagRepositoryError> {
        BookTag::filter(
            BookTag::fields()
                .book_id()
                .eq(book_id.clone())
                .and(BookTag::fields().tag_id().eq(tag_id.clone())),
        )
        .first()
        .exec(&mut self.tx)
        .await
        .map_err(map_tag_db_error)
    }

    pub async fn create_binding(&mut self, binding: BookTag) -> Result<(), TagRepositoryError> {
        binding
            .create()
            .exec(&mut self.tx)
            .await
            .map_err(map_tag_db_error)?;
        Ok(())
    }

    pub async fn delete_binding(&mut self, binding: BookTag) -> Result<(), TagRepositoryError> {
        binding
            .delete()
            .exec(&mut self.tx)
            .await
            .map_err(map_tag_db_error)
    }

    pub async fn commit(self) -> Result<(), TagRepositoryError> {
        self.tx.commit().await.map_err(map_tag_db_error)
    }
}

impl ChangeTarget<TagDefinition> for TagTransaction<'_> {
    type Error = TagRepositoryError;

    fn executor(&mut self) -> &mut dyn toasty::codegen_support::Executor {
        &mut self.tx
    }

    fn map_error(error: toasty::Error) -> Self::Error {
        map_tag_db_error(error)
    }
}

impl ChangeTarget<BookTag> for TagTransaction<'_> {
    type Error = TagRepositoryError;

    fn executor(&mut self) -> &mut dyn toasty::codegen_support::Executor {
        &mut self.tx
    }

    fn map_error(error: toasty::Error) -> Self::Error {
        map_tag_db_error(error)
    }
}

async fn list_tag_definitions(
    executor: &mut impl Executor,
) -> Result<Vec<TagDefinition>, TagRepositoryError> {
    TagDefinition::all()
        .order_by(TagDefinition::fields().name().asc())
        .order_by(TagDefinition::fields().id().asc())
        .exec(executor)
        .await
        .map_err(map_tag_db_error)
}

async fn active_book_exists(
    executor: &mut impl Executor,
    id: &BookId,
) -> Result<bool, TagRepositoryError> {
    let book = Book::filter(Book::fields().id().eq(id.clone()))
        .first()
        .exec(executor)
        .await
        .map_err(map_tag_db_error)?;
    Ok(book.is_some_and(|book| book.status() == BookStatus::Active))
}

fn map_tag_db_error(error: toasty::Error) -> TagRepositoryError {
    if error.is_record_not_found() {
        return TagRepositoryError::NotFound;
    }
    if let Some(rusqlite::Error::SqliteFailure(code, message)) = sqlite_error(&error) {
        match (code.extended_code, message.as_deref()) {
            (rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE, Some(msg))
                if msg.contains("tag_definitions.name") =>
            {
                return TagRepositoryError::SiblingNameConflict;
            }
            (rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY, _) => {
                return TagRepositoryError::ParentNotFound;
            }
            _ => {}
        }
    }
    TagRepositoryError::Storage(error.into())
}
