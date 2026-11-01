use derive_new::new;
use std::sync::Arc;

use crate::{
    application::identity::{
        crypto::service::CryptoService, errors::IdentityApplicationError, user::{
            commands::{
                UserChangePasswordCommand,
                UserReanmeCommand, UserRegisterCommand, UserUnregisterCommand,
            }, dto::GetUserOutput, errors::UserApplicationError, ports::UserRepository,
            queries::GetUsersQuery,
        },
    }, domain::identity::user::{
        entity::User,
        value_objects::{UserId, UserName, UserPasswordHash},
    }, shared::time::now_ns, state::UserSqlPoolProvider,
};

#[derive(new)]
pub struct UserService {
    repository: Arc<dyn UserRepository>,
    user_sqlpool_provider: Arc<UserSqlPoolProvider>,
    crypto: Arc<CryptoService>,
}

impl UserService {

    pub async fn register(
        &self,
        command: UserRegisterCommand,
    ) -> Result<(), IdentityApplicationError> {
        let now = now_ns();

        let id = UserId::new();
        let name = UserName::parse(command.name)?;

        let password = command.password;
        let password_hash = self.crypto.hash_password(&password)?;
        let password_hash = UserPasswordHash::parse(password_hash)?;

        let user = User::new(id, name, password_hash, now);
        let user_id = user.id().to_string();

        // 此处错误有两种可能，第一种为数据库原因导致无法创建用户而发生错误；第二种为系统文件系统有问题。
        match self.user_sqlpool_provider.append(&user_id).await {
            Ok(_) => {
                if let Err(error) = self.repository.create(&user).await {
                    self.user_sqlpool_provider.remove(&user_id).await.map_err(|cleanup| {
                        UserApplicationError::Storage(cleanup.context(format!(
                            "failed to clean up registration after: {error}"
                        )))
                    })?;
                    return Err(error.into());
                }
                Ok(())
            },
            Err(e) => {
                let _ = self.user_sqlpool_provider.clear(&user_id);
                Err(IdentityApplicationError::User(UserApplicationError::Storage(e)))
            },
        }
    }

    pub async fn unregister(
        &self,
        command: UserUnregisterCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        self.repository.delete(&id).await?; // 这边与添加时相反，文件可以先不管，数据库里只要没有了我什么时候都可以清理掉多余的那些问题，但如果出现文件删一半出错了，结果数据库那边还存有该用户的记录那问题就大了，用户会看见一副奇怪场景的！
        match self.user_sqlpool_provider.remove(&id.to_string()).await {
            Ok(o) => Ok(o),
            Err(e) => return Err(IdentityApplicationError::User(UserApplicationError::Storage(e))),
        }
    }

    pub async fn rename(&self, command: UserReanmeCommand) -> Result<(), IdentityApplicationError> {
        let id = UserId::parse(command.id)?;
        let name = UserName::parse(command.name)?;
        let mut user = self.repository.find_by_id(&id).await?;
        user.rename(name)?;

        self.repository.commit(user).await?;
        Ok(())
    }

    pub async fn change_password(
        &self,
        command: UserChangePasswordCommand,
    ) -> Result<(), IdentityApplicationError> {
        let id: UserId = UserId::parse(command.id)?;

        let password = command.password;
        let password_hash = self.crypto.hash_password(&password)?;
        let password_hash = UserPasswordHash::parse(password_hash)?;

        let mut user: crate::infrastructure::repositories::sqlite::VersionedRecord<User> =
            self.repository.find_by_id(&id).await?;
        user.change_password_hash(password_hash)?;

        self.repository.commit(user).await?;
        Ok(())
    }

    pub async fn get_users(
        &self,
        query: GetUsersQuery,
    ) -> Result<Vec<GetUserOutput>, IdentityApplicationError> {
        let users = self
            .repository
            .list(
                query
                    .user_ids
                    .iter()
                    .map(|id| UserId::parse(id))
                    .collect::<Result<_, _>>()?,
            )
            .await?;

        Ok(users)
    }
}
