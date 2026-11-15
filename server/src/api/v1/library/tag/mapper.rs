use crate::{
    application::library::tag::dto::{BookTagOutput, TagDefinitionOutput, TagTreeNodeOutput},
    domain::library::tag::value_objects::TagDeleteStrategy,
};

use super::dto::{BookTagResponse, DeleteTagStrategy, TagResponse, TagTreeNodeResponse};

impl From<BookTagOutput> for BookTagResponse {
    fn from(output: BookTagOutput) -> Self {
        Self { tag_id: output.tag_id, value: output.value }
    }
}

impl From<TagDefinitionOutput> for TagResponse {
    fn from(output: TagDefinitionOutput) -> Self {
        Self {
            id: output.id,
            name: output.name,
            parent_id: output.parent_id,
            allows_value: output.allows_value,
            value_regex: output.value_regex,
            created_at: output.created_at,
            updated_at: output.updated_at,
        }
    }
}

impl From<TagTreeNodeOutput> for TagTreeNodeResponse {
    fn from(output: TagTreeNodeOutput) -> Self {
        Self {
            tag: output.tag.into(),
            children: output.children.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<DeleteTagStrategy> for TagDeleteStrategy {
    fn from(strategy: DeleteTagStrategy) -> Self {
        match strategy {
            DeleteTagStrategy::Cascade => Self::Cascade,
            DeleteTagStrategy::Promote => Self::Promote,
        }
    }
}
