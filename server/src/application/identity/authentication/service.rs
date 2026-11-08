use std::sync::Arc;

use derive_new::new;

use crate::{
    application::identity::{
        authentication::{
            ports::SessionRepository, commands::{LogoutCommand, LoginCommand, RefreshAccessTokenCommand, VerifyAccessToeknCommand}, dto::{LoginOutput, RefreshAccessTokenOutput, VerifyAcessToeknOutput}, errors::AuthenticationApplicationError,
        }, crypto::service::CryptoService, errors::IdentityApplicationError, user::ports::UserRepository,
    }, domain::identity::user::value_objects::{UserId, UserName}, infrastructure::repositories::errors::UserRepositoryError,
};


#[derive(new)]
pub struct AuthenticationService {
    repository: Arc<dyn UserRepository>,
    crypto: Arc<CryptoService>,
    sessions: Arc<dyn SessionRepository>,
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
        self.sessions.create(&user.id().to_string(), &token_hash(&refresh_token), &token_hash(&access_token))
            .await.map_err(AuthenticationApplicationError::Storage)?;
        Ok(LoginOutput {
            access_token: access_token,
            refresh_token: refresh_token,
        })
    }

    pub async fn logout(&self, command: LogoutCommand) -> Result<(), IdentityApplicationError> {
        // Verify signature, issuer, audience, expiry and token type before touching storage.
        // No access token is required: logout remains possible after it expires.
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        self.sessions.revoke(&user_id, &token_hash(&command.token))
            .await.map_err(AuthenticationApplicationError::Storage)?;
        Ok(())
    }

    pub async fn refresh_access_token(
        &self,
        command: RefreshAccessTokenCommand,
    ) -> Result<RefreshAccessTokenOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        let _ =  self.repository.find_by_id(&UserId::parse(&user_id)?).await.map_err(|_| IdentityApplicationError::Authentication(AuthenticationApplicationError::InvalidToken))?;

        let access_token = self.crypto.issue_access_token(user_id.clone())?;
        if !self.sessions.add_access(&user_id, &token_hash(&command.token), &token_hash(&access_token))
            .await.map_err(AuthenticationApplicationError::Storage)? {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        Ok(RefreshAccessTokenOutput { access_token })
    }

    pub async fn verify_access_token(&self, command: VerifyAccessToeknCommand) -> Result<VerifyAcessToeknOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_access_token(&command.token)?;
        if !self.sessions.contains_access(&user_id, &token_hash(&command.token))
            .await.map_err(AuthenticationApplicationError::Storage)? {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        Ok(VerifyAcessToeknOutput { user_id })
    }
}

fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}
