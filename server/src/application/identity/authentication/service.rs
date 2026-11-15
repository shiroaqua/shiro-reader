use std::sync::Arc;

use derive_new::new;
use toasty::Db;

use crate::{
    application::MapDbResult,
    application::identity::{
        authentication::{
            commands::{LoginCommand, LogoutCommand, RefreshAccessTokenCommand, VerifyAccessToeknCommand},
            dto::{LoginOutput, RefreshAccessTokenOutput, VerifyAcessToeknOutput},
            errors::AuthenticationApplicationError,
        },
        crypto::service::CryptoService,
        errors::{IdentityApplicationError, user_storage},
        user::errors::UserApplicationError,
    },
    domain::{EntityCreate, EntityId, RequiresVerificationEntity, identity::{session::{AuthAccessToken, AuthSession}, user::{entity::User, value_objects::{UserId, UserName}}}},
};

#[derive(new)]
pub struct AuthenticationService {
    db: Db,
    crypto: Arc<CryptoService>,
}

impl AuthenticationService {
    pub async fn login(&self, command: LoginCommand) -> Result<LoginOutput, IdentityApplicationError> {
        let name = UserName::parse(command.username)?;
        let user = User::filter(User::fields().name().eq(name))
            .first().exec(&mut self.db.clone()).await.map_err(user_storage)?
            .ok_or(AuthenticationApplicationError::InvalidCredentials)?;
        let user = user.verify()
            .map_err(|error| UserApplicationError::Storage(error.into()))?;

        if !self.crypto.verify_password(&command.password, user.password_hash().as_str())? {
            return Err(AuthenticationApplicationError::InvalidCredentials.into());
        }

        let access_token = self.crypto.issue_access_token(user.id().to_string())?;
        let refresh_token = self.crypto.issue_refresh_token(user.id().to_string())?;
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(auth_storage)?;
        let session = AuthSession::new(token_hash(&refresh_token), user.id().clone());
        session.create().exec(&mut tx).await.map_err(auth_storage)?;
        let access = AuthAccessToken::new(token_hash(&access_token), token_hash(&refresh_token));
        access.create().exec(&mut tx).await.map_err(auth_storage)?;
        tx.commit().await.map_err(auth_storage)?;
        Ok(LoginOutput { access_token, refresh_token })
    }

    pub async fn logout(&self, command: LogoutCommand) -> Result<(), IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        let user_id = uuid::Uuid::parse_str(&user_id)
            .map_err(|error| AuthenticationApplicationError::Storage(error.into()))?;
        AuthSession::filter(
            AuthSession::fields().refresh_hash().eq(token_hash(&command.token))
                .and(AuthSession::fields().user_id().inner().eq(user_id)),
        ).delete().exec(&mut self.db.clone()).await.map_err(auth_storage)
    }

    pub async fn refresh_access_token(&self, command: RefreshAccessTokenCommand) -> Result<RefreshAccessTokenOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_refresh_token(&command.token)?;
        User::get_by_id(&mut self.db.clone(), &UserId::parse(&user_id)?)
            .await.map_result()
            .map_err(|_| AuthenticationApplicationError::InvalidToken)?;

        let access_token = self.crypto.issue_access_token(user_id.clone())?;
        let mut db = self.db.clone();
        let mut tx = db.transaction().await.map_err(auth_storage)?;
        let session = AuthSession::filter(
            AuthSession::fields().refresh_hash().eq(token_hash(&command.token))
                .and(AuthSession::fields().user_id().inner().eq(
                    uuid::Uuid::parse_str(&user_id)
                        .map_err(|error| AuthenticationApplicationError::Storage(error.into()))?,
                )),
        ).first().exec(&mut tx).await.map_err(auth_storage)?;
        if session.is_none() {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        let access = AuthAccessToken::new(token_hash(&access_token), token_hash(&command.token));
        access.create().exec(&mut tx).await.map_err(auth_storage)?;
        tx.commit().await.map_err(auth_storage)?;
        Ok(RefreshAccessTokenOutput { access_token })
    }

    pub async fn verify_access_token(&self, command: VerifyAccessToeknCommand) -> Result<VerifyAcessToeknOutput, IdentityApplicationError> {
        let user_id = self.crypto.verify_access_token(&command.token)?;
        let mut db = self.db.clone();
        let Some(access) = AuthAccessToken::filter(AuthAccessToken::fields().access_hash().eq(token_hash(&command.token)))
            .first().exec(&mut db).await.map_err(auth_storage)? else {
                return Err(AuthenticationApplicationError::InvalidToken.into());
            };
        let session = AuthSession::filter(
            AuthSession::fields().refresh_hash().eq(access.refresh_hash)
                .and(AuthSession::fields().user_id().inner().eq(
                    uuid::Uuid::parse_str(&user_id)
                        .map_err(|error| AuthenticationApplicationError::Storage(error.into()))?,
                )),
        ).first().exec(&mut db).await.map_err(auth_storage)?;
        if session.is_none() {
            return Err(AuthenticationApplicationError::InvalidToken.into());
        }
        Ok(VerifyAcessToeknOutput { user_id })
    }
}

fn auth_storage(error: toasty::Error) -> IdentityApplicationError {
    AuthenticationApplicationError::Storage(error.into()).into()
}

fn token_hash(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}
