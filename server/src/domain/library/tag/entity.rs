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
    created_at: i128,
    updated_at: i128,
}

impl TagDefinition {
    pub fn new(
        name: TagName,
        parent_id: Option<TagId>,
        value_rule: TagValueRule,
        created_at: i128,
    ) -> Result<Self, TagDomainError> {
        validate_timestamps(created_at, created_at)?;
        Ok(Self {
            id: TagId::new(),
            name,
            parent_id,
            value_rule,
            created_at,
            updated_at: created_at,
        })
    }

    pub(crate) fn reconstitute(
        id: TagId,
        name: TagName,
        parent_id: Option<TagId>,
        value_rule: TagValueRule,
        created_at: i128,
        updated_at: i128,
    ) -> Result<Self, TagDomainError> {
        if parent_id.as_ref() == Some(&id) {
            return Err(TagDomainError::InvalidTree);
        }
        validate_timestamps(created_at, updated_at)?;
        Ok(Self { id, name, parent_id, value_rule, created_at, updated_at })
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

    pub fn created_at(&self) -> i128 {
        self.created_at
    }

    pub fn updated_at(&self) -> i128 {
        self.updated_at
    }

    pub(crate) fn next_updated_at(&self, candidate: i128) -> Result<i128, TagDomainError> {
        let updated_at = candidate.max(self.updated_at.saturating_add(1));
        validate_timestamps(self.created_at, updated_at)?;
        Ok(updated_at)
    }

    pub fn rename(&mut self, name: TagName, updated_at: i128) -> Result<(), TagDomainError> {
        if self.name == name {
            return Err(TagDomainError::NameUnchanged);
        }
        let updated_at = self.next_updated_at(updated_at)?;
        self.name = name;
        self.updated_at = updated_at;
        Ok(())
    }
}

fn validate_timestamps(created_at: i128, updated_at: i128) -> Result<(), TagDomainError> {
    if !is_valid_timestamp(created_at)
        || !is_valid_timestamp(updated_at)
        || updated_at < created_at
    {
        return Err(TagDomainError::InvalidTimestamp);
    }
    Ok(())
}

fn is_valid_timestamp(timestamp: i128) -> bool {
    let seconds = timestamp.div_euclid(1_000_000_000);
    let Ok(seconds) = i64::try_from(seconds) else {
        return false;
    };
    let nanos = timestamp.rem_euclid(1_000_000_000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(seconds, nanos).is_some()
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