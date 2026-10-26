use std::sync::Arc;

use derive_new::new;

use crate::{
    application::identity::{
        authentication::{
            commands::{LoginCommand, RefreshAccessTokenCommand, VerifyAccessToeknCommand}, dto::{LoginOutput, RefreshAccessTokenOutput, VerifyAcessToeknOutput}, errors::AuthenticationApplicationError,
        }, crypto::service::CryptoService, errors::IdentityApplicationError, user::ports::UserRepository,
    }, domain::identity::user::value_objects::{UserId, UserName}, infrastructure::repositories::errors::UserRepositoryError,
};


#[derive(new)]
pub struct AuthenticationService {
    repository: Arc<dyn UserRepository>,
    crypto: Arc<CryptoService>,
}

impl AuthenticationService {
    pub async fn login(
        &self,
        command: LoginCommand,
    ) -> Result<LoginOutput, IdentityApplicationError> {
        let name = UserName::parse(command.username)?;
        let user = match self.repository.find_by_name(&name).await {
            Ok(user) => user,
            Err(UserRepositoryError::UserNotFound) => return Err(AuthenticationApplicationError::InvalidCredentials.into()),
            Err(e) => return Err(e.into()),
        };

        if !self.crypto.verify_password(&command.password, user.password_hash().as_str())? {
            return Err(AuthenticationApplicationError::InvalidCredentials.into());
        }

        let access_token = self.crypto.issue_access_token(user.id().to_string())?;
        let refresh_token = self.crypto.issue_refresh_token(user.id().to_string())?;
        Ok(LoginOutput {
            access_token: access_token,
            refresh_token: refresh_token,
        })
    }

    pub async fn refresh_access_token(
        &self,
        command: RefreshAccessTokenCommand,
    ) -> Result<RefreshAccessTokenOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        let _ =  self.repository.find_by_id(&UserId::parse(&user_id)?).await?;

        let access_token = self.crypto.issue_access_token(user_id)?;
        Ok(RefreshAccessTokenOutput { access_token })
    }

    pub async fn verify_access_token(&self, command: VerifyAccessToeknCommand) -> Result<VerifyAcessToeknOutput, IdentityApplicationError> {
        Ok(VerifyAcessToeknOutput {user_id: self.crypto.verify_access_token(&command.token)?})
    }
}
