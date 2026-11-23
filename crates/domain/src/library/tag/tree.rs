use std::collections::HashMap;

use super::{
    entity::TagDefinition,
    errors::TagDomainError,
    value_objects::{TagDeleteStrategy, TagId, TagName},
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
            children
                .entry(tag.parent_id().cloned())
                .or_default()
                .push(index);
        }
        if tags
            .iter()
            .filter_map(TagDefinition::parent_id)
            .any(|id| !by_id.contains_key(id))
        {
            return Err(TagDomainError::InvalidTree);
        }

        // 每个标签只有一个父标签，因此任何环只要存在自指就无法同时再指向另一个健康树状结构中的父标签。
        let mut pending = children.get(&None).cloned().unwrap_or_default();
        let mut visited = 0;
        while let Some(index) = pending.pop() {
            visited += 1;
            if let Some(indices) = children.get(&Some(tags[index].id().clone())) {
                pending.extend(indices);
            }
        }
        if visited != tags.len() {
            return Err(TagDomainError::InvalidTree);
        }
        Ok(Self {
            tags,
            by_id,
            children,
        })
    }

    pub fn children_of(&self, parent_id: Option<&TagId>) -> impl Iterator<Item = &TagDefinition> {
        self.children
            .get(&parent_id.cloned())
            .into_iter()
            .flatten()
            .map(|index| &self.tags[*index])
    }

    pub fn definition(&self, id: &TagId) -> Option<&TagDefinition> {
        self.by_id.get(id).map(|index| &self.tags[*index])
    }

    pub fn definition_mut(&mut self, id: &TagId) -> Option<&mut TagDefinition> {
        self.by_id.get(id).map(|index| &mut self.tags[*index])
    }

    pub fn take_definitions(self, ids: &[TagId]) -> Result<Vec<TagDefinition>, TagDomainError> {
        let mut tags: Vec<Option<TagDefinition>> = self.tags.into_iter().map(Some).collect();
        ids.iter()
            .map(|id| {
                let index = *self.by_id.get(id).ok_or(TagDomainError::NotFound)?;
                tags[index].take().ok_or(TagDomainError::NotFound)
            })
            .collect()
    }

    pub fn into_nodes<T>(self, mut build: impl FnMut(TagDefinition, Vec<T>) -> T) -> Vec<T> {
        fn build_children<T>(
            parent_id: Option<TagId>,
            tags: &mut [Option<TagDefinition>],
            children: &HashMap<Option<TagId>, Vec<usize>>,
            build: &mut impl FnMut(TagDefinition, Vec<T>) -> T,
        ) -> Vec<T> {
            children
                .get(&parent_id)
                .into_iter()
                .flatten()
                .map(|&index| {
                    let tag = tags[index]
                        .take()
                        .expect("valid tag tree visits each node once");
                    let child_nodes = build_children(Some(tag.id().clone()), tags, children, build);
                    build(tag, child_nodes)
                })
                .collect()
        }

        let mut tags: Vec<Option<TagDefinition>> = self.tags.into_iter().map(Some).collect();
        build_children(None, &mut tags, &self.children, &mut build)
    }

    pub fn has_sibling_name(
        &self,
        parent_id: Option<&TagId>,
        name: &TagName,
        except_id: Option<&TagId>,
    ) -> bool {
        self.children_of(parent_id)
            .any(|tag| Some(tag.id()) != except_id && tag.name() == name)
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
                Ok(TagDeletion {
                    deleted_ids,
                    promoted_children: Vec::new(),
                })
            }
            TagDeleteStrategy::Promote => {
                let promoted_children = self
                    .children_of(Some(id))
                    .map(|child| TagParentChange {
                        id: child.id().clone(),
                        parent_id: tag.parent_id().cloned(),
                    })
                    .collect();
                Ok(TagDeletion {
                    deleted_ids: vec![id.clone()],
                    promoted_children,
                })
            }
        }
    }

    pub fn plan_move(
        &self,
        id: &TagId,
        parent_id: Option<&TagId>,
    ) -> Result<TagParentChange, TagDomainError> {
        let source_index = *self.by_id.get(id).ok_or(TagDomainError::NotFound)?;
        let parent_index = parent_id
            .map(|parent_id| {
                self.by_id
                    .get(parent_id)
                    .copied()
                    .ok_or(TagDomainError::ParentNotFound)
            })
            .transpose()?;

        let tag = &self.tags[source_index];
        if tag.parent_id() == parent_id {
            return Err(TagDomainError::InvalidMove);
        }

        let mut ancestor = parent_index;
        while let Some(index) = ancestor {
            if index == source_index {
                return Err(TagDomainError::InvalidMove);
            }
            ancestor = self.tags[index]
                .parent_id()
                .and_then(|id| self.by_id.get(id).copied());
        }

        Ok(TagParentChange {
            id: id.clone(),
            parent_id: parent_id.cloned(),
        })
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

    pub fn into_parts(self) -> (Vec<TagId>, Vec<TagParentChange>) {
        (self.deleted_ids, self.promoted_children)
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

    pub fn into_parts(self) -> (TagId, Option<TagId>) {
        (self.id, self.parent_id)
    }
}
