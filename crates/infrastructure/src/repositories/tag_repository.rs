use shiro_application::library::ports::{TagRepository, TagRepositoryError, TagTransaction};
use shiro_domain::library::{
    book::value_objects::BookId,
    tag::{
        entity::{
            BookTag, BookTagChange, TagDefinition, TagDefinitionChange, TagDefinitionFields,
        },
        value_objects::{BookTagStatus, TagId},
    },
};
use toasty::{Db, Executor, Transaction};

use crate::{
    db::{
        models::{BookRecord, BookTagRecord, TagDefinitionRecord},
        scalars::DbTimestamp,
        sqlite::sqlite_error,
    },
    repositories::book_repository::apply_book_tag_change,
};

#[derive(Clone)]
pub struct SqliteTagRepository {
    db: Db,
}

impl SqliteTagRepository {
    pub fn new(db: Db) -> Self {
        Self { db }
    }
}

impl TagRepository for SqliteTagRepository {
    type Tx<'a> = SqliteTagTransaction<'a>;

    async fn create(
        &mut self,
        definition: TagDefinition,
    ) -> Result<TagDefinition, TagRepositoryError> {
        let record = TagDefinitionRecord::from(definition)
            .into_create()
            .exec(&mut self.db)
            .await
            .map_err(map_tag_db_error)?;
        TagDefinition::try_from(record).map_err(TagRepositoryError::Storage)
    }

    async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        list_tag_definitions(&mut self.db).await
    }

    async fn active_book_exists(&mut self, book_id: &BookId) -> Result<bool, TagRepositoryError> {
        active_book_exists(&mut self.db, book_id).await
    }

    async fn list_active_book_tags(
        &mut self,
        book_id: &BookId,
    ) -> Result<Vec<BookTag>, TagRepositoryError> {
        let bindings =
            BookTagRecord::filter(BookTagRecord::fields().book_id().eq(book_id.as_uuid()))
                .order_by(BookTagRecord::fields().tag_id().asc())
                .exec(&mut self.db)
                .await
                .map_err(map_tag_db_error)?;
        Ok(bindings
            .into_iter()
            .map(BookTag::from)
            .filter(|binding| binding.status() == BookTagStatus::Active)
            .collect())
    }

    async fn begin_transaction(&mut self) -> Result<SqliteTagTransaction<'_>, TagRepositoryError> {
        let tx = self.db.transaction().await.map_err(map_tag_db_error)?;
        Ok(SqliteTagTransaction { tx })
    }
}

pub struct SqliteTagTransaction<'a> {
    tx: Transaction<'a>,
}

