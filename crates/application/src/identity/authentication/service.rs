use std::sync::Arc;

use shiro_domain::identity::{
    session::{AuthAccessToken, AuthSession},
    user::value_objects::{UserId, UserName},
};

use crate::identity::{
    authentication::{
        commands::{
            LoginCommand, LogoutCommand, RefreshAccessTokenCommand, VerifyAccessTokenCommand,
        },
        dto::{LoginOutput, RefreshAccessTokenOutput, VerifyAccessTokenOutput},
        errors::AuthenticationApplicationError,
    },
    errors::IdentityApplicationError,
    ports::{IdentityCrypto, SessionRepository, UserRepository},
};

pub struct AuthenticationService<U, S, C> {
    user_repository: U,
    session_repository: S,
    crypto: Arc<C>,
}

impl<U, S, C> AuthenticationService<U, S, C> {
    pub fn new(user_repository: U, session_repository: S, crypto: Arc<C>) -> Self {
        Self {
            user_repository,
            session_repository,
            crypto,
        }
    }
}

impl<U, S, C> AuthenticationService<U, S, C>
where
    U: UserRepository,
    S: SessionRepository,
    C: IdentityCrypto,
{
    pub async fn login(
        &self,
        command: LoginCommand,
    ) -> Result<LoginOutput, IdentityApplicationError> {
        let name = UserName::parse(command.username)?;
        let user = self
            .user_repository
            .find_by_name(name)
            .await?
            .ok_or(AuthenticationApplicationError::InvalidCredentials)?
            .into_inner();

        if !self
            .crypto
            .verify_password(&command.password, user.password_hash.as_str())?
        {
            return Err(AuthenticationApplicationError::InvalidCredentials.into());
        }

        let user_id = user.id.to_string();
        let username: String = user.name.into();
        let access_token = self.crypto.issue_access_token(user_id.clone())?;
        let refresh_token = self.crypto.issue_refresh_token(user_id.clone())?;
        let access_hash = token_hash(&access_token);
        let refresh_hash = token_hash(&refresh_token);

        let session = AuthSession::new(refresh_hash.clone(), user.id);
        let access = AuthAccessToken::new(access_hash, refresh_hash);
        self.session_repository
            .create_session(session, access)
            .await?;

        Ok(LoginOutput {
            user_id,
            username,
            access_token,
            refresh_token,
        })
    }

    pub async fn logout(&self, command: LogoutCommand) -> Result<(), IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        let user_id = UserId::parse(&user_id)
            .map_err(|error| AuthenticationApplicationError::Storage(error.into()))?;
        self.session_repository
            .revoke(&user_id, token_hash(&command.token))
            .await?;
        Ok(())
    }

    pub async fn refresh_access_token(
        &self,
        command: RefreshAccessTokenCommand,
    ) -> Result<RefreshAccessTokenOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        let parsed_user_id =
            UserId::parse(&user_id).map_err(|_| AuthenticationApplicationError::InvalidToken)?;
        let refresh_hash = token_hash(&command.token);
        let access_token = self.crypto.issue_access_token(user_id)?;
        let access = AuthAccessToken::new(token_hash(&access_token), refresh_hash);
        if !self
            .session_repository
            .add_access_token(&parsed_user_id, access)
            .await?
        {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        Ok(RefreshAccessTokenOutput { access_token })
    }

    pub async fn verify_access_token(
        &self,
        command: VerifyAccessTokenCommand,
    ) -> Result<VerifyAccessTokenOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_access_token(&command.token)?;
        if !self
            .session_repository
            .contains_access_token(token_hash(&command.token))
            .await?
        {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        Ok(VerifyAccessTokenOutput { user_id })
    }
}

fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}
