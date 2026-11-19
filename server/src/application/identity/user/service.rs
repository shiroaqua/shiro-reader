use std::sync::Arc;

use derive_new::new;

use crate::{
    application::identity::{
        crypto::service::CryptoService,
        errors::IdentityApplicationError,
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
        EntityId,
        identity::user::{
            entity::User,
            value_objects::{UserId, UserName, UserPasswordHash},
        },
    },
    infrastructure::repositories::user_repository::UserRepository,
    state::UserDatabaseProvider,
};

#[derive(new)]
pub struct UserService {
    repository: UserRepository,
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
                if let Err(error) = self.repository.create(user).await {
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
                    return Err(error.into());
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
        let user = self.repository.find_by_id(&id).await?;
        self.repository.delete(user).await?;
        self.user_database_provider
            .remove(&id.to_string())
            .await
            .map_err(|error| UserApplicationError::Storage(error).into())
    }

    pub async fn rename(&self, command: UserReanmeCommand) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let name = UserName::parse(command.name)?;
        let mut session = self.repository.session();
        let mut tx = session.begin_transaction().await?;
        let mut user = tx.find_by_id(&id).await?;
        user.rename(name)?.exec(&mut tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn change_password(
        &self,
        command: UserChangePasswordCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let password_hash = UserPasswordHash::parse(self.crypto.hash_password(&command.password)?)?;
        let mut session = self.repository.session();
        let mut tx = session.begin_transaction().await?;
        let mut user = tx.find_by_id(&id).await?;
        user.change_password_hash(password_hash)?
            .exec(&mut tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_users(
        &self,
        query: GetUsersQuery,
    ) -> Result<Vec<GetUserOutput>, IdentityApplicationError> {
        let ids = query
            .user_ids
            .iter()
            .map(UserId::parse)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self
            .repository
            .list(&ids)
            .await?
            .into_iter()
            .map(Into::into)
            .collect())
    }
}