impl TagTransaction for SqliteTagTransaction<'_> {
    async fn find_definition_by_id(
        &mut self,
        id: &TagId,
    ) -> Result<TagDefinition, TagRepositoryError> {
        let record = TagDefinitionRecord::get_by_id(&mut self.tx, id.as_uuid())
            .await
            .map_err(map_tag_db_error)?;
        TagDefinition::try_from(record).map_err(TagRepositoryError::Storage)
    }

    async fn definition_exists(&mut self, id: &TagId) -> Result<bool, TagRepositoryError> {
        Ok(
            TagDefinitionRecord::filter(TagDefinitionRecord::fields().id().eq(id.as_uuid()))
                .first()
                .exec(&mut self.tx)
                .await
                .map_err(map_tag_db_error)?
                .is_some(),
        )
    }

    async fn list_definitions(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError> {
        list_tag_definitions(&mut self.tx).await
    }

    async fn apply_definition_change(
        &mut self,
        change: TagDefinitionChange<'_>,
    ) -> Result<(), TagRepositoryError> {
        if change.is_empty() {
            return Ok(());
        }
        let entity = change.entity();
        let fields = change.fields();
        let mut update = TagDefinitionRecord::filter(
            TagDefinitionRecord::fields().id().eq(entity.id().as_uuid()),
        )
        .update();
        if fields.contains(TagDefinitionFields::NAME) {
            update.set_name(entity.name().as_str().to_owned());
        }
        if fields.contains(TagDefinitionFields::PARENT_ID) {
            update.set_parent_id(entity.parent_id().map(TagId::as_uuid));
        }
        if fields.contains(TagDefinitionFields::COLOR) {
            update.set_color(entity.color().as_str().to_owned());
        }
        if fields.contains(TagDefinitionFields::UPDATED_AT) {
            update.set_updated_at(DbTimestamp(entity.updated_at()));
        }
        update.exec(&mut self.tx).await.map_err(map_tag_db_error)
    }

    async fn delete_definitions(
        &mut self,
        definitions: Vec<TagDefinition>,
    ) -> Result<(), TagRepositoryError> {
        for definition in definitions {
            TagDefinitionRecord::filter(
                TagDefinitionRecord::fields()
                    .id()
                    .eq(definition.id().as_uuid()),
            )
            .delete()
            .exec(&mut self.tx)
            .await
            .map_err(map_tag_db_error)?;
        }
        Ok(())
    }

    async fn active_book_exists(&mut self, book_id: &BookId) -> Result<bool, TagRepositoryError> {
        active_book_exists(&mut self.tx, book_id).await
    }

    async fn find_binding(
        &mut self,
        book_id: &BookId,
        tag_id: &TagId,
    ) -> Result<Option<BookTag>, TagRepositoryError> {
        let record = BookTagRecord::filter(
            BookTagRecord::fields()
                .book_id()
                .eq(book_id.as_uuid())
                .and(BookTagRecord::fields().tag_id().eq(tag_id.as_uuid())),
        )
        .first()
        .exec(&mut self.tx)
        .await
        .map_err(map_tag_db_error)?;
        Ok(record.map(BookTag::from))
    }

    async fn create_binding(&mut self, binding: BookTag) -> Result<(), TagRepositoryError> {
        BookTagRecord::from(binding)
            .into_create()
            .exec(&mut self.tx)
            .await
            .map_err(map_tag_db_error)?;
        Ok(())
    }

    async fn apply_binding_change(
        &mut self,
        change: BookTagChange<'_>,
    ) -> Result<(), TagRepositoryError> {
        apply_book_tag_change(&mut self.tx, change)
            .await
            .map_err(map_tag_db_error)
    }

    async fn delete_binding(&mut self, binding: BookTag) -> Result<(), TagRepositoryError> {
        BookTagRecord::filter(
            BookTagRecord::fields()
                .tag_id()
                .eq(binding.tag_id().as_uuid())
                .and(
                    BookTagRecord::fields()
                        .book_id()
                        .eq(binding.book_id().as_uuid()),
                ),
        )
        .delete()
        .exec(&mut self.tx)
        .await
        .map_err(map_tag_db_error)
    }

    async fn commit(self) -> Result<(), TagRepositoryError> {
        self.tx.commit().await.map_err(map_tag_db_error)
    }
}

async fn list_tag_definitions(
    executor: &mut impl Executor,
) -> Result<Vec<TagDefinition>, TagRepositoryError> {
    TagDefinitionRecord::all()
        .order_by(TagDefinitionRecord::fields().name().asc())
        .order_by(TagDefinitionRecord::fields().id().asc())
        .exec(executor)
        .await
        .map_err(map_tag_db_error)?
        .into_iter()
        .map(TagDefinition::try_from)
        .collect::<Result<Vec<_>, _>>()
        .map_err(TagRepositoryError::Storage)
}

async fn active_book_exists(
    executor: &mut impl Executor,
    id: &BookId,
) -> Result<bool, TagRepositoryError> {
    let book = BookRecord::filter(BookRecord::fields().id().eq(id.as_uuid()))
        .first()
        .exec(executor)
        .await
        .map_err(map_tag_db_error)?;
    Ok(book.is_some_and(|book| book.deleted_at.is_none()))
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
