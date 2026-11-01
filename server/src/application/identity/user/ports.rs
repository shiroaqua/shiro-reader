use async_trait::async_trait;

use crate::{
    application::identity::user::dto::GetUserOutput,
    domain::identity::user::{entity::User, value_objects::{UserId, UserName}}, infrastructure::repositories::{errors::UserRepositoryError, sqlite::VersionedRecord},
};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn create(&self, user: &User) -> Result<(), UserRepositoryError>;
    async fn delete(&self, id: &UserId) -> Result<(), UserRepositoryError>;
    async fn password_hash(&self, id: &UserId) -> Result<String, UserRepositoryError>;

    async fn list_ids(&self) -> Result<Vec<String>, UserRepositoryError>;
    async fn list(&self, user_ids: Vec<UserId>) -> Result<Vec<GetUserOutput>, UserRepositoryError>;


    async fn find_by_name(&self, name: &UserName) -> Result<User, UserRepositoryError>;
    async fn find_by_id(&self, id: &UserId) -> Result<VersionedRecord<User>, UserRepositoryError>;
    async fn commit(&self, user: VersionedRecord<User>) -> Result<(), UserRepositoryError>;
}
