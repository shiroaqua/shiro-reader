use serde::{Deserialize, Serialize};
use shiro_domain::library::tag::entity::{BookTag, TagDefinition};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookTagOutput {
    pub tag_id: String,
    pub value: Option<String>,
}

impl From<BookTag> for BookTagOutput {
    fn from(binding: BookTag) -> Self {
        let binding = binding.into_inner();
        Self {
            tag_id: binding.tag_id.to_string(),
            value: binding.value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagDefinitionOutput {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub color: String,
    pub allows_value: bool,
    pub value_regex: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&TagDefinition> for TagDefinitionOutput {
    fn from(definition: &TagDefinition) -> Self {
        Self {
            id: definition.id().to_string(),
            name: definition.name().to_string(),
            parent_id: definition.parent_id().map(ToString::to_string),
            color: definition.color().to_string(),
            allows_value: definition.value_rule().allows_value(),
            value_regex: definition.value_rule().value_regex().map(str::to_owned),
            created_at: definition.created_at().to_string(),
            updated_at: definition.updated_at().to_string(),
        }
    }
}

impl From<TagDefinition> for TagDefinitionOutput {
    fn from(definition: TagDefinition) -> Self {
        let definition = definition.into_inner();
        Self {
            id: definition.id.to_string(),
            name: definition.name.into(),
            parent_id: definition.parent_id.map(|id| id.to_string()),
            color: definition.color.into(),
            allows_value: definition.value_rule.allows_value(),
            value_regex: definition.value_rule.value_regex().map(str::to_owned),
            created_at: definition.created_at.to_string(),
            updated_at: definition.updated_at.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TagTreeNodeOutput {
    #[serde(flatten)]
    pub tag: TagDefinitionOutput,
    pub children: Vec<TagTreeNodeOutput>,
}
