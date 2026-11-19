use derive_new::new;

use crate::{
    application::library::{
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
    domain::library::{
        book::value_objects::BookId,
        tag::{
            entity::{BookTag, TagDefinition},
            errors::TagDomainError,
            tree::TagTree,
            value_objects::{BookTagStatus, TagColor, TagId, TagName, TagValueRule},
        },
    },
    infrastructure::repositories::tag_repository::TagRepository,
};

#[derive(new)]
pub struct TagService {
    repository: TagRepository,
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
        let tag_def = self
            .repository
            .create(TagDefinition::new(name, parent_id, color, value_rule))
            .await?;
        Ok(tag_def.into())
    }

    pub async fn undefine_tag(
        &mut self,
        command: UndefineTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut tree = TagTree::new(tx.list_definitions().await?)?;
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
            definition.change_parent(parent_id)?.exec(&mut tx).await?;
        }
        let deleted_definitions = tree.take_definitions(&deleted_ids)?;
        tx.delete_definitions(deleted_definitions).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn rename_tag(
        &mut self,
        command: RenameTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let name = TagName::parse(command.name)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut definition = tx.find_definition_by_id(&id).await?;
        definition.rename(name)?.exec(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn change_tag_color(
        &mut self,
        command: ChangeTagColorCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let color = TagColor::parse(command.color)?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut definition = tx.find_definition_by_id(&id).await?;
        definition.change_color(color)?.exec(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn move_tag(
        &mut self,
        command: MoveTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let parent_id = command.parent_id.map(TagId::parse).transpose()?;
        let mut tx = self.repository.begin_transaction().await?;
        let mut tree = TagTree::new(tx.list_definitions().await?)?;
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
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_tree(&mut self) -> Result<Vec<TagTreeNodeOutput>, LibraryApplicationError> {
        let tree = TagTree::new(self.repository.list_definitions().await?)?;
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
        if !self.repository.active_book_exists(&book_id).await? {
            return Err(BookApplicationError::NotFound.into());
        }
        Ok(self
            .repository
            .list_active_book_tags(&book_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    pub async fn bind_book_tag(
        &mut self,
        command: BindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let tag_id = TagId::parse(command.tag_id)?;
        let book_id = BookId::parse(command.book_id)?;
        let mut tx = self.repository.begin_transaction().await?;
        let definition = tx.find_definition_by_id(&tag_id).await?;
        let binding = BookTag::new(&definition, book_id, command.value)?;
        if !tx.active_book_exists(binding.book_id()).await? {
            return Err(BookApplicationError::NotFound.into());
        }
        if let Some(mut existing) = tx.find_binding(binding.book_id(), binding.tag_id()).await? {
            existing
                .rebind(&definition, binding.into_inner().value)?
                .exec(&mut tx)
                .await?;
        } else {
            tx.create_binding(binding).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn unbind_book_tag(
        &mut self,
        command: UnbindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let book_id = BookId::parse(command.book_id)?;
        let tag_id = TagId::parse(command.tag_id)?;
        let mut tx = self.repository.begin_transaction().await?;
        if let Some(binding) = tx.find_binding(&book_id, &tag_id).await? {
            if binding.status() != BookTagStatus::Active {
                if !tx.active_book_exists(&book_id).await? {
                    return Err(BookApplicationError::NotFound.into());
                }
            } else {
                tx.delete_binding(binding).await?;
            }
        } else {
            if !tx.definition_exists(&tag_id).await? {
                return Err(TagDomainError::NotFound.into());
            }
            if !tx.active_book_exists(&book_id).await? {
                return Err(BookApplicationError::NotFound.into());
            }
        }
        tx.commit().await?;
        Ok(())
    }
}
