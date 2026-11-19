use jiff::Timestamp;

use entity_derive::Entity;
use crate::domain::library::book::value_objects::BookId;

use super::{
    errors::TagDomainError,
    value_objects::{BookTagStatus, TagColor, TagId, TagName, TagValueRule},
};

#[derive(Debug, Clone, toasty::Model, Entity)]
#[table = "tag_definitions"]
pub struct TagDefinition {
    #[key]
    id: TagId,
    name: TagName,
    #[index]
    parent_id: Option<TagId>,
    color: TagColor,
    value_rule: TagValueRule,
    created_at: Timestamp,
    updated_at: Timestamp,
}

impl TagDefinition {
    pub fn new(
        name: TagName,
        parent_id: Option<TagId>,
        color: TagColor,
        value_rule: TagValueRule,
    ) -> Self {
        let created_at = Timestamp::now();
        Self {
            id: TagId::new(),
            name,
            parent_id,
            color,
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

    pub fn color(&self) -> &TagColor {
        &self.color
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

    pub fn rename(&mut self, name: TagName) -> Result<TagDefinitionChange<'_>, TagDomainError> {
        if self.name == name {
            return Err(TagDomainError::NameUnchanged);
        }
        self.name = name;
        self.updated_at = Timestamp::now();
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

    pub fn change_color(
        &mut self,
        color: TagColor,
    ) -> Result<TagDefinitionChange<'_>, TagDomainError> {
        if self.color == color {
            return Err(TagDomainError::ColorUnchanged);
        }
        self.color = color;
        self.updated_at = Timestamp::now();
        Ok(TagDefinitionChange {
            entity: self,
            fields: TagDefinitionFields::COLOR | TagDefinitionFields::UPDATED_AT,
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
    deleted_at: Option<Timestamp>,
}

impl BookTag {
    pub fn status(&self) -> BookTagStatus {
        match self.deleted_at {
            Some(deleted_at) => BookTagStatus::Deleted { deleted_at },
            None => BookTagStatus::Active,
        }
    }

    pub fn mark_deleted(&mut self) -> Result<BookTagChange<'_>, TagDomainError> {
        if self.status() != BookTagStatus::Active {
            return Err(TagDomainError::AlreadyDeleted);
        }
        let now = Timestamp::now();
        self.deleted_at = Some(now);
        self.updated_at = now;
        Ok(BookTagChange {
            entity: self,
            fields: BookTagFields::DELETED_AT | BookTagFields::UPDATED_AT,
        })
    }

    pub fn restore(&mut self) -> Result<BookTagChange<'_>, TagDomainError> {
        if self.status() == BookTagStatus::Active {
            return Err(TagDomainError::NotDeleted);
        }
        self.deleted_at = None;
        self.updated_at = Timestamp::now();
        Ok(BookTagChange {
            entity: self,
            fields: BookTagFields::DELETED_AT | BookTagFields::UPDATED_AT,
        })
    }

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
            deleted_at: None,
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

    pub fn rebind(
        &mut self,
        definition: &TagDefinition,
        value: Option<String>,
    ) -> Result<BookTagChange<'_>, TagDomainError> {
        definition.value_rule().validate(value.as_deref())?;
        let mut fields = BookTagFields::empty();
        if self.value != value {
            self.value = value;
            fields |= BookTagFields::VALUE;
        }
        if self.status() != BookTagStatus::Active {
            self.deleted_at = None;
            fields |= BookTagFields::DELETED_AT;
        }
        if !fields.is_empty() {
            self.updated_at = Timestamp::now();
            fields |= BookTagFields::UPDATED_AT;
        }
        Ok(BookTagChange {
            entity: self,
            fields,
        })
    }
}

impl crate::domain::EntityId for TagDefinition {
    type Id = TagId;

    fn id(&self) -> &TagId {
        &self.id
    }
}