use shiro_domain::identity::user::errors::UserDomainError;
use thiserror::Error;

use crate::identity::{
    authentication::errors::AuthenticationApplicationError,
    crypto::errors::CryptoApplicationError,
    ports::{SessionRepositoryError, UserRepositoryError},
    user::errors::UserApplicationError,
};

#[derive(Debug, Error)]
pub enum IdentityApplicationError {
    #[error("User error: {0}")]
    User(#[from] UserApplicationError),

    #[error("Authentication error: {0}")]
    Authentication(#[from] AuthenticationApplicationError),

    #[error("Crypto error: {0}")]
    Crypto(#[from] CryptoApplicationError),
}

impl From<UserDomainError> for UserApplicationError {
    fn from(err: UserDomainError) -> Self {
        match err {
            UserDomainError::InvalidId => Self::InvalidId,
            UserDomainError::MissingUserName => Self::MissingUserName,
            UserDomainError::InvalidUserNameFormat => Self::InvalidUserNameFormat,
            UserDomainError::InvalidPasswordHash => Self::InvalidPasswordHash,
            UserDomainError::NameUnchanged => Self::NameUnchanged,
            UserDomainError::PasswordHashUnchanged => Self::PasswordHashUnchanged,
        }
    }
}

impl From<UserDomainError> for IdentityApplicationError {
    fn from(err: UserDomainError) -> Self {
        Self::User(err.into())
    }
}

impl From<UserRepositoryError> for UserApplicationError {
    fn from(value: UserRepositoryError) -> Self {
        match value {
            UserRepositoryError::NotFound => Self::NotFound,
            UserRepositoryError::NameConflict => Self::NameConflict,
            UserRepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}

impl From<UserRepositoryError> for IdentityApplicationError {
    fn from(error: UserRepositoryError) -> Self {
        Self::User(error.into())
    }
}

impl From<SessionRepositoryError> for AuthenticationApplicationError {
    fn from(value: SessionRepositoryError) -> Self {
        match value {
            SessionRepositoryError::Storage(error) => Self::Storage(error),
        }
    }
}

impl From<SessionRepositoryError> for IdentityApplicationError {
    fn from(error: SessionRepositoryError) -> Self {
        Self::Authentication(error.into())
    }
}
