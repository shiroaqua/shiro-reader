use std::collections::HashMap;

use super::{
    entity::TagDefinition,
    errors::TagDomainError,
    value_objects::{TagDeleteStrategy, TagId},
};

#[derive(Debug)]
pub struct TagTree {
    tags: Vec<TagDefinition>,
    by_id: HashMap<TagId, usize>,
    children: HashMap<Option<TagId>, Vec<usize>>,
}

impl TagTree {
    pub fn new(tags: Vec<TagDefinition>) -> Result<Self, TagDomainError> {
        let mut by_id = HashMap::new();
        let mut children: HashMap<Option<TagId>, Vec<usize>> = HashMap::new();
        for (index, tag) in tags.iter().enumerate() {
            if by_id.insert(tag.id().clone(), index).is_some() {
                return Err(TagDomainError::InvalidTree);
            }
            children.entry(tag.parent_id().cloned()).or_default().push(index);
        }
        if tags.iter().filter_map(TagDefinition::parent_id).any(|id| !by_id.contains_key(id)) {
            return Err(TagDomainError::InvalidTree);
        }

        // 每个标签只有一个父标签，因此任何环只要存在自指就无法同时再指向另一个健康树状结构中的父标签。换句话说就是它是孤立的！不能被挂在树上的！
        let mut pending = children.get(&None).cloned().unwrap_or_default();
        let mut visited = 0;
        while let Some(index) = pending.pop() {
            visited += 1; // 这个是关键！从一个集合展开的一棵树中如果存在孤立环则必然导致数量对不上！
            if let Some(indices) = children.get(&Some(tags[index].id().clone())) {
                pending.extend(indices);
            }
        }
        if visited != tags.len() {
            return Err(TagDomainError::InvalidTree);
        }
        Ok(Self { tags, by_id, children })
    }

    pub fn children_of(&self, parent_id: Option<&TagId>) -> impl Iterator<Item = &TagDefinition> {
        self.children.get(&parent_id.cloned()).into_iter().flatten()
            .map(|index| &self.tags[*index])
    }

    pub fn plan_delete(
        &self,
        id: &TagId,
        strategy: TagDeleteStrategy,
    ) -> Result<TagDeletion, TagDomainError> {
        let index = *self.by_id.get(id).ok_or(TagDomainError::NotFound)?;
        let tag = &self.tags[index];
        match strategy {
            TagDeleteStrategy::Cascade => {
                let mut pending = vec![index];
                let mut deleted_ids = Vec::new();
                while let Some(index) = pending.pop() {
                    let id = self.tags[index].id();
                    deleted_ids.push(id.clone());
                    if let Some(indices) = self.children.get(&Some(id.clone())) {
                        pending.extend(indices);
                    }
                }
                // 持久化时先删子孙，最后删父标签。
                deleted_ids.reverse();
                Ok(TagDeletion { deleted_ids, promoted_children: Vec::new() })
            }
            TagDeleteStrategy::Promote => {
                let promoted_children = self.children_of(Some(id))
                    .map(|child| TagParentChange {
                        id: child.id().clone(),
                        parent_id: tag.parent_id().cloned(),
                    })
                    .collect();
                Ok(TagDeletion { deleted_ids: vec![id.clone()], promoted_children })
            }
        }
    }
}

#[derive(Debug)]
pub struct TagDeletion {
    deleted_ids: Vec<TagId>,
    promoted_children: Vec<TagParentChange>,
}

impl TagDeletion {
    pub fn deleted_ids(&self) -> &[TagId] {
        &self.deleted_ids
    }

    pub fn promoted_children(&self) -> &[TagParentChange] {
        &self.promoted_children
    }
}

#[derive(Debug)]
pub struct TagParentChange {
    id: TagId,
    parent_id: Option<TagId>,
}

impl TagParentChange {
    pub fn id(&self) -> &TagId {
        &self.id
    }

    pub fn parent_id(&self) -> Option<&TagId> {
        self.parent_id.as_ref()
    }
}
