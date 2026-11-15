use derive_new::new;
use toasty::{Db, Executor};

use crate::{
    application::{
        MapDbResult,
        errors::MapToastyError,
        library::{
            book::errors::BookApplicationError,
            errors::LibraryApplicationError,
            tag::{
                commands::{
                    BindBookTagCommand, CreateTagDefinitionCommand, UndefineTagCommand,
                    MoveTagCommand, RenameTagCommand, UnbindBookTagCommand,
                },
                dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
                errors::TagApplicationError,
            },
        },
    },
    domain::{
        EntityCreate, EntityId, RequiresVerificationEntity,
        library::{
            book::{entity::Book, value_objects::BookId},
            tag::{
                entity::{BookTag, TagDefinition},
                errors::TagDomainError,
                tree::{TagParentChange, TagTree},
                value_objects::{TagId, TagName, TagValueRule},
            },
        },
    },
};

#[derive(new)]
pub struct TagService {
    db: Db,
}

impl TagService {
    pub async fn define_tag(
        &mut self,
        command: CreateTagDefinitionCommand,
    ) -> Result<TagDefinitionOutput, LibraryApplicationError> {
        let name = TagName::parse(command.name)?;
        let parent_id = command.parent_id.map(TagId::parse).transpose()?;
        let value_rule = TagValueRule::new(command.allows_value, command.value_regex)?;
        if let Some(parent_id) = parent_id {
            let mut tx = self
                .db
                .transaction()
                .await
                .map_err(TagDefinition::map_db_err)?;
            if !tag_exists(&mut tx, &parent_id).await? {
                return Err(TagApplicationError::ParentNotFound.into());
            }
            let tag_def = TagDefinition::new(name, Some(parent_id), value_rule)
                .create()
                .exec(&mut tx)
                .await
                .map_result()?;
            tx.commit().await.map_err(TagDefinition::map_db_err)?;
            Ok(tag_def.into())
        } else {
            Ok(TagDefinition::new(name, None, value_rule)
                .create()
                .exec(&mut self.db)
                .await
                .map_result()?
                .into())
        }
    }

    pub async fn undefine_tag(
        &mut self,
        command: UndefineTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let mut tx = self
            .db
            .transaction()
            .await
            .map_err(TagDefinition::map_db_err)?;
        let tree = TagTree::new(definitions(&mut tx).await?)?;
        let deletion = tree.plan_delete(&id, command.strategy)?;
        for change in deletion.promoted_children() {
            apply_parent_change(&mut tx, change).await?;
        }
        for id in deletion.deleted_ids() {
            if !tag_exists(&mut tx, id).await? {
                return Err(TagDomainError::NotFound.into());
            }
            TagDefinition::filter(TagDefinition::fields().id().eq(id.clone()))
                .delete()
                .exec(&mut tx)
                .await
                .map_err(TagDefinition::map_db_err)?;
        }
        tx.commit().await.map_err(TagDefinition::map_db_err)
    }

    pub async fn rename_tag(
        &mut self,
        command: RenameTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let name = TagName::parse(command.name)?;
        let mut tx = self
            .db
            .transaction()
            .await
            .map_err(TagDefinition::map_db_err)?;
        let mut definition = TagDefinition::get_by_id(&mut tx, &id).await.map_result()?;
        definition
            .rename(name)?
            .exec(&mut tx)
            .await
            .map_err(TagDefinition::map_db_err)?;
        tx.commit().await.map_err(TagDefinition::map_db_err)
    }

    pub async fn move_tag(
        &mut self,
        command: MoveTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let parent_id = command.parent_id.map(TagId::parse).transpose()?;
        let mut tx = self
            .db
            .transaction()
            .await
            .map_err(TagDefinition::map_db_err)?;
        let tree = TagTree::new(definitions(&mut tx).await?)?;
        let change = tree.plan_move(&id, parent_id.as_ref())?;
        apply_parent_change(&mut tx, &change).await?;
        tx.commit().await.map_err(TagDefinition::map_db_err)
    }

    pub async fn get_tree(&mut self) -> Result<Vec<TagTreeNodeOutput>, LibraryApplicationError> {
        let tree = TagTree::new(definitions(&mut self.db).await?)?;
        Ok(tree
            .children_of(None)
            .map(|tag| tree_node(&tree, tag))
            .collect())
    }

    pub async fn get_book_tags(
        &mut self,
        book_id: String,
    ) -> Result<Vec<BookTagOutput>, LibraryApplicationError> {
        let book_id = BookId::parse(book_id)?;
        let mut tx = self.db.transaction().await.map_err(BookTag::map_db_err)?;
        if !book_exists(&mut tx, &book_id).await? {
            return Err(BookApplicationError::NotFound.into());
        }
        let bindings = BookTag::filter(BookTag::fields().book_id().eq(book_id))
            .order_by(BookTag::fields().tag_id().asc())
            .exec(&mut tx)
            .await
            .map_err(BookTag::map_db_err)?;
        tx.commit().await.map_err(BookTag::map_db_err)?;
        Ok(bindings
            .into_iter()
            .map(|binding| BookTagOutput {
                tag_id: binding.tag_id().to_string(),
                value: binding.value().map(str::to_owned),
            })
            .collect())
    }

