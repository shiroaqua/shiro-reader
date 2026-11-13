use crate::domain::library::tag::entity::TagDefinition;

#[derive(Debug)]
pub struct BookTagOutput {
    pub tag_id: String,
    pub value: Option<String>,
}

#[derive(Debug)]
pub struct TagDefinitionOutput {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub allows_value: bool,
    pub value_regex: Option<String>,
    pub created_at: i128,
    pub updated_at: i128,
}

impl From<&TagDefinition> for TagDefinitionOutput {
    fn from(definition: &TagDefinition) -> Self {
        Self {
            id: definition.id().to_string(),
            name: definition.name().to_string(),
            parent_id: definition.parent_id().map(ToString::to_string),
            allows_value: definition.value_rule().allows_value(),
            value_regex: definition.value_rule().value_regex().map(str::to_owned),
            created_at: definition.created_at(),
            updated_at: definition.updated_at(),
        }
    }
}

#[derive(Debug)]
pub struct TagTreeNodeOutput {
    pub tag: TagDefinitionOutput,
    pub children: Vec<TagTreeNodeOutput>,
}
