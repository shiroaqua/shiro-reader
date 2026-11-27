use std::future::Future;

use shiro_domain::identity::{
    session::{AuthAccessToken, AuthSession, TokenHash},
    user::{
        entity::{User, UserChange},
        value_objects::{UserId, UserName},
    },
};

use crate::identity::crypto::errors::CryptoApplicationError;

#[derive(Debug, thiserror::Error)]
pub enum UserRepositoryError {
    #[error("user not found")]
    NotFound,

    #[error("user name conflict")]
    NameConflict,

    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

pub trait UserRepository: Clone + Send + Sync + 'static {
    type Session: UserSession + Send;

    fn session(&self) -> Self::Session;

    fn create(&self, user: User) -> impl Future<Output = Result<User, UserRepositoryError>> + Send;

    fn delete(&self, user: User) -> impl Future<Output = Result<(), UserRepositoryError>> + Send;

    fn find_by_id(
        &self,
        id: &UserId,
    ) -> impl Future<Output = Result<User, UserRepositoryError>> + Send;

    fn find_by_name(
        &self,
        name: UserName,
    ) -> impl Future<Output = Result<Option<User>, UserRepositoryError>> + Send;

    fn list(
        &self,
        ids: &[UserId],
    ) -> impl Future<Output = Result<Vec<User>, UserRepositoryError>> + Send;
}

pub trait UserSession: Send {
    type Tx<'a>: UserTransaction + Send + 'a
    where
        Self: 'a;

    fn begin_transaction(
        &mut self,
    ) -> impl Future<Output = Result<Self::Tx<'_>, UserRepositoryError>> + Send;
}

pub trait UserTransaction: Send {
    fn find_by_id(
        &mut self,
        id: &UserId,
    ) -> impl Future<Output = Result<User, UserRepositoryError>> + Send;

    fn apply_user_change(
        &mut self,
        change: UserChange<'_>,
    ) -> impl Future<Output = Result<(), UserRepositoryError>> + Send;

    fn commit(self) -> impl Future<Output = Result<(), UserRepositoryError>> + Send;
}

#[derive(Debug, thiserror::Error)]
pub enum SessionRepositoryError {
    #[error(transparent)]
    Storage(#[from] anyhow::Error),
}

pub trait SessionRepository: Clone + Send + Sync + 'static {
    fn create_session(
        &self,
        session: AuthSession,
        access_token: AuthAccessToken,
    ) -> impl Future<Output = Result<(), SessionRepositoryError>> + Send;

    fn add_access_token(
        &self,
        user_id: &UserId,
        access_token: AuthAccessToken,
    ) -> impl Future<Output = Result<bool, SessionRepositoryError>> + Send;

    fn contains_access_token(
        &self,
        access_hash: TokenHash,
    ) -> impl Future<Output = Result<bool, SessionRepositoryError>> + Send;

    fn revoke(
        &self,
        user_id: &UserId,
        refresh_hash: TokenHash,
    ) -> impl Future<Output = Result<(), SessionRepositoryError>> + Send;
}

/// Port for provisioning and cleaning up per-user library databases on the server.
pub trait UserLibraryProvisioner: Send + Sync + 'static {
    fn provision_library(&self, user_id: &str) -> impl Future<Output = anyhow::Result<()>> + Send;

    fn remove_library(&self, user_id: &str) -> impl Future<Output = anyhow::Result<()>> + Send;

    fn cleanup_failed_provision(&self, user_id: &str) -> anyhow::Result<()>;
}

/// Port for password hashing and access/refresh token issuance and verification.
pub trait IdentityCrypto: Send + Sync + 'static {
    fn hash_password(&self, plaintext: &str) -> Result<String, CryptoApplicationError>;

    fn verify_password(
        &self,
        plaintext: &str,
        stored_hash: &str,
    ) -> Result<bool, CryptoApplicationError>;

    fn issue_access_token(&self, user_id: String) -> Result<String, CryptoApplicationError>;

    fn issue_refresh_token(&self, user_id: String) -> Result<String, CryptoApplicationError>;

    fn verify_access_token(&self, token: &str) -> Result<String, CryptoApplicationError>;

    fn verify_refresh_token(&self, token: &str) -> Result<String, CryptoApplicationError>;
}
