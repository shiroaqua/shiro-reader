use crate::domain::library::book::value_objects::BookId;

use super::{
    errors::TagDomainError,
    value_objects::{TagId, TagName, TagValueRule},
};

#[derive(Debug, Clone)]
pub struct TagDefinition {
    id: TagId,
    name: TagName,
    parent_id: Option<TagId>,
    value_rule: TagValueRule,
}

impl TagDefinition {
    pub fn new(name: TagName, parent_id: Option<TagId>, value_rule: TagValueRule) -> Self {
        Self { id: TagId::new(), name, parent_id, value_rule }
    }

    pub(crate) fn reconstitute(
        id: TagId,
        name: TagName,
        parent_id: Option<TagId>,
        value_rule: TagValueRule,
    ) -> Result<Self, TagDomainError> {
        if parent_id.as_ref() == Some(&id) {
            return Err(TagDomainError::InvalidTree);
        }
        Ok(Self { id, name, parent_id, value_rule })
    }

    pub fn id(&self) -> &TagId {
        &self.id
    }

    pub fn name(&self) -> &TagName {
        &self.name
    }

    pub fn parent_id(&self) -> Option<&TagId> {
        self.parent_id.as_ref()
    }

    pub fn value_rule(&self) -> &TagValueRule {
        &self.value_rule
    }

    pub fn rename(&mut self, name: TagName) -> Result<(), TagDomainError> {
        if self.name == name {
            return Err(TagDomainError::NameUnchanged);
        }
        self.name = name;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct BookTag {
    tag_id: TagId,
    book_id: BookId,
    value: Option<String>,
}

impl BookTag {
    pub fn new(
        definition: &TagDefinition,
        book_id: BookId,
        value: Option<String>,
    ) -> Result<Self, TagDomainError> {
        definition.value_rule().validate(value.as_deref())?;
        Ok(Self { tag_id: definition.id().clone(), book_id, value })
    }

    pub fn tag_id(&self) -> &TagId {
        &self.tag_id
    }

    pub fn book_id(&self) -> &BookId {
        &self.book_id
    }

    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}
