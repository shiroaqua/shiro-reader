use derive_new::new;

use crate::{
    application::library::errors::LibraryApplicationError,
    domain::library::{
        book::value_objects::BookId,
        tag::{
            entity::{BookTag, TagDefinition},
            tree::TagTree,
            value_objects::{TagId, TagName, TagValueRule},
        },
    },
};

use super::{
    commands::{
        BindBookTagCommand, CreateTagDefinitionCommand, DeleteTagDefinitionCommand, MoveTagCommand,
        RenameTagCommand, UnbindBookTagCommand,
    },
    dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
    ports::TagRepository,
};

#[derive(new)]
pub struct TagService<T: TagRepository> {
    repository: T,
}

impl<T: TagRepository> TagService<T> {
    pub async fn define_tag(
        &self,
        command: CreateTagDefinitionCommand,
    ) -> Result<TagDefinitionOutput, LibraryApplicationError> {
        let name = TagName::parse(command.name)?;
        let parent_id = command.parent_id.map(TagId::parse).transpose()?;
        let value_rule = TagValueRule::new(command.allows_value, command.value_regex)?;
     
        let tag_def = TagDefinition::new(name, parent_id, value_rule);
        self.repository.create(&tag_def).await?;
        Ok((&tag_def).into())
    }

    
    pub async fn delete_tag_definition(
        &self,
        command: DeleteTagDefinitionCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let mut transaction = self.repository.begin_delete().await?;
        let tree = TagTree::new(transaction.list().await?)?;
        let deletion = tree.plan_delete(&id, command.strategy)?;
        transaction.commit(deletion).await?;
        Ok(())
    }

    pub async fn rename_tag(
        &self,
        command: RenameTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let name = TagName::parse(command.name)?;
        let mut tag_def = self.repository.find_by_id(&id).await?;
        tag_def.rename(name)?;
        self.repository.commit(tag_def).await?;
        Ok(())
    }

    pub async fn move_tag(
        &self,
        command: MoveTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let id = TagId::parse(command.id)?;
        let parent_id = command.parent_id.map(TagId::parse).transpose()?;
        let mut transaction = self.repository.begin_move().await?;
        let tree = TagTree::new(transaction.list().await?)?;
        let change= tree.plan_move(&id, parent_id.as_ref())?;
        transaction.commit(change).await?;
        Ok(())
    }

    pub async fn get_tree(&self) -> Result<Vec<TagTreeNodeOutput>, LibraryApplicationError> {
        let tree = TagTree::new(self.repository.list().await?)?;
        Ok(tree.children_of(None).map(|tag| tree_node(&tree, tag)).collect())
    }


    pub async fn get_book_tags(
        &self,
        book_id: String,
    ) -> Result<Vec<BookTagOutput>, LibraryApplicationError> {
        let book_id = BookId::parse(book_id)?;
        Ok(self.repository.list_book_tags(&book_id).await?)
    }

    pub async fn bind_book_tag(
        &self,
        command: BindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let tag_id = TagId::parse(command.tag_id)?;
        let book_id = BookId::parse(command.book_id)?;
        let tag_def = self.repository.find_by_id(&tag_id).await?;
        let binding = BookTag::new(&tag_def, book_id, command.value)?;
        self.repository.bind_tag(&binding).await?;
        Ok(())
    }

    pub async fn unbind_book_tag(
        &self,
        command: UnbindBookTagCommand,
    ) -> Result<(), LibraryApplicationError> {
        let book_id = BookId::parse(command.book_id)?;
        let tag_id = TagId::parse(command.tag_id)?;
        self.repository.unbind_tag(&book_id, &tag_id).await?;
        Ok(())
    }
}

fn tree_node(tree: &TagTree, definition: &TagDefinition) -> TagTreeNodeOutput {
    TagTreeNodeOutput {
        tag: definition.into(),
        children: tree.children_of(Some(definition.id())).map(|child| tree_node(tree, child)).collect(),
    }
}
