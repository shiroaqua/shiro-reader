use async_trait::async_trait;

use crate::{
    domain::library::{
        book::value_objects::BookId,
        tag::{
            entity::{BookTag, TagDefinition},
            tree::{TagDeletion, TagParentChange},
            value_objects::TagId,
        },
    },
    infrastructure::repositories::{errors::TagRepositoryError, sqlite::VersionedRecord},
};

use super::dto::BookTagOutput;

#[async_trait]
pub trait TagRepository: Send + Sync {
    async fn create(&self, definition: &TagDefinition) -> Result<(), TagRepositoryError>;
    async fn find_by_id(&self, id: &TagId) -> Result<VersionedRecord<TagDefinition>, TagRepositoryError>;
    async fn commit(&self, definition: VersionedRecord<TagDefinition>) -> Result<(), TagRepositoryError>;
    async fn list(&self) -> Result<Vec<TagDefinition>, TagRepositoryError>;
    async fn list_book_tags(&self, book_id: &BookId) -> Result<Vec<BookTagOutput>, TagRepositoryError>;
    async fn bind_tag(&self, binding: &BookTag) -> Result<(), TagRepositoryError>;
    async fn unbind_tag(&self, book_id: &BookId, tag_id: &TagId) -> Result<(), TagRepositoryError>;
    async fn begin_delete(&self) -> Result<Box<dyn TagDeleteTransaction + '_>, TagRepositoryError>;
    async fn begin_move(&self) -> Result<Box<dyn TagMoveTransaction + '_>, TagRepositoryError>;
}

#[async_trait]
pub trait TagDeleteTransaction: Send {
    async fn list(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError>;
    async fn commit(self: Box<Self>, deletion: TagDeletion) -> Result<(), TagRepositoryError>;
}

#[async_trait]
pub trait TagMoveTransaction: Send {
    async fn list(&mut self) -> Result<Vec<TagDefinition>, TagRepositoryError>;
    async fn commit(self: Box<Self>, change: TagParentChange) -> Result<(), TagRepositoryError>;
}