    pub async fn bind_book_tag(
        &mut self,
        command: BindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let tag_id = TagId::parse(command.tag_id)?;
        let book_id = BookId::parse(command.book_id)?;
        let mut tx = self.db.transaction().await.map_err(BookTag::map_db_err)?;
        let definition = TagDefinition::get_by_id(&mut tx, &tag_id)
            .await
            .map_result()?;
        let binding = BookTag::new(&definition, book_id, command.value)?;
        check_binding_target(&mut tx, binding.book_id(), binding.tag_id()).await?;
        let filter = || {
            BookTag::fields()
                .book_id()
                .eq(binding.book_id().clone())
                .and(BookTag::fields().tag_id().eq(binding.tag_id().clone()))
        };
        if let Some(mut existing) = BookTag::filter(filter())
            .first()
            .exec(&mut tx)
            .await
            .map_err(BookTag::map_db_err)?
        {
            existing
                .change_value(&definition, binding.value().map(str::to_owned))?
                .exec(&mut tx)
                .await
                .map_err(BookTag::map_db_err)?;
        } else {
            binding
                .create()
                .exec(&mut tx)
                .await
                .map_err(BookTag::map_db_err)?;
        }
        tx.commit().await.map_err(BookTag::map_db_err)
    }

    pub async fn unbind_book_tag(
        &mut self,
        command: UnbindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let book_id = BookId::parse(command.book_id)?;
        let tag_id = TagId::parse(command.tag_id)?;
        let mut tx = self.db.transaction().await.map_err(BookTag::map_db_err)?;
        let filter = || {
            BookTag::fields()
                .book_id()
                .eq(book_id.clone())
                .and(BookTag::fields().tag_id().eq(tag_id.clone()))
        };
        if BookTag::filter(filter())
            .first()
            .exec(&mut tx)
            .await
            .map_err(BookTag::map_db_err)?
            .is_none()
        {
            check_binding_target(&mut tx, &book_id, &tag_id).await?;
        } else {
            BookTag::filter(filter())
                .delete()
                .exec(&mut tx)
                .await
                .map_err(BookTag::map_db_err)?;
        }
        tx.commit().await.map_err(BookTag::map_db_err)
    }
}

fn tree_node(tree: &TagTree, definition: &TagDefinition) -> TagTreeNodeOutput {
    TagTreeNodeOutput {
        tag: definition.into(),
        children: tree
            .children_of(Some(definition.id()))
            .map(|child| tree_node(tree, child))
            .collect(),
    }
}

async fn definitions(
    executor: &mut dyn Executor,
) -> Result<Vec<TagDefinition>, LibraryApplicationError> {
    let definitions = TagDefinition::all()
        .order_by(TagDefinition::fields().name().asc())
        .order_by(TagDefinition::fields().id().asc())
        .exec(executor)
        .await
        .map_err(TagDefinition::map_db_err)?;
    definitions
        .into_iter()
        .map(|definition| definition.verify().map_err(Into::into))
        .collect()
}

async fn tag_exists(
    executor: &mut dyn Executor,
    id: &TagId,
) -> Result<bool, LibraryApplicationError> {
    Ok(
        TagDefinition::filter(TagDefinition::fields().id().eq(id.clone()))
            .first()
            .exec(executor)
            .await
            .map_err(TagDefinition::map_db_err)?
            .is_some(),
    )
}

async fn book_exists(
    executor: &mut dyn Executor,
    id: &BookId,
) -> Result<bool, LibraryApplicationError> {
    Ok(Book::filter(Book::fields().id().eq(id.clone()))
        .first()
        .exec(executor)
        .await
        .map_err(Book::map_db_err)?
        .is_some())
}

async fn check_binding_target(
    executor: &mut dyn Executor,
    book_id: &BookId,
    tag_id: &TagId,
) -> Result<(), LibraryApplicationError> {
    if !tag_exists(executor, tag_id).await? {
        return Err(TagDomainError::NotFound.into());
    }
    if !book_exists(executor, book_id).await? {
        return Err(BookApplicationError::NotFound.into());
    }
    Ok(())
}

async fn apply_parent_change(
    executor: &mut dyn Executor,
    change: &TagParentChange,
) -> Result<(), LibraryApplicationError> {
    let mut definition = TagDefinition::get_by_id(executor, change.id())
        .await
        .map_result()?;
    definition
        .change_parent(change.parent_id().cloned())?
        .exec(executor)
        .await
        .map_err(TagDefinition::map_db_err)
}
