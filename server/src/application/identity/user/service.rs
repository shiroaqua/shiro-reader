use std::sync::Arc;

use derive_new::new;
use toasty::Db;

use crate::{
    application::MapDbResult,
    application::identity::{
        crypto::service::CryptoService,
        errors::{IdentityApplicationError, user_storage},
        user::{
            commands::{
                UserChangePasswordCommand, UserReanmeCommand, UserRegisterCommand,
                UserUnregisterCommand,
            },
            dto::GetUserOutput,
            errors::UserApplicationError,
            queries::GetUsersQuery,
        },
    },
    domain::{
        EntityCreate, EntityId, RequiresVerificationEntity,
        identity::user::{
            entity::User,
            value_objects::{UserId, UserName, UserPasswordHash},
        },
    },
    state::UserDatabaseProvider,
};

#[derive(new)]
pub struct UserService {
    db: Db,
    user_database_provider: Arc<UserDatabaseProvider>,
    crypto: Arc<CryptoService>,
}

impl UserService {
    pub async fn register(
        &self,
        command: UserRegisterCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::new();
        let name = UserName::parse(command.name)?;
        let password_hash = UserPasswordHash::parse(self.crypto.hash_password(&command.password)?)?;
        let user = User::new(id, name, password_hash);
        let user_id = user.id().to_string();

        match self.user_database_provider.append(&user_id).await {
            Ok(_) => {
                let created = user.create().exec(&mut self.db.clone()).await;
                if let Err(error) = created {
                    self.user_database_provider
                        .remove(&user_id)
                        .await
                        .map_err(|cleanup| {
                            UserApplicationError::Storage(
                                cleanup.context(format!(
                                    "failed to clean up registration after: {error}"
                                )),
                            )
                        })?;
                    return Err(user_storage(error));
                }
                Ok(())
            }
            Err(error) => {
                let _ = self.user_database_provider.clear(&user_id);
                Err(UserApplicationError::Storage(error).into())
            }
        }
    }

    pub async fn unregister(
        &self,
        command: UserUnregisterCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let mut db = self.db.clone();
        let user = User::get_by_id(&mut db, &id).await.map_result()?;
        user.delete().exec(&mut db).await.map_err(user_storage)?;
        self.user_database_provider
            .remove(&id.to_string())
            .await
            .map_err(|error| UserApplicationError::Storage(error).into())
    }

    pub async fn rename(&self, command: UserReanmeCommand) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let name = UserName::parse(command.name)?;
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(user_storage)?;
        let mut user = User::get_by_id(&mut tx, &id).await.map_result()?;
        user.rename(name)?
            .exec(&mut tx)
            .await
            .map_err(user_storage)?;
        tx.commit().await.map_err(user_storage)
    }

    pub async fn change_password(
        &self,
        command: UserChangePasswordCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let password_hash = UserPasswordHash::parse(self.crypto.hash_password(&command.password)?)?;
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(user_storage)?;
        let mut user = User::get_by_id(&mut tx, &id).await.map_result()?;
        user.change_password_hash(password_hash)?
            .exec(&mut tx)
            .await
            .map_err(user_storage)?;
        tx.commit().await.map_err(user_storage)
    }

    pub async fn get_users(
        &self,
        query: GetUsersQuery,
    ) -> Result<Vec<GetUserOutput>, IdentityApplicationError> {
        let user_ids = query
            .user_ids
            .iter()
            .map(UserId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        let mut users = User::all();
        if !user_ids.is_empty() {
            users = users.filter(
                User::fields().id().inner().in_list(
                    user_ids
                        .iter()
                        .map(|id| *id.as_ref())
                        .collect::<Vec<uuid::Uuid>>(),
                ),
            );
        }
        users
            .exec(&mut self.db.clone())
            .await
            .map_err(user_storage)?
            .into_iter()
            .map(|user| {
                let user = user
                    .verify()
                    .map_err(|error| UserApplicationError::Storage(error.into()))?;
                Ok(GetUserOutput {
                    id: user.id().to_string(),
                    name: user.name().to_string(),
                    created_at: user.created_at(),
                    updated_at: user.updated_at(),
                })
            })
            .collect()
    }
}
