use std::sync::Arc;

use derive_new::new;
use shiro_domain::identity::user::{
    entity::User,
    value_objects::{UserId, UserName, UserPasswordHash},
};

use crate::identity::{
    errors::IdentityApplicationError,
    ports::{IdentityCrypto, UserLibraryProvisioner, UserRepository, UserSession, UserTransaction},
    user::{
        commands::{
            UserChangePasswordCommand, UserRegisterCommand, UserRenameCommand,
            UserUnregisterCommand,
        },
        dto::GetUserOutput,
        errors::UserApplicationError,
        queries::GetUsersQuery,
    },
};

#[derive(new)]
pub struct UserService<R, P, C> {
    repository: R,
    library_provisioner: Arc<P>,
    crypto: Arc<C>,
}

impl<R, P, C> UserService<R, P, C>
where
    R: UserRepository,
    P: UserLibraryProvisioner,
    C: IdentityCrypto,
{
    pub async fn register(
        &self,
        command: UserRegisterCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::new();
        let name = UserName::parse(command.name)?;
        let password_hash = UserPasswordHash::parse(self.crypto.hash_password(&command.password)?)?;
        let user = User::new(id, name, password_hash);
        let user_id = user.id().to_string();

        match self.library_provisioner.provision_library(&user_id).await {
            Ok(()) => {
                if let Err(error) = self.repository.create(user).await {
                    self.library_provisioner
                        .remove_library(&user_id)
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
                let _ = self.library_provisioner.cleanup_failed_provision(&user_id);
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
        self.library_provisioner
            .remove_library(&id.to_string())
            .await
            .map_err(|error| UserApplicationError::Storage(error).into())
    }

    pub async fn rename(&self, command: UserRenameCommand) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let name = UserName::parse(command.name)?;
        let mut session = self.repository.session();
        let mut tx = session.begin_transaction().await?;
        let mut user = tx.find_by_id(&id).await?;
        let change = user.rename(name)?;
        tx.apply_user_change(change).await?;
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
        let change = user.change_password_hash(password_hash)?;
        tx.apply_user_change(change).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn get_users(
        &self,
        query: GetUsersQuery,
    ) -> Result<Vec<GetUserOutput>, IdentityApplicationError> {
        let ids = query
            .ids
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
