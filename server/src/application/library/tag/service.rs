use derive_new::new;
use toasty::{Db, Executor};

use crate::{
    application::{
        errors::MapToastyError,
        library::{
            book::errors::BookApplicationError,
            errors::LibraryApplicationError,
            tag::{
                commands::{
                    BindBookTagCommand, ChangeTagColorCommand, CreateTagDefinitionCommand,
                    MoveTagCommand, RenameTagCommand, UnbindBookTagCommand, UndefineTagCommand,
                },
                dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
                errors::TagApplicationError,
            },
        },
    },
    domain::{
        EntityCreate,
        library::{
            book::{
                entity::Book,
                value_objects::{BookId, BookStatus},
            },
            tag::{
                entity::{BookTag, TagDefinition},
                errors::TagDomainError,
                tree::TagTree,
                value_objects::{BookTagStatus, TagColor, TagId, TagName, TagValueRule},
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
        let color = TagColor::parse(command.color)?;
        let value_rule = TagValueRule::new(command.allows_value, command.value_regex)?;
        let tag_def = TagDefinition::new(name, parent_id, color, value_rule)
            .create()
            .exec(&mut self.db)
            .await
            .map_err(TagDefinition::map_db_err)?;
        Ok(tag_def.into())
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
        let mut tree = TagTree::new(definitions(&mut tx).await?)?;
        let deletion = tree.plan_delete(&id, command.strategy)?;
        for (index, change) in deletion.promoted_children().iter().enumerate() {
            let child = tree
                .definition(change.id())
                .ok_or(TagDomainError::NotFound)?;
            if tree.has_sibling_name(change.parent_id(), child.name(), Some(&id)) {
                return Err(TagApplicationError::SiblingNameConflict.into());
            }
            for previous in &deletion.promoted_children()[..index] {
                let previous = tree
                    .definition(previous.id())
                    .ok_or(TagDomainError::NotFound)?;
                if previous.name() == child.name() {
                    return Err(TagApplicationError::SiblingNameConflict.into());
                }
            }
        }
        let (deleted_ids, promoted_children) = deletion.into_parts();
        for change in promoted_children {
            let (child_id, parent_id) = change.into_parts();
            let definition = tree
                .definition_mut(&child_id)
                .ok_or(TagDomainError::NotFound)?;
            definition
                .change_parent(parent_id)?
                .exec(&mut tx)
                .await
                .map_err(TagDefinition::map_db_err)?;
        }
        for id in deleted_ids {
            TagDefinition::filter(TagDefinition::fields().id().eq(id))
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
        let mut definition = TagDefinition::get_by_id(&mut tx, &id).await.map_err(TagDefinition::map_db_err)?;
        definition
            .rename(name)?
            .exec(&mut tx)
            .await
            .map_err(TagDefinition::map_db_err)?;
        tx.commit().await.map_err(TagDefinition::map_db_err)
    }

    pub async fn change_tag_color(
        &mut self,
        command: ChangeTagColorCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let color = TagColor::parse(command.color)?;
        let mut tx = self
            .db
            .transaction()
            .await
            .map_err(TagDefinition::map_db_err)?;
        let mut definition = TagDefinition::get_by_id(&mut tx, &id).await.map_err(TagDefinition::map_db_err)?;
        definition
            .change_color(color)?
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
        let mut tree = TagTree::new(definitions(&mut tx).await?)?;
        let change = tree.plan_move(&id, parent_id.as_ref())?;
        let tag_name = tree
            .definition(&id)
            .ok_or(TagDomainError::NotFound)?
            .name();
        if tree.has_sibling_name(change.parent_id(), tag_name, Some(&id)) {
            return Err(TagApplicationError::SiblingNameConflict.into());
        }
        let (tag_id, new_parent_id) = change.into_parts();
        let definition = tree
            .definition_mut(&tag_id)
            .ok_or(TagDomainError::NotFound)?;
        definition
            .change_parent(new_parent_id)?
            .exec(&mut tx)
            .await
            .map_err(TagDefinition::map_db_err)?;
        tx.commit().await.map_err(TagDefinition::map_db_err)
    }

    pub async fn get_tree(&mut self) -> Result<Vec<TagTreeNodeOutput>, LibraryApplicationError> {
        let tree = TagTree::new(definitions(&mut self.db).await?)?;
        Ok(tree.into_nodes(|tag, children| TagTreeNodeOutput {
            tag: tag.into(),
            children,
        }))
    }

    pub async fn get_book_tags(
        &mut self,
        book_id: String,
    ) -> Result<Vec<BookTagOutput>, LibraryApplicationError> {
        let book_id = BookId::parse(book_id)?;
        if !book_exists(&mut self.db, &book_id).await? {
            return Err(BookApplicationError::NotFound.into());
        }
        let bindings = BookTag::filter(BookTag::fields().book_id().eq(book_id))
            .order_by(BookTag::fields().tag_id().asc())
            .exec(&mut self.db)
            .await
            .map_err(BookTag::map_db_err)?;
        Ok(bindings
            .into_iter()
            .filter_map(|binding| {
                (binding.status() == BookTagStatus::Active).then(|| {
                    let binding = binding.into_inner();
                    BookTagOutput {
                        tag_id: binding.tag_id.to_string(),
                        value: binding.value,
                    }
                })
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
            .map_err(TagDefinition::map_db_err)?;
        let binding = BookTag::new(&definition, book_id, command.value)?;
        if !book_exists(&mut tx, binding.book_id()).await? {
            return Err(BookApplicationError::NotFound.into());
        }
        if let Some(mut existing) = BookTag::filter(
            BookTag::fields()
                .book_id()
                .eq(binding.book_id().clone())
                .and(BookTag::fields().tag_id().eq(binding.tag_id().clone())),
        )
        .first()
        .exec(&mut tx)
        .await
        .map_err(BookTag::map_db_err)?
        {
            existing
                .rebind(&definition, binding.into_inner().value)?
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
        if let Some(binding) = BookTag::filter(
            BookTag::fields()
                .book_id()
                .eq(book_id.clone())
                .and(BookTag::fields().tag_id().eq(tag_id.clone())),
        )
        .first()
        .exec(&mut tx)
        .await
        .map_err(BookTag::map_db_err)?
        {
            if binding.status() != BookTagStatus::Active {
                if !book_exists(&mut tx, &book_id).await? {
                    return Err(BookApplicationError::NotFound.into());
                }
            } else {
                binding
                    .delete()
                    .exec(&mut tx)
                    .await
                    .map_err(BookTag::map_db_err)?;
            }
        } else {
            check_binding_target(&mut tx, &book_id, &tag_id).await?;
        }
        tx.commit().await.map_err(BookTag::map_db_err)
    }
}

async fn definitions(
    executor: &mut dyn Executor,
) -> Result<Vec<TagDefinition>, LibraryApplicationError> {
    TagDefinition::all()
        .order_by(TagDefinition::fields().name().asc())
        .order_by(TagDefinition::fields().id().asc())
        .exec(executor)
        .await
        .map_err(TagDefinition::map_db_err)
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
    let book = Book::filter(Book::fields().id().eq(id.clone()))
        .first()
        .exec(executor)
        .await
        .map_err(Book::map_db_err)?;
    let Some(book) = book else {
        return Ok(false);
    };
    Ok(book.status() == BookStatus::Active)
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
