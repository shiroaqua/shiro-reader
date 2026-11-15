use jiff::Timestamp;

use entity_derive::Entity;

use crate::domain::library::book::value_objects::BookId;
use crate::domain::RequiresVerificationEntity;

use super::{
    errors::TagDomainError,
    value_objects::{TagId, TagName, TagValueRule},
};

#[derive(Debug, Clone, toasty::Model, Entity)]
#[table = "tag_definitions"]
pub struct TagDefinition {
    #[key]
    id: TagId,
    #[unique]
    name: TagName,
    #[index]
    parent_id: Option<TagId>,
    value_rule: TagValueRule,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl TagDefinition {
    pub fn new(
        name: TagName,
        parent_id: Option<TagId>,
        value_rule: TagValueRule,
    ) -> Self {
        let created_at = Timestamp::now();
        Self {
            id: TagId::new(),
            name,
            parent_id,
            value_rule,
            created_at,
            updated_at: created_at,
        }
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

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }
    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn rename(
        &mut self,
        name: TagName,
    ) -> Result<TagDefinitionChange<'_>, TagDomainError> {
        if self.name == name {
            return Err(TagDomainError::NameUnchanged);
        }
        self.name = name;
        self.updated_at =  Timestamp::now();
        Ok(TagDefinitionChange {
            entity: self,
            fields: TagDefinitionFields::NAME | TagDefinitionFields::UPDATED_AT,
        })
    }

    pub fn change_parent(
        &mut self,
        parent_id: Option<TagId>,
    ) -> Result<TagDefinitionChange<'_>, TagDomainError> {
        if self.parent_id == parent_id || parent_id.as_ref() == Some(&self.id) {
            return Err(TagDomainError::InvalidMove);
        }
        self.parent_id = parent_id;
        self.updated_at = Timestamp::now();
        Ok(TagDefinitionChange {
            entity: self,
            fields: TagDefinitionFields::PARENT_ID | TagDefinitionFields::UPDATED_AT,
        })
    }
}

#[derive(Debug, Clone, toasty::Model, Entity)]
#[table = "book_tags"]
pub struct BookTag {
    #[key]
    tag_id: TagId,
    #[key]
    book_id: BookId,
    value: Option<String>,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl BookTag {
    pub fn new(
        definition: &TagDefinition,
        book_id: BookId,
        value: Option<String>,
    ) -> Result<Self, TagDomainError> {
        definition.value_rule().validate(value.as_deref())?;
        let now = Timestamp::now();
        Ok(Self {
            tag_id: crate::domain::EntityId::id(definition).clone(),
            book_id,
            value,
            created_at: now,
            updated_at: now,
        })
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

    pub fn created_at(&self) -> Timestamp {
        self.created_at
    }

    pub fn updated_at(&self) -> Timestamp {
        self.updated_at
    }

    pub fn change_value(
        &mut self,
        definition: &TagDefinition,
        value: Option<String>,
    ) -> Result<BookTagChange<'_>, TagDomainError> {
        definition.value_rule().validate(value.as_deref())?;
        self.value = value;
        self.updated_at = Timestamp::now();
        Ok(BookTagChange {
            entity: self,
            fields: BookTagFields::VALUE | BookTagFields::UPDATED_AT,
        })
    }
}

impl crate::domain::EntityId for TagDefinition {
    type Id = TagId;

    fn id(&self) -> &TagId {
        &self.id
    }
}

impl RequiresVerificationEntity<TagDomainError> for TagDefinition {
    fn verify(self) -> Result<Self, TagDomainError> {
        Ok(self)
    }
}

impl RequiresVerificationEntity<TagDomainError> for BookTag {
    fn verify(self) -> Result<Self, TagDomainError> {
        Ok(self)
    }
}